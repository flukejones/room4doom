use std::alloc::{alloc, dealloc, Layout};
use std::fmt::{self, Debug};
use std::mem::{align_of, size_of};
use std::ptr::{self, null_mut};

use log::{debug, error};

use crate::env::ceiling::CeilingMove;
use crate::env::doors::VerticalDoor;
use crate::env::floor::FloorMove;
use crate::env::lights::{FireFlicker, Glow, LightFlash, StrobeFlash};
use crate::env::platforms::Platform;
use crate::level::Level;
use crate::thing::MapObject;

#[derive(PartialEq, PartialOrd)]
pub struct TestObject {
    pub x: u32,
    pub thinker: *mut Thinker,
}

impl Think for TestObject {
    fn think(thinker: &mut Thinker, _level: &mut Level) -> bool {
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
        unsafe { &mut *self.thinker }
    }

    fn thinker(&self) -> &Thinker {
        unsafe { &*self.thinker }
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
        unsafe {
            for idx in 0..self.capacity {
                self.drop_item(idx);
            }
            let size = self.capacity * size_of::<Thinker>();
            let layout = Layout::from_size_align_unchecked(size, align_of::<Thinker>());
            dealloc(self.buf_ptr as *mut _, layout);
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
    /// Once allocated the owner of this `ThinkerAlloc` must not move.
    pub(crate) unsafe fn new(capacity: usize) -> Self {
        let size = capacity * size_of::<Thinker>();
        let layout = Layout::from_size_align_unchecked(size, align_of::<Thinker>());
        let buf_ptr = alloc(layout) as *mut Thinker;

        // Need to initialise everything to a blank slate
        for n in 0..capacity {
            buf_ptr.add(n).write(Thinker {
                prev: null_mut(),
                next: null_mut(),
                data: ThinkerData::Free,
                func: Thinker::placeholder,
            })
        }

        Self {
            buf_ptr,
            capacity,
            len: 0,
            next_free: buf_ptr,
            head: null_mut(),
        }
    }

    pub unsafe fn run_thinkers(&mut self, level: &mut Level) {
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

    /// Returns false if `func` returns false
    pub fn run_fn_on_things(&mut self, mut func: impl FnMut(&mut Thinker) -> bool) -> bool {
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
    pub(crate) fn find_thinker<F>(&self, finder: F) -> Option<&mut Thinker>
    where
        F: Fn(&Thinker) -> bool,
    {
        let mut current = unsafe { &mut *self.head };
        let mut next;

        loop {
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
    pub(crate) fn find_thinker_mut<F>(&self, mut finder: F) -> Option<&mut Thinker>
    where
        F: FnMut(&mut Thinker) -> bool,
    {
        let mut current = unsafe { &mut *self.head };
        let mut next;

        loop {
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
        debug_assert!(idx < self.capacity);
        let ptr = self.ptr_for_idx(idx);
        if std::mem::needs_drop::<Thinker>() {
            ptr::drop_in_place(ptr);
        }
    }

    fn ptr_for_idx(&self, idx: usize) -> *mut Thinker {
        unsafe { self.buf_ptr.add(idx) }
    }

    /// `final_loop` should always be false when called the first time
    fn find_first_free(&mut self, mut final_loop: bool) -> Option<*mut Thinker> {
        if self.len >= self.capacity {
            return None;
        }

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
    pub(crate) fn push<T: Think>(&mut self, thinker: Thinker) -> Option<&mut Thinker> {
        if self.len == self.capacity {
            return None;
        }
        if matches!(thinker.data, ThinkerData::Free) {
            panic!("Can't push a thinker with `Thinker::Free` as the inner data. Please use `Thinker::Remove` to initiate removal, or use `remove()`");
        }

        let root_ptr = self.find_first_free(false)?;
        debug!("Pushing: {:?}", root_ptr);
        match &thinker.data {
            ThinkerData::MapObject(mobj) => {
                debug!("Adding Thinker of type {:?}", mobj.kind);
            }
            _ => debug!("Adding Thinker of type {:?}", thinker.data),
        }
        unsafe { ptr::write(root_ptr, thinker) };
        let current = unsafe { &mut *root_ptr };

        if self.head.is_null() {
            self.head = root_ptr;
            let head = unsafe { &mut *self.head };
            head.prev = head;
            head.next = head;
        } else {
            let head = unsafe { &mut *self.head };
            unsafe {
                (*head.prev).next = current;
            }
            current.next = head;
            current.prev = head.prev;
            head.prev = current;
        }

        current.set_obj_thinker_ptr();
        self.len += 1;
        self.next_free = unsafe { self.next_free.add(1) };
        unsafe { Some(&mut *root_ptr) }
    }

    /// Ensure head is null if the pool is zero length
    fn maybe_reset_head(&mut self) {
        if self.len == 0 {
            self.head = null_mut();
        }
    }

    /// Removes the entry at index. Sets both func + object to None values to
    /// indicate the slot is "empty".
    pub(crate) fn remove(&mut self, thinker: &mut Thinker) {
        debug!("Removing Thinker: {:?}", thinker);
        unsafe {
            if ptr::eq(thinker, self.head) {
                self.head = thinker.next;
            }
            (*thinker.next).prev = thinker.prev;
            (*thinker.prev).next = thinker.next;

            self.len -= 1;
            self.next_free = thinker; // reuse the slot on next insert
            self.maybe_reset_head();

            ptr::write(
                thinker,
                Thinker {
                    prev: null_mut(),
                    next: null_mut(),
                    data: ThinkerData::Free,
                    func: TestObject::think,
                },
            );
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
    VerticalDoor(VerticalDoor),
    FloorMove(FloorMove),
    CeilingMove(CeilingMove),
    Platform(Platform),
    LightFlash(LightFlash),
    StrobeFlash(StrobeFlash),
    FireFlicker(FireFlicker),
    Glow(Glow),
    /// The thinker function should set to this when the linked-list node
    /// and memory is no-longer required. On thinker run it will be set to
    /// `Free` and unlinked.
    Remove,
    /// Used to mark a `ThinkerAlloc` slot as free to be re-used.
    Free,
}

impl Debug for ThinkerData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TestObject(_) => f.debug_tuple("Test").finish(),
            Self::MapObject(_) => f.debug_tuple("Mobj").finish(),
            Self::VerticalDoor(_) => f.debug_tuple("VDoor").finish(),
            Self::FloorMove(_) => f.debug_tuple("FloorMove").finish(),
            Self::CeilingMove(_) => f.debug_tuple("CeilingMove").finish(),
            Self::Platform(_) => f.debug_tuple("Platform").finish(),
            Self::LightFlash(_) => f.debug_tuple("LightFlash").finish(),
            Self::StrobeFlash(_) => f.debug_tuple("StrobeFlash").finish(),
            Self::FireFlicker(_) => f.debug_tuple("FireFlicker").finish(),
            Self::Glow(_) => f.debug_tuple("Glow").finish(),
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
    prev: *mut Thinker,
    next: *mut Thinker,
    data: ThinkerData,
    func: fn(&mut Self, &mut Level) -> bool,
}

impl Thinker {
    pub fn should_remove(&self) -> bool {
        matches!(self.data, ThinkerData::Remove)
    }

    pub fn mark_remove(&mut self) {
        self.data = ThinkerData::Remove;
    }

    pub fn set_action(&mut self, func: fn(&mut Thinker, &mut Level) -> bool) {
        self.func = func
    }

    pub fn set_obj_thinker_ptr(&mut self) {
        let ptr = self as *mut Self;
        match &mut self.data {
            ThinkerData::TestObject(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::MapObject(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::VerticalDoor(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::FloorMove(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::CeilingMove(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::Platform(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::LightFlash(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::StrobeFlash(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::FireFlicker(obj) => obj.set_thinker_ptr(ptr),
            ThinkerData::Glow(obj) => obj.set_thinker_ptr(ptr),
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
    pub fn think(&mut self, level: &mut Level) -> bool {
        (self.func)(self, level)
    }

    /// Empty function purely for ThinkerAlloc init
    fn placeholder(_: &mut Thinker, _: &mut Level) -> bool {
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
        if let ThinkerData::MapObject(ref obj) = self.data {
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
            panic!("ObjectType is not MapObject, was: {:?}", self);
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

    /// Get inner `VerticalDoor` data as mut. Panics if the inner is not
    /// actually `VerticalDoor`
    pub fn vdoor_mut(&mut self) -> &mut VerticalDoor {
        if let ThinkerData::VerticalDoor(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not VerticalDoor");
        }
    }

    /// Get inner `CeilingMove` data as mut. Panics if the inner is not actually
    /// `CeilingMove`
    pub fn ceiling_mut(&mut self) -> &mut CeilingMove {
        if let ThinkerData::CeilingMove(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not CeilingMove");
        }
    }

    /// Get inner `FloorMove` data as mut. Panics if the inner is not actually
    /// `FloorMove`
    pub fn floor_mut(&mut self) -> &mut FloorMove {
        if let ThinkerData::FloorMove(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not FloorMove");
        }
    }

    /// Get inner `Platform` data as mut. Panics if the inner is not actually
    /// `Platform`
    pub fn platform_mut(&mut self) -> &mut Platform {
        if let ThinkerData::Platform(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not Platform");
        }
    }

    /// Get inner `LightFlash` data as mut. Panics if the inner is not actually
    /// `LightFlash`
    pub fn light_flash_mut(&mut self) -> &mut LightFlash {
        if let ThinkerData::LightFlash(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not LightFlash");
        }
    }

    /// Get inner `StrobeFlash` data as mut. Panics if the inner is not actually
    /// `StrobeFlash`
    pub fn strobe_flash_mut(&mut self) -> &mut StrobeFlash {
        if let ThinkerData::StrobeFlash(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not StrobeFlash");
        }
    }

    /// Get inner `FireFlicker` data as mut. Panics if the inner is not actually
    /// `FireFlicker`
    pub fn fire_flick_mut(&mut self) -> &mut FireFlicker {
        if let ThinkerData::FireFlicker(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not FireFlicker");
        }
    }

    /// Get inner `Glow` data as mut. Panics if the inner is not actually `Glow`
    pub fn glow_mut(&mut self) -> &mut Glow {
        if let ThinkerData::Glow(obj) = &mut self.data {
            obj
        } else {
            panic!("ObjectType is not Glow");
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
    fn create_thinker(object: ThinkerData, func: fn(&mut Thinker, &mut Level) -> bool) -> Thinker {
        Thinker {
            prev: null_mut(),
            next: null_mut(),
            data: object,
            func,
        }
    }

    // fn set_thinker_action(&mut self, func: fn(&mut Thinker, &mut Level) -> bool)
    // {     self.thinker_mut().func = func
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
    fn think(thinker: &mut Thinker, level: &mut Level) -> bool;

    /// Implementer must store the pointer to the conatining Thinker. This
    /// method exists mostly to force remembering to actually ref the
    /// Thinker or not.
    fn set_thinker_ptr(&mut self, ptr: *mut Thinker);

    fn thinker_mut(&mut self) -> &mut Thinker;

    fn thinker(&self) -> &Thinker;
}

#[cfg(test)]
impl ThinkerData {
    fn bad_ref<T>(&self) -> &T {
        let mut ptr = self as *const Self as usize;
        ptr += size_of::<u64>();
        unsafe { &*(ptr as *const T) }
    }

    fn bad_mut<T>(&mut self) -> &mut T {
        let mut ptr = self as *mut Self as usize;
        ptr += size_of::<u64>();
        unsafe { &mut *(ptr as *mut T) }
    }
}

#[cfg(test)]
mod tests {
    use wad::WadData;

    use crate::doom_def::GameMode;
    use crate::level::map_data::MapData;
    use crate::level::Level;
    use crate::thinker::{Think, Thinker};
    use crate::{PicData, Player, Skill, MAXPLAYERS};

    use super::{TestObject, ThinkerAlloc, ThinkerData};
    use std::cell::RefCell;
    use std::ptr::null_mut;
    use std::rc::Rc;
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
        let wad = WadData::new("../doom1.wad".into());
        let mut map = MapData::new("E1M1".to_owned());
        map.load(&PicData::default(), &wad);
        let textures = PicData::init(false, &wad);
        let (tx, _rx) = channel();

        let mut l = unsafe {
            Level::new(
                Skill::Baby,
                1,
                1,
                GameMode::Shareware,
                Vec::new(),
                Rc::new(RefCell::new(textures)),
                tx,
                &[false; MAXPLAYERS],
                &mut [
                    Player::default(),
                    Player::default(),
                    Player::default(),
                    Player::default(),
                ],
                0,
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

        let ptr = &mut x as *mut Thinker;
        x.data.bad_mut::<TestObject>().set_thinker_ptr(ptr);
        assert!(x.data.bad_mut::<TestObject>().thinker_mut().think(&mut l));
    }

    #[test]
    fn allocate() {
        let links = unsafe { ThinkerAlloc::new(64) };
        assert_eq!(links.len, 0);
        assert_eq!(links.capacity, 64);
    }

    #[test]
    fn push_1() {
        let mut links = unsafe { ThinkerAlloc::new(64) };
        assert_eq!(links.len, 0);
        assert_eq!(links.capacity, 64);

        let think = links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerData::Remove,
                TestObject::think,
            ))
            .unwrap() as *mut Thinker;
        assert!(!links.head.is_null());
        assert_eq!(links.len, 1);
        unsafe {
            assert!((*links.head).should_remove());
        }

        unsafe {
            dbg!(&*links.buf_ptr.add(0));
            dbg!(&*links.buf_ptr.add(1));
            dbg!(&*links.buf_ptr.add(2));
            dbg!(&*links.buf_ptr.add(62));

            assert!(matches!((*links.buf_ptr.add(0)).data, ThinkerData::Remove));
            assert!(matches!((*links.buf_ptr.add(1)).data, ThinkerData::Free));
            assert!(matches!((*links.buf_ptr.add(2)).data, ThinkerData::Free));

            links.remove(&mut *think);
            assert_eq!(links.len, 0);
        }
    }

    #[test]
    fn check_next_prev_links() {
        let mut links = unsafe { ThinkerAlloc::new(64) };

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerData::Remove,
                TestObject::think,
            ))
            .unwrap();
        assert!(!links.head.is_null());

        let one = links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerData::TestObject(TestObject {
                    x: 666,
                    thinker: null_mut(),
                }),
                TestObject::think,
            ))
            .unwrap() as *mut Thinker;

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerData::TestObject(TestObject {
                    x: 123,
                    thinker: null_mut(),
                }),
                TestObject::think,
            ))
            .unwrap();
        let three = links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerData::TestObject(TestObject {
                    x: 333,
                    thinker: null_mut(),
                }),
                TestObject::think,
            ))
            .unwrap() as *mut Thinker;

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
        unsafe {
            links.remove(&mut *one);
            assert!((*links.head).should_remove());
            assert_eq!((*(*links.head).prev).data.bad_ref::<TestObject>().x, 333);
            assert_eq!(
                (*(*(*links.head).prev).prev).data.bad_ref::<TestObject>().x,
                123
            );
        }

        unsafe {
            links.remove(&mut *three);
            assert!((*links.head).should_remove());
            assert_eq!((*(*links.head).prev).data.bad_ref::<TestObject>().x, 123);
            assert!((*(*(*links.head).prev).prev).should_remove());
        }
    }

    // #[test]
    // fn link_iter_and_removes() {
    //     let mut links = unsafe { ThinkerAlloc::new(64) };

    //     links.push::<TestObject>(TestObject::create_thinker(
    //         ThinkerType::Test(TestObject {
    //             x: 42,
    //             thinker: null_mut(),
    //         }),
    //         ActionF::None,
    //     ));
    //     links.push::<TestObject>(TestObject::create_thinker(
    //         ThinkerType::Test(TestObject {
    //             x: 123,
    //             thinker: null_mut(),
    //         }),
    //         ActionF::None,
    //     ));
    //     links.push::<TestObject>(TestObject::create_thinker(
    //         ThinkerType::Test(TestObject {
    //             x: 666,
    //             thinker: null_mut(),
    //         }),
    //         ActionF::None,
    //     ));
    //     links.push::<TestObject>(TestObject::create_thinker(
    //         ThinkerType::Test(TestObject {
    //             x: 333,
    //             thinker: null_mut(),
    //         }),
    //         ActionF::None,
    //     ));

    //     for (i, thinker) in links.iter().enumerate() {
    //         if i == 0 {
    //             assert_eq!(thinker.object.bad_ref::<TestObject>().x, 42);
    //         }
    //         if i == 1 {
    //             assert_eq!(thinker.object.bad_ref::<TestObject>().x, 123);
    //         }
    //         if i == 2 {
    //             assert_eq!(thinker.object.bad_ref::<TestObject>().x, 666);
    //         }
    //         if i == 3 {
    //             assert_eq!(thinker.object.bad_ref::<TestObject>().x, 333);
    //         }
    //     }
    //     unsafe {
    //         assert_eq!(
    //             (*links.buf_ptr.add(3))
    //                 .as_ref()
    //                 .unwrap()
    //                 .object
    //                 .bad_ref::<TestObject>()
    //                 .x,
    //             333
    //         );
    //     }

    //     assert_eq!(links.iter().count(), 4);

    //     links.remove(3);
    //     assert_eq!(links.len(), 3);
    //     assert_eq!(links.iter().count(), 3);

    //     for (i, num) in links.iter().enumerate() {
    //         if i == 0 {
    //             assert_eq!((*num).object.bad_ref::<TestObject>().x, 42);
    //         }
    //         if i == 1 {
    //             assert_eq!((*num).object.bad_ref::<TestObject>().x, 123);
    //         }
    //         if i == 2 {
    //             assert_eq!((*num).object.bad_ref::<TestObject>().x, 666);
    //         }
    //     }
    //     //
    //     links.remove(1);
    //     assert_eq!(links.len(), 2);
    //     assert_eq!(links.iter().count(), 2);

    //     for (i, num) in links.iter().enumerate() {
    //         if i == 0 {
    //             assert_eq!((*num).object.bad_ref::<TestObject>().x, 42);
    //         }
    //         if i == 1 {
    //             assert_eq!((*num).object.bad_ref::<TestObject>().x, 666);
    //         }
    //     }
    // }
}
