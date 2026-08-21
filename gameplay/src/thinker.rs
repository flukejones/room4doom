#[cfg(feature = "hprof")]
use coarse_prof::profile;
use log::{debug, error, trace};
use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::fmt::{self, Debug};
use std::mem::needs_drop;
use std::ptr::{self, null_mut};

use crate::env::effects::{LightEffect, MoveEffect};
use crate::level::LevelState;
use crate::thing::MapObject;

#[derive(PartialEq, PartialOrd)]
pub struct TestObject {
    pub x: u32,
    pub thinker: *mut Thinker,
}

impl Think for TestObject {
    fn think(thinker: &mut Thinker, _level: &mut LevelState) -> bool {
        if let Some(test) = thinker.test_mut() {
            test.x = 1000;
            return true;
        }
        false
    }

    fn set_thinker_ptr(&mut self, ptr: *mut Thinker) {
        self.thinker = ptr;
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        // SAFETY: set_thinker_ptr installs this object's enclosing initialized pool slot before
        // it is used.
        unsafe { &mut *self.thinker }
    }
}

/// A custom allocation for `Thinker` objects. This intends to keep them in a
/// contiguous zone of memory.
pub struct ThinkerAlloc {
    /// The main AllocPool buffer
    buf_ptr: *mut Thinker,
    /// Total capacity. Not possible to allocate over this.
    capacity: usize,
    /// Actual used AllocPool
    len: usize,
    /// The next free slot to insert in
    next_free: *mut Thinker,
    head: *mut Thinker,
}

impl Drop for ThinkerAlloc {
    fn drop(&mut self) {
        // SAFETY: Every pool slot is initialized, and this uses the same checked nonzero layout
        // as allocation.
        unsafe {
            for idx in 0..self.capacity {
                self.drop_item(idx);
            }
            let layout =
                Layout::array::<Thinker>(self.capacity.max(1)).expect("valid thinker allocation");
            dealloc(self.buf_ptr.cast(), layout);
        }
    }
}

impl ThinkerAlloc {
    /// Allocate a new block of `Thinker`. On creation the entire memory block
    /// is initialised with the inner data as `ThinkerData::Free`. This
    /// means raw pointers to a `Thinker` are always valid as long as
    /// `ThinkerAlloc` never moves. Accessor methods are available to get the
    /// inner data.
    ///
    /// # Safety
    /// Keep the pool alive while any thinker or spatial link refers to its slots.
    pub(crate) unsafe fn new(capacity: usize) -> Self {
        // SAFETY: The checked nonzero layout has Thinker alignment; allocation failure is
        // handled, and writes stay within capacity.
        unsafe {
            let layout = Layout::array::<Thinker>(capacity.max(1))
                .expect("thinker capacity fits allocation layout");
            let buf_ptr = alloc(layout).cast::<Thinker>();
            if buf_ptr.is_null() {
                handle_alloc_error(layout);
            }

            // Need to initialise everything to a blank slate
            for n in 0..capacity {
                let blank: Thinker = Thinker {
                    prev: null_mut(),
                    next: null_mut(),
                    data: ThinkerData::Free,
                    func: Thinker::placeholder,
                };
                buf_ptr.add(n).write(blank);
            }

            Self {
                buf_ptr,
                capacity,
                len: 0,
                next_free: buf_ptr,
                head: null_mut(),
            }
        }
    }

    pub unsafe fn run_thinkers(&mut self, level: &mut LevelState) {
        // SAFETY: The circular links reference initialized slots in the fixed pool; traversal
        // and removal run sequentially on the game thread.
        unsafe {
            #[cfg(feature = "hprof")]
            profile!("run_thinkers");
            let mut current = &mut *self.head;
            let mut next;

            loop {
                if current.should_remove() {
                    next = &mut *current.next;
                    self.remove(&mut *current);
                } else {
                    current.think(level);
                    next = &mut *current.next;
                }
                current = next;

                if ptr::eq(current, self.head) {
                    break;
                }
            }
        }
    }

    /// Returns false if `func` returns false
    pub fn run_fn_on_things(&mut self, mut func: impl FnMut(&mut Thinker) -> bool) -> bool {
        // SAFETY: The circular links reference initialized slots in the fixed pool; traversal
        // and removal run sequentially on the game thread.
        unsafe {
            let mut current = &mut *self.head;
            let mut next;

            loop {
                if current.should_remove() {
                    next = &mut *current.next;
                } else {
                    next = &mut *current.next;
                    if !func(&mut *current) {
                        return false;
                    }
                }
                current = next;

                if ptr::eq(current, self.head) {
                    break;
                }
            }
        }
        true
    }

    /// Iterates through the list of thinkers until either the closure returns
    /// true or the end is reached.
    #[allow(clippy::mut_from_ref)] // raw-pointer intrusive-list traversal
    pub(crate) fn find_thinker<F>(&self, finder: F) -> Option<&mut Thinker>
    where
        F: Fn(&Thinker) -> bool,
    {
        // SAFETY: The circular links reference initialized slots in the fixed pool; traversal
        // and removal run sequentially on the game thread.
        let mut current = unsafe { &mut *self.head };
        let mut next;

        loop {
            // SAFETY: The circular links reference initialized slots in the fixed pool;
            // traversal and removal run sequentially on the game thread.
            unsafe {
                if finder(current) {
                    return Some(current);
                }
                next = &mut *current.next;
            }
            current = next;

            if ptr::eq(current, self.head) {
                return None;
            }
        }
    }

    /// Iterates through the list of thinkers until either the closure returns
    /// true or the end is reached.
    ///
    /// # This is worse than the `find_thinker()` as there can be side effects
    #[allow(clippy::mut_from_ref)] // raw-pointer intrusive-list traversal
    pub(crate) fn find_thinker_mut<F>(&self, mut finder: F) -> Option<&mut Thinker>
    where
        F: FnMut(&mut Thinker) -> bool,
    {
        // SAFETY: The circular links reference initialized slots in the fixed pool; traversal
        // and removal run sequentially on the game thread.
        let mut current = unsafe { &mut *self.head };
        let mut next;

        loop {
            // SAFETY: The circular links reference initialized slots in the fixed pool;
            // traversal and removal run sequentially on the game thread.
            unsafe {
                if finder(current) {
                    return Some(current);
                }
                next = &mut *current.next;
            }
            current = next;

            if ptr::eq(current, self.head) {
                return None;
            }
        }
    }

    unsafe fn drop_item(&mut self, idx: usize) {
        debug_assert!(idx < self.capacity, "idx out of bounds");
        let ptr = self.ptr_for_idx(idx);
        if needs_drop::<Thinker>() {
            // SAFETY: The index/free-slot scan stays inside the fixed allocation; the one-past
            // pointer is used only for traversal bounds.
            unsafe {
                ptr::drop_in_place(ptr);
            }
        }
    }

    fn ptr_for_idx(&self, idx: usize) -> *mut Thinker {
        // SAFETY: The index/free-slot scan stays inside the fixed allocation; the one-past
        // pointer is used only for traversal bounds.
        unsafe { self.buf_ptr.add(idx) }
    }

    /// `final_loop` should always be false when called the first time
    fn find_first_free(&mut self, mut final_loop: bool) -> Option<*mut Thinker> {
        if self.len >= self.capacity {
            return None;
        }

        // SAFETY: The index/free-slot scan stays inside the fixed allocation; the one-past
        // pointer is used only for traversal bounds.
        unsafe {
            let max = self.buf_ptr.add(self.capacity - 1);
            loop {
                if self.next_free == max {
                    break;
                }
                if matches!((*self.next_free).data, ThinkerData::Free) {
                    return Some(self.next_free);
                }
                self.next_free = self.next_free.add(1);
            }
        }
        if !final_loop {
            final_loop = true;
            self.next_free = self.buf_ptr;
            return self.find_first_free(final_loop);
        }

        panic!("No more thinker slots");
    }

    /// Push a Thinker to the `ThinkerAlloc`. Returns a mutable ref to the
    /// Thinker.
    pub(crate) fn push<Th: Think>(&mut self, thinker: Thinker) -> Option<&mut Thinker> {
        if self.len == self.capacity {
            return None;
        }
        if matches!(thinker.data, ThinkerData::Free) {
            panic!(
                "Can't push a thinker with `Thinker::Free` as the inner data. Please use `Thinker::Remove` to initiate removal, or use `remove()`"
            );
        }

        let root_ptr = self.find_first_free(false)?;
        trace!("Pushing: {root_ptr:?}");
        match &thinker.data {
            ThinkerData::MapObject(mobj) => {
                trace!("Adding Thinker of type {:?}", mobj.kind);
            }
            _ => trace!("Adding Thinker of type {:?}", thinker.data),
        }
        // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized before
        // linking, and the list links remain in the pool.
        unsafe { ptr::write(root_ptr, thinker) };
        // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized before
        // linking, and the list links remain in the pool.
        let current = unsafe { &mut *root_ptr };

        if self.head.is_null() {
            self.head = root_ptr;
            // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized
            // before linking, and the list links remain in the pool.
            let head = unsafe { &mut *self.head };
            head.prev = head;
            head.next = head;
        } else {
            // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized
            // before linking, and the list links remain in the pool.
            let head = unsafe { &mut *self.head };
            // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized
            // before linking, and the list links remain in the pool.
            unsafe {
                (*head.prev).next = current;
            }
            current.next = head;
            current.prev = head.prev;
            head.prev = current;
        }

        current.set_obj_thinker_ptr();
        self.len += 1;
        // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized before
        // linking, and the list links remain in the pool.
        self.next_free = unsafe { self.next_free.add(1) };
        // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized before
        // linking, and the list links remain in the pool.
        unsafe { Some(&mut *root_ptr) }
    }

    /// Ensure head is null if the pool is zero length
    fn maybe_reset_head(&mut self) {
        if self.len == 0 {
            self.head = null_mut();
        }
    }

    /// Walk the live thinker list, calling `f` on each non-Free/non-Remove
    /// thinker.
    pub fn for_each<F: FnMut(&Thinker)>(&self, mut f: F) {
        if self.head.is_null() {
            return;
        }
        // SAFETY: The circular links reference initialized slots in the fixed pool; traversal
        // and removal run sequentially on the game thread.
        unsafe {
            let mut current = &*self.head;
            loop {
                if !matches!(current.data, ThinkerData::Free | ThinkerData::Remove) {
                    f(current);
                }
                current = &*current.next;
                if ptr::eq(current, self.head) {
                    break;
                }
            }
        }
    }

    /// Mutable variant of `for_each`.
    pub fn for_each_mut<F: FnMut(&mut Thinker)>(&mut self, mut f: F) {
        if self.head.is_null() {
            return;
        }
        // SAFETY: The circular links reference initialized slots in the fixed pool; traversal
        // and removal run sequentially on the game thread.
        unsafe {
            let mut current = self.head;
            loop {
                let next = (*current).next;
                if !matches!((*current).data, ThinkerData::Free | ThinkerData::Remove) {
                    f(&mut *current);
                }
                current = next;
                if ptr::eq(current, self.head) {
                    break;
                }
            }
        }
    }

    /// Reset the allocator to empty state. All thinker slots become `Free`,
    /// the linked list is cleared, and the free pointer is reset.
    pub fn clear(&mut self) {
        // SAFETY: The circular links reference initialized slots in the fixed pool; traversal
        // and removal run sequentially on the game thread.
        unsafe {
            for idx in 0..self.capacity {
                let ptr = self.ptr_for_idx(idx);
                let blank: Thinker = Thinker {
                    prev: null_mut(),
                    next: null_mut(),
                    data: ThinkerData::Free,
                    func: Thinker::placeholder,
                };
                ptr::write(ptr, blank);
            }
        }
        self.head = null_mut();
        self.len = 0;
        self.next_free = self.buf_ptr;
    }

    /// Push a pre-built `Thinker` directly. Unlike `push`, this does not
    /// require a type param — used by the save/load system.
    pub fn push_raw(&mut self, thinker: Thinker) -> Option<&mut Thinker> {
        if self.len == self.capacity {
            return None;
        }
        if matches!(thinker.data, ThinkerData::Free) {
            panic!("Can't push a thinker with `Thinker::Free`");
        }

        let root_ptr = self.find_first_free(false)?;
        // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized before
        // linking, and the list links remain in the pool.
        unsafe { ptr::write(root_ptr, thinker) };
        // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized before
        // linking, and the list links remain in the pool.
        let current = unsafe { &mut *root_ptr };

        if self.head.is_null() {
            self.head = root_ptr;
            // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized
            // before linking, and the list links remain in the pool.
            let head = unsafe { &mut *self.head };
            head.prev = head;
            head.next = head;
        } else {
            // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized
            // before linking, and the list links remain in the pool.
            let head = unsafe { &mut *self.head };
            // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized
            // before linking, and the list links remain in the pool.
            unsafe {
                (*head.prev).next = current;
            }
            current.next = head;
            current.prev = head.prev;
            head.prev = current;
        }

        current.set_obj_thinker_ptr();
        self.len += 1;
        // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized before
        // linking, and the list links remain in the pool.
        self.next_free = unsafe { self.next_free.add(1) };
        // SAFETY: find_first_free returns a slot in this fixed pool; it is initialized before
        // linking, and the list links remain in the pool.
        unsafe { Some(&mut *root_ptr) }
    }

    /// Removes the entry at index. Sets both func + object to None values to
    /// indicate the slot is "empty".
    pub(crate) fn remove(&mut self, thinker: &mut Thinker) {
        debug!("Removing Thinker: {thinker:?}");
        // SAFETY: The circular links reference initialized slots in the fixed pool; traversal
        // and removal run sequentially on the game thread.
        unsafe {
            if ptr::eq(thinker, self.head) {
                self.head = thinker.next;
            }
            (*thinker.next).prev = thinker.prev;
            (*thinker.prev).next = thinker.next;

            self.len -= 1;
            self.next_free = thinker; // reuse the slot on next insert
            self.maybe_reset_head();

            let blank: Thinker = Thinker {
                prev: null_mut(),
                next: null_mut(),
                data: ThinkerData::Free,
                func: Thinker::placeholder,
            };
            ptr::write(thinker, blank);
        }
    }
}

/// All map object thinkers need to be registered here. If the object has
/// pointees then these must be dealt with before setting `ObjectType::Remove`.
#[repr(C)]
#[allow(clippy::large_enum_variant)]
pub enum ThinkerData {
    TestObject(TestObject),
    MapObject(MapObject),
    /// A sector surface in motion: door, floor, ceiling, crusher or lift.
    Move(MoveEffect),
    /// An animated sector light level.
    Light(LightEffect),
    /// The thinker function should set to this when the linked-list node
    /// and memory is no-longer required. On thinker run it will be set to
    /// `Free` and unlinked.
    Remove,
    /// An empty `ThinkerAlloc` slot, available for the next insert.
    Free,
}

impl Debug for ThinkerData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TestObject(_) => f.debug_tuple("Test").finish(),
            Self::MapObject(_) => f.debug_tuple("Mobj").finish(),
            Self::Move(_) => f.debug_tuple("Move").finish(),
            Self::Light(_) => f.debug_tuple("Light").finish(),
            Self::Remove => f.debug_tuple("Remove").finish(),
            Self::Free => f.debug_tuple("Free - this shouldn't ever be seen").finish(),
        }
    }
}

/// Thinkers *must* be contained in a structure that has **stable** memory
/// locations. In Doom this is managed by Doom's custom allocator `z_malloc`,
/// where each location in memory also has a pointer to the locations 'owner'.
/// When Doom does a defrag or any op that moves memory locations it also runs
/// through the owners and updates their pointers. This isn't done in the Rust
/// version as that introduces a lot of overhead and makes various things harder
/// to do or harder to prove correct (if using unsafe).
///
/// Another way to manager Thinkers in a volatile container like a Vec is to use
/// `self.function` to mark for removal (same as Doom), then iterate over the
/// container and only run thinkers not marked for removal, then remove marked
/// thinkers after cycle. This method would have a big impact on iter speed
/// though as there may be many 'dead' thinkers and it would also impact the
/// order of thinkers, which then means recorded demo playback may be quite
/// different to OG Doom.
///
/// Inserting the `Thinker` in to the game-exe is done in p_tick.c with
/// `P_RunThinkers`.
///
/// The LinkedList style serves to give the Objects a way to find the next/prev
/// of its neighbours and more, without having to pass in a ref to the Thinker
/// container, or iterate over possible blank spots in memory.
pub struct Thinker {
    prev: *mut Self,
    next: *mut Self,
    data: ThinkerData,
    func: fn(&mut Self, &mut LevelState) -> bool,
}

impl Thinker {
    pub fn should_remove(&self) -> bool {
        matches!(self.data, ThinkerData::Remove)
    }

    pub fn mark_remove(&mut self) {
        self.data = ThinkerData::Remove;
    }

    /// Cast an erased `*mut Thinker` back to `&mut Thinker`.
    pub unsafe fn from_erased<'a>(ptr: *mut Self) -> &'a mut Self {
        // SAFETY: The caller supplies a live aligned Thinker pointer and guarantees the
        // returned reference lifetime and access exclusivity.
        unsafe { &mut *ptr }
    }

    /// Immutable variant of [`from_erased`].
    pub unsafe fn from_erased_ref<'a>(ptr: *mut Self) -> &'a Self {
        // SAFETY: The caller supplies a live aligned Thinker pointer and guarantees the
        // returned reference lifetime and access exclusivity.
        unsafe { &*ptr }
    }

    pub fn set_action(&mut self, func: fn(&mut Self, &mut LevelState) -> bool) {
        self.func = func;
    }

    pub fn set_obj_thinker_ptr(&mut self) {
        let ptr = ptr::from_mut(self);
        match &mut self.data {
            ThinkerData::TestObject(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::MapObject(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::Move(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::Light(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::Remove => {
                error!("Tried to set the Thinker pointer for an Object that was 'Remove'");
            }
            ThinkerData::Free => {
                error!("Tried to set the Thinker pointer for an Object that was 'Free'");
            }
        }
    }

    /// Run the `ThinkerType`'s `think()`. If the `think()` returns false then
    /// it should be marked for removal
    pub fn think(&mut self, level: &mut LevelState) -> bool {
        (self.func)(self, level)
    }

    /// Empty function purely for ThinkerAlloc init
    fn placeholder(_: &mut Self, _: &mut LevelState) -> bool {
        false
    }

    pub fn data(&self) -> &ThinkerData {
        &self.data
    }

    pub fn is_mobj(&self) -> bool {
        matches!(self.data, ThinkerData::MapObject(_))
    }

    /// Get inner `MapObject` data as ref. Panics if the inner is not actually
    /// `MapObject`
    pub fn mobj(&self) -> &MapObject {
        if let ThinkerData::MapObject(obj) = &self.data {
            obj
        } else {
            panic!("ObjectType is not MapObject");
        }
    }

    /// Get inner `MapObject` data as mut. Panics if the inner is not actually
    /// `MapObject`
    pub fn mobj_mut(&mut self) -> &mut MapObject {
        if let ThinkerData::MapObject(ref mut obj) = self.data {
            obj
        } else {
            panic!("ObjectType is not MapObject, was: {self:?}");
        }
    }

    /// Get inner `TestObject` data as mut. Panics if the inner is not actually
    /// `TestObject`
    pub fn test_mut(&mut self) -> Option<&mut TestObject> {
        if let ThinkerData::TestObject(obj) = &mut self.data {
            Some(obj)
        } else {
            error!("ObjectType is not TestObject");
            None
        }
    }

    /// Get inner `MoveEffect` data as mut. Panics if the inner is not actually
    /// `MoveEffect`
    pub fn mover_mut(&mut self) -> &mut MoveEffect {
        if let ThinkerData::Move(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not MoveEffect");
        }
    }

    /// Get inner `LightEffect` data as mut. Panics if the inner is not actually
    /// `LightEffect`
    pub fn light_mut(&mut self) -> &mut LightEffect {
        if let ThinkerData::Light(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not LightEffect");
        }
    }
}

impl Debug for Thinker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Thinker")
            .field("prev", &(self.prev))
            .field("next", &(self.next))
            .field("data", &(self.data))
            .finish_non_exhaustive()
    }
}

/// Every map object should implement this trait
pub(crate) trait Think {
    /// Creating a thinker should be the last step in new objects as `Thinker`
    /// takes ownership
    fn create_thinker(
        object: ThinkerData,
        func: fn(&mut Thinker, &mut LevelState) -> bool,
    ) -> Thinker {
        Thinker {
            prev: null_mut(),
            next: null_mut(),
            data: object,
            func,
        }
    }

    // fn set_thinker_action(&mut self, func: fn(&mut Thinker, &mut LevelState) ->
    // bool) {     self.thinker_mut().func = func
    // }

    /// impl of this trait function should return true *if* the thinker + object
    /// are to be removed
    ///
    /// Functionally this is Acp1, but in Doom when used with a Thinker it calls
    /// only one function on the object and Null is used to track if the map
    /// object should be removed.
    ///
    /// **NOTE:**
    ///
    /// The impl of `think()` on type will need to cast `ThinkerType` with
    /// `object.bad_mut()`.
    fn think(thinker: &mut Thinker, level: &mut LevelState) -> bool;

    /// Implementer must store the pointer to the conatining Thinker. This
    /// method exists mostly to force remembering to actually ref the
    /// Thinker or not.
    fn set_thinker_ptr(&mut self, ptr: *mut Thinker);

    fn thinker_mut(&mut self) -> &mut Thinker;
}

#[cfg(test)]
impl ThinkerData {
    fn bad_ref<T>(&self) -> &T {
        let mut ptr = ptr::from_ref(self) as usize;
        ptr += size_of::<u64>();
        // SAFETY: This test helper is called only for the selected repr(u64) payload type; the
        // tag occupies the first eight bytes.
        unsafe { &*(ptr as *const T) }
    }

    fn bad_mut<T>(&mut self) -> &mut T {
        let mut ptr = ptr::from_mut(self) as usize;
        ptr += size_of::<u64>();
        // SAFETY: This test helper is called only for the selected repr(u64) payload type; the
        // tag occupies the first eight bytes.
        unsafe { &mut *(ptr as *mut T) }
    }
}

#[cfg(test)]
mod tests {
    use test_utils::doom1_wad_path;
    use wad::WadData;

    use crate::level::LevelState;
    use crate::thinker::{Think as _, Thinker};
    use crate::{MAXPLAYERS, Player};
    use game_config::GameMode;
    use level::LevelData;

    use super::{TestObject, ThinkerAlloc, ThinkerData};
    use std::ptr::{self, null_mut};
    use std::sync::mpsc::channel;

    #[test]
    fn bad_stuff() {
        let mut x = ThinkerData::TestObject(TestObject {
            x: 42,
            thinker: null_mut(),
        });

        if let ThinkerData::TestObject(f) = &x {
            assert_eq!(f.x, 42);

            let f = x.bad_ref::<TestObject>();
            assert_eq!(f.x, 42);

            assert_eq!(x.bad_mut::<TestObject>().x, 42);

            x.bad_mut::<TestObject>().x = 55;
            assert_eq!(x.bad_mut::<TestObject>().x, 55);
        }
    }

    #[test]
    fn bad_stuff_thinking() {
        let wad = WadData::new(&doom1_wad_path());
        let mut map = LevelData::default();
        map.load("E1M1", |_| None, &wad, None, None);
        let (tx, _rx) = channel();

        // SAFETY: The test keeps its local level/pool alive and inspects only slots initialized
        // by that pool.
        let mut l = unsafe {
            LevelState::new_empty(
                game_config::GameOptions::default(),
                GameMode::Shareware,
                tx,
                &[false; MAXPLAYERS],
                &mut [
                    Player::default(),
                    Player::default(),
                    Player::default(),
                    Player::default(),
                ],
            )
        };
        let mut x = Thinker {
            prev: null_mut(),
            next: null_mut(),
            data: ThinkerData::TestObject(TestObject {
                x: 42,
                thinker: null_mut(),
            }),
            func: TestObject::think,
        };

        assert!(x.think(&mut l));

        let ptr = ptr::from_mut(&mut x);
        x.data.bad_mut::<TestObject>().set_thinker_ptr(ptr);
        assert!(x.data.bad_mut::<TestObject>().thinker_mut().think(&mut l));
    }

    #[test]
    fn allocate() {
        // SAFETY: The test keeps its local level/pool alive and inspects only slots initialized
        // by that pool.
        let links = unsafe { ThinkerAlloc::new(64) };
        assert_eq!(links.len, 0);
        assert_eq!(links.capacity, 64);
    }

    #[test]
    fn empty_pool_has_valid_allocation_and_no_live_entries() {
        // SAFETY: No raw entry links escape this local pool.
        let mut pool = unsafe { ThinkerAlloc::new(0) };
        assert!(!pool.buf_ptr.is_null());
        assert_eq!(pool.len, 0);
        pool.for_each(|_| panic!("empty pool has no entries"));
        pool.for_each_mut(|_| panic!("empty pool has no entries"));
    }

    #[test]
    fn push_1() {
        // SAFETY: The test keeps its local level/pool alive and inspects only slots initialized
        // by that pool.
        let mut links = unsafe { ThinkerAlloc::new(64) };
        assert_eq!(links.len, 0);
        assert_eq!(links.capacity, 64);

        let think = ptr::from_mut(
            links
                .push::<TestObject>(TestObject::create_thinker(
                    ThinkerData::Remove,
                    TestObject::think,
                ))
                .unwrap(),
        );
        assert!(!links.head.is_null());
        assert_eq!(links.len, 1);
        // SAFETY: The test keeps its local level/pool alive and inspects only slots initialized
        // by that pool.
        unsafe {
            assert!((*links.head).should_remove());
        }

        // SAFETY: The test keeps its local level/pool alive and inspects only slots initialized
        // by that pool.
        unsafe {
            assert!(matches!((*links.buf_ptr.add(0)).data, ThinkerData::Remove));
            assert!(matches!((*links.buf_ptr.add(1)).data, ThinkerData::Free));
            assert!(matches!((*links.buf_ptr.add(2)).data, ThinkerData::Free));

            links.remove(&mut *think);
            assert_eq!(links.len, 0);
        }
    }

    #[test]
    fn check_next_prev_links() {
        // SAFETY: The test keeps its local level/pool alive and inspects only slots initialized
        // by that pool.
        let mut links = unsafe { ThinkerAlloc::new(64) };

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerData::Remove,
                TestObject::think,
            ))
            .unwrap();
        assert!(!links.head.is_null());

        let one = ptr::from_mut(
            links
                .push::<TestObject>(TestObject::create_thinker(
                    ThinkerData::TestObject(TestObject {
                        x: 666,
                        thinker: null_mut(),
                    }),
                    TestObject::think,
                ))
                .unwrap(),
        );

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerData::TestObject(TestObject {
                    x: 123,
                    thinker: null_mut(),
                }),
                TestObject::think,
            ))
            .unwrap();
        let three = ptr::from_mut(
            links
                .push::<TestObject>(TestObject::create_thinker(
                    ThinkerData::TestObject(TestObject {
                        x: 333,
                        thinker: null_mut(),
                    }),
                    TestObject::think,
                ))
                .unwrap(),
        );

        // SAFETY: The test keeps its local level/pool alive and inspects only slots initialized
        // by that pool.
        unsafe {
            // forward
            assert!((*links.buf_ptr).should_remove());
            assert_eq!((*(*links.buf_ptr).next).data.bad_ref::<TestObject>().x, 666);
            assert_eq!(
                (*(*(*links.buf_ptr).next).next)
                    .data
                    .bad_ref::<TestObject>()
                    .x,
                123
            );
            assert_eq!(
                (*(*(*(*links.buf_ptr).next).next).next)
                    .data
                    .bad_ref::<TestObject>()
                    .x,
                333
            );
            assert!((*(*(*(*(*links.buf_ptr).next).next).next).next).should_remove());
            // back
            assert!((*links.head).should_remove());
            assert_eq!((*(*links.head).prev).data.bad_ref::<TestObject>().x, 333);
            assert_eq!(
                (*(*(*links.head).prev).prev).data.bad_ref::<TestObject>().x,
                123
            );
            assert_eq!(
                (*(*(*(*links.head).prev).prev).prev)
                    .data
                    .bad_ref::<TestObject>()
                    .x,
                666
            );
        }
        // SAFETY: The test keeps its local level/pool alive and inspects only slots initialized
        // by that pool.
        unsafe {
            links.remove(&mut *one);
            assert!((*links.head).should_remove());
            assert_eq!((*(*links.head).prev).data.bad_ref::<TestObject>().x, 333);
            assert_eq!(
                (*(*(*links.head).prev).prev).data.bad_ref::<TestObject>().x,
                123
            );
        }

        // SAFETY: The test keeps its local level/pool alive and inspects only slots initialized
        // by that pool.
        unsafe {
            links.remove(&mut *three);
            assert!((*links.head).should_remove());
            assert_eq!((*(*links.head).prev).data.bad_ref::<TestObject>().x, 123);
            assert!((*(*(*links.head).prev).prev).should_remove());
        }
    }
}
