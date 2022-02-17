use std::alloc::{alloc, dealloc, Layout};
use std::fmt::{self, Debug};
use std::marker::PhantomData;
use std::mem::{align_of, size_of};
use std::ptr::{self, null_mut, NonNull};

use log::error;

use crate::level_data::level::Level;
use crate::p_ceiling::CeilingMove;
use crate::p_doors::VerticalDoor;
use crate::p_floor::FloorMove;
use crate::p_lights::{FireFlicker, Glow, LightFlash, StrobeFlash};
use crate::p_map_object::MapObject;
use crate::p_platforms::Platform;
use crate::p_player_sprite::PspDef;
use crate::player::Player;

#[derive(PartialEq, PartialOrd)]
pub struct TestObject {
    pub x: u32,
    pub thinker: NonNull<Thinker>,
}

impl Think for TestObject {
    fn think(object: &mut ThinkerType, _level: &mut Level) -> bool {
        let this = object.bad_mut::<TestObject>();
        this.x = 1000;
        true
    }

    fn set_thinker_ptr(&mut self, ptr: std::ptr::NonNull<Thinker>) {
        self.thinker = ptr;
    }

    fn thinker(&self) -> NonNull<Thinker> {
        self.thinker
    }
}

/// A custom allocation for `Thinker` objects. This intends to keep them in a contiguous
/// zone of memory.
pub struct ThinkerAlloc {
    /// The main AllocPool buffer
    buf_ptr: *mut Option<Thinker>,
    /// Total capacity. Not possible to allocate over this.
    capacity: usize,
    /// Actual used AllocPool
    len: usize,
    /// The next free slot to insert in
    next_free: usize,
    pub tail: *mut Thinker,
}

impl Drop for ThinkerAlloc {
    fn drop(&mut self) {
        unsafe {
            for idx in 0..self.capacity {
                self.drop_item(idx);
            }
            let size = self.capacity * size_of::<Option<Thinker>>();
            let layout = Layout::from_size_align_unchecked(size, align_of::<Option<Thinker>>());
            dealloc(self.buf_ptr as *mut _, layout);
        }
    }
}

impl ThinkerAlloc {
    /// # Safety
    /// Once allocated the owner of this `ThinkerAlloc` must not move.
    pub unsafe fn new(capacity: usize) -> Self {
        let size = capacity * size_of::<Option<Thinker>>();
        let layout = Layout::from_size_align_unchecked(size, align_of::<Option<Thinker>>());
        let buf_ptr = alloc(layout) as *mut Option<Thinker>;

        for i in 0..capacity {
            buf_ptr.add(i).write(None);
        }

        Self {
            buf_ptr,
            capacity,
            len: 0,
            next_free: 0,
            tail: null_mut(),
        }
    }

    unsafe fn drop_item(&mut self, idx: usize) {
        debug_assert!(idx < self.capacity);
        let ptr = self.ptr_for_idx(idx);
        if std::mem::needs_drop::<Thinker>() {
            ptr::drop_in_place(ptr);
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    fn ptr_for_idx(&self, idx: usize) -> *mut Option<Thinker> {
        unsafe { self.buf_ptr.add(idx) }
    }

    fn find_first_free(&self, start: usize) -> Option<usize> {
        if self.len >= self.capacity {
            return None;
        }

        let mut ptr = unsafe { self.buf_ptr.add(start) };
        for idx in start..self.capacity {
            unsafe {
                ptr = ptr.add(1);
                if (*ptr).is_none() {
                    return Some(idx);
                }
            }
        }

        self.find_first_free(0)
    }

    /// Push an item to the Lump. Returns the index the item was pushed to if
    /// successful. This index can be used to remove the item, if you want to
    /// accurately remove the pushed item you should store this somewhere.
    ///
    /// # Safety:
    ///
    /// `<T>` must match the inner type of `Thinker`
    pub fn push<T: Think>(&mut self, mut thinker: Thinker) -> Option<NonNull<Thinker>> {
        if self.len == self.capacity {
            return None;
        }

        let mut idx = self.next_free;
        unsafe {
            let root_ptr = self.ptr_for_idx(idx);
            // Check if it's empty, if not, try to find a free slot
            if (*root_ptr).is_some() {
                idx = self.find_first_free(self.next_free)?;
            }
            thinker.index = idx;

            let root_ptr = self.ptr_for_idx(idx);
            ptr::write(root_ptr, Some(thinker));
            let inner_ptr = (*root_ptr).as_mut().unwrap_unchecked() as *mut Thinker;

            if self.tail.is_null() {
                self.tail = inner_ptr;
                (*self.tail).prev = self.tail;
                (*self.tail).next = self.tail;
            } else {
                (*inner_ptr).next = (*self.tail).next;
                (*inner_ptr).prev = self.tail;

                (*(*self.tail).next).prev = inner_ptr;
                (*self.tail).next = inner_ptr;
            }

            (*inner_ptr)
                .object
                .bad_mut::<T>()
                .set_thinker_ptr(NonNull::new_unchecked(inner_ptr));

            self.len += 1;
            if self.next_free < self.capacity {
                self.next_free += 1;
            }

            Some(NonNull::new_unchecked(inner_ptr))
        }
    }

    /// Ensure head is null if the pool is zero length
    fn maybe_reset_head(&mut self) {
        if self.len == 0 {
            self.tail = null_mut();
        }
    }

    /// Removes the entry at index
    pub fn remove(&mut self, idx: usize) {
        debug_assert!(idx < self.capacity);

        unsafe {
            let root_ptr = self.ptr_for_idx(idx);
            if (*root_ptr).is_none() {
                error!("TRIED TO REMOVE INVALID THINKER!");
                return;
            }
            let inner_ptr = (*root_ptr).as_mut().unwrap() as *mut Thinker;
            (*(*inner_ptr).next).prev = (*inner_ptr).prev;
            (*(*inner_ptr).prev).next = (*inner_ptr).next;

            if inner_ptr == self.tail {
                self.tail = (*self.tail).prev;
            }

            self.len -= 1;
            self.next_free = idx; // reuse the slot on next insert
            self.maybe_reset_head();
            std::ptr::write(root_ptr, None);
        }
    }

    pub fn iter(&self) -> IterLinks {
        IterLinks::new(self.tail)
    }

    pub fn iter_mut(&mut self) -> IterLinksMut {
        IterLinksMut::new(self.tail)
    }
}

pub struct IterLinks<'a> {
    start: *mut Thinker,
    current: *mut Thinker,
    _phantom: PhantomData<&'a Thinker>,
}

impl<'a> IterLinks<'a> {
    pub(crate) fn new(start: *mut Thinker) -> Self {
        Self {
            start,
            current: null_mut(),
            _phantom: PhantomData,
        }
    }
}

impl<'a> Iterator for IterLinks<'a> {
    type Item = &'a Thinker;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if !self.current.is_null() && self.current == self.start {
                return None;
            } else if self.current.is_null() {
                self.current = self.start;
            }

            let current = self.current;
            self.current = (*self.current).next;
            Some(&(*current))
        }
    }
}

pub struct IterLinksMut<'a> {
    start: *mut Thinker,
    current: *mut Thinker,
    _phantom: PhantomData<&'a Thinker>,
}

impl<'a> IterLinksMut<'a> {
    pub(crate) fn new(start: *mut Thinker) -> Self {
        Self {
            start,
            current: null_mut(),
            _phantom: PhantomData,
        }
    }
}

impl<'a> Iterator for IterLinksMut<'a> {
    type Item = &'a mut Thinker;

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            if self.current == self.start {
                return None;
            } else if self.current.is_null() {
                self.current = self.start;
            }

            let current = self.current;
            self.current = (*self.current).next;
            Some(&mut (*current))
        }
    }
}

/// All map object thinkers need to be registered here
#[repr(C)]
#[allow(clippy::large_enum_variant)]
pub enum ThinkerType {
    Test(TestObject),
    Mobj(MapObject),
    VDoor(VerticalDoor),
    FloorMove(FloorMove),
    CeilingMove(CeilingMove),
    Platform(Platform),
    LightFlash(LightFlash),
    StrobeFlash(StrobeFlash),
    FireFlicker(FireFlicker),
    Glow(Glow),
}

impl Debug for ThinkerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Test(_) => f.debug_tuple("Test").finish(),
            Self::Mobj(_) => f.debug_tuple("Mobj").finish(),
            Self::VDoor(_) => f.debug_tuple("VDoor").finish(),
            Self::FloorMove(_) => f.debug_tuple("FloorMove").finish(),
            Self::CeilingMove(_) => f.debug_tuple("CeilingMove").finish(),
            Self::Platform(_) => f.debug_tuple("Platform").finish(),
            Self::LightFlash(_) => f.debug_tuple("LightFlash").finish(),
            Self::StrobeFlash(_) => f.debug_tuple("StrobeFlash").finish(),
            Self::FireFlicker(_) => f.debug_tuple("FireFlicker").finish(),
            Self::Glow(_) => f.debug_tuple("Glow").finish(),
        }
    }
}

impl ThinkerType {
    pub fn bad_ref<T>(&self) -> &T {
        let mut ptr = self as *const Self as usize;
        ptr += size_of::<u64>();
        unsafe { &*(ptr as *const T) }
    }

    pub fn bad_mut<T>(&mut self) -> &mut T {
        let mut ptr = self as *mut Self as usize;
        ptr += size_of::<u64>();
        unsafe { &mut *(ptr as *mut T) }
    }
}

/// Thinkers *must* be contained in a structure that has **stable** memory locations.
/// In Doom this is managed by Doom's custom allocator `z_malloc`, where each location in memory
/// also has a pointer to the locations 'owner'. When Doom does a defrag or any op
/// that moves memory locations it also runs through the owners and updates their
/// pointers. This isn't done in the Rust version as that introduces a lot of overhead
/// and makes various things harder to do or harder to prove correct (if using unsafe).
///
/// Another way to manager Thinkers in a volatile container like a Vec is to use `self.function`
/// to mark for removal (same as Doom), then iterate over the container and only
/// run thinkers not marked for removal, then remove marked thinkers after cycle.
/// This method would have a big impact on iter speed though as there may be many
/// 'dead' thinkers and it would also impact the order of thinkers, which then means
/// recorded demo playback may be quite different to OG Doom.
///
/// Inserting the `Thinker` in to the game is done in p_tick.c with `P_RunThinkers`.
///
/// The LinkedList style serves to give the Objects a way to find the next/prev of
/// its neighbours and more, without having to pass in a ref to the Thinker container,
/// or iterate over possible blank spots in memory.
pub struct Thinker {
    /// The index location in the allocation
    index: usize,
    prev: *mut Thinker,
    next: *mut Thinker,
    object: ThinkerType,
    func: ActionF,
}

impl Thinker {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn obj_ref(&self) -> &ThinkerType {
        &self.object
    }

    pub fn obj_mut(&mut self) -> &mut ThinkerType {
        &mut self.object
    }

    pub fn has_action(&self) -> bool {
        !matches!(self.func, ActionF::None)
    }

    pub fn remove(&self) -> bool {
        matches!(self.func, ActionF::Remove)
    }

    pub fn set_action(&mut self, func: ActionF) {
        self.func = func
    }

    /// Run the `ThinkerType`'s `think()`. If the `think()` returns false then the function pointer is set to None
    /// to mark removal.
    pub fn think(&mut self, level: &mut Level) -> bool {
        match self.func {
            ActionF::Action1(f) => (f)(&mut self.object, level),
            ActionF::Player(_f) => true,
            ActionF::None => true,
            ActionF::Remove => false,
        }
    }
}

impl fmt::Debug for Thinker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Thinker")
            .field("prev", &(self.prev as usize))
            .field("next", &(self.next as usize))
            .field("object", &(self as *const Self as usize))
            .field("func", &self.func)
            .finish()
    }
}

#[derive(Clone)]
pub enum ActionF {
    None, // actionf_v
    /// To have the thinker removed from the thinker list on next cleanup pass
    Remove,
    Action1(fn(&mut ThinkerType, &mut Level) -> bool),
    Player(fn(&mut Player, &mut PspDef)), // P_SetPsprite runs this
}

impl fmt::Debug for ActionF {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionF::None => f.debug_struct("None").finish(),
            ActionF::Remove => f.debug_struct("Remove").finish(),
            ActionF::Action1(_) => f.debug_struct("Action1").finish(),
            ActionF::Player(_) => f.debug_struct("Player").finish(),
        }
    }
}

/// Every map object should implement this trait
pub trait Think {
    /// Creating a thinker should be the last step in new objects as `Thinker` takes ownership
    fn create_thinker(object: ThinkerType, func: ActionF) -> Thinker {
        Thinker {
            index: 0,
            prev: null_mut(),
            next: null_mut(),
            object,
            func,
        }
    }

    fn state(&self) -> bool {
        if let ActionF::None = self.thinker_ref().func {
            return true;
        }
        false
    }

    /// impl of this trait function should return true *if* the thinker + object are to be removed
    ///
    /// Functionally this is Acp1, but in Doom when used with a Thinker it calls only one function
    /// on the object and Null is used to track if the map object should be removed.
    ///
    /// **NOTE:**
    ///
    /// The impl of `think()` on type will need to cast `ThinkerType` with `object.bad_mut()`.
    fn think(object: &mut ThinkerType, level: &mut Level) -> bool;

    /// Implementer must store the pointer to the conatining Thinker
    fn set_thinker_ptr(&mut self, ptr: NonNull<Thinker>);

    fn thinker(&self) -> NonNull<Thinker>;

    fn thinker_ref(&self) -> &Thinker {
        unsafe { self.thinker().as_ref() }
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        unsafe { self.thinker().as_mut() }
    }
}

#[cfg(test)]
mod tests {
    use wad::WadData;

    use crate::{
        d_thinker::{ActionF, TestObject, Think, Thinker, ThinkerType},
        level_data::{level::Level, map_data::MapData},
    };

    use super::ThinkerAlloc;
    use std::ptr::{null_mut, NonNull};

    #[test]
    fn bad_stuff() {
        let mut x = ThinkerType::Test(TestObject {
            x: 42,
            thinker: NonNull::dangling(),
        });

        if let ThinkerType::Test(f) = &x {
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
        map.load(&wad);

        let mut l = unsafe {
            Level::new(
                crate::d_main::Skill::Baby,
                1,
                1,
                crate::doom_def::GameMode::Shareware,
            )
        };
        let mut x = Thinker {
            index: 0,
            prev: null_mut(),
            next: null_mut(),
            object: ThinkerType::Test(TestObject {
                x: 42,
                thinker: NonNull::dangling(),
            }),
            func: ActionF::Action1(TestObject::think),
        };

        assert!(x.think(&mut l));

        let ptr = NonNull::from(&mut x);
        x.object.bad_mut::<TestObject>().set_thinker_ptr(ptr);
        assert!(x.object.bad_mut::<TestObject>().thinker_mut().think(&mut l));
    }

    #[test]
    fn allocate() {
        let links = unsafe { ThinkerAlloc::new(64) };
        assert_eq!(links.len(), 0);
        assert_eq!(links.capacity(), 64);
    }

    #[test]
    fn push_1() {
        let mut links = unsafe { ThinkerAlloc::new(64) };
        assert_eq!(links.len(), 0);
        assert_eq!(links.capacity(), 64);

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerType::Test(TestObject {
                    x: 42,
                    thinker: NonNull::dangling(),
                }),
                ActionF::None,
            ))
            .unwrap();
        assert!(!links.tail.is_null());
        assert_eq!(links.len(), 1);
        unsafe {
            assert_eq!((*links.tail).object.bad_ref::<TestObject>().x, 42);
        }

        unsafe {
            dbg!(&*links.buf_ptr.add(0));
            dbg!(&*links.buf_ptr.add(1));
            dbg!(&*links.buf_ptr.add(2));
            dbg!(&*links.buf_ptr.add(62));

            assert!((*links.buf_ptr.add(0)).is_some());
            assert!((*links.buf_ptr.add(1)).is_none());
            assert!((*links.buf_ptr.add(2)).is_none());
        }

        links.remove(0);
        assert_eq!(links.len(), 0);
    }

    #[test]
    fn check_next_prev_links() {
        let mut links = unsafe { ThinkerAlloc::new(64) };

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerType::Test(TestObject {
                    x: 42,
                    thinker: NonNull::dangling(),
                }),
                ActionF::None,
            ))
            .unwrap();
        assert!(!links.tail.is_null());

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerType::Test(TestObject {
                    x: 666,
                    thinker: NonNull::dangling(),
                }),
                ActionF::None,
            ))
            .unwrap();

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerType::Test(TestObject {
                    x: 123,
                    thinker: NonNull::dangling(),
                }),
                ActionF::None,
            ))
            .unwrap();
        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerType::Test(TestObject {
                    x: 333,
                    thinker: NonNull::dangling(),
                }),
                ActionF::None,
            ))
            .unwrap();

        unsafe {
            // forward
            assert_eq!(
                (*links.buf_ptr)
                    .as_ref()
                    .unwrap_unchecked()
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                42
            );
            assert_eq!(
                (*(*links.buf_ptr).as_ref().unwrap_unchecked().next)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                666
            );
            assert_eq!(
                (*(*(*links.buf_ptr).as_ref().unwrap_unchecked().next).next)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                123
            );
            assert_eq!(
                (*(*(*(*links.buf_ptr).as_ref().unwrap_unchecked().next).next).next)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                333
            );
            assert_eq!(
                (*(*(*(*(*links.buf_ptr).as_ref().unwrap_unchecked().next).next).next).next)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                42
            );
            // back
            assert_eq!((*links.tail).object.bad_ref::<TestObject>().x, 42);
            assert_eq!((*(*links.tail).prev).object.bad_ref::<TestObject>().x, 333);
            assert_eq!(
                (*(*(*links.tail).prev).prev)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                123
            );
            assert_eq!(
                (*(*(*(*links.tail).prev).prev).prev)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                666
            );
        }

        links.remove(1);
        unsafe {
            assert_eq!((*links.tail).object.bad_ref::<TestObject>().x, 42);
            assert_eq!((*(*links.tail).prev).object.bad_ref::<TestObject>().x, 333);
            assert_eq!(
                (*(*(*links.tail).prev).prev)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                123
            );
        }
        links.remove(3);
        unsafe {
            assert_eq!((*links.tail).object.bad_ref::<TestObject>().x, 42);
            assert_eq!((*(*links.tail).prev).object.bad_ref::<TestObject>().x, 123);
            assert_eq!(
                (*(*(*links.tail).prev).prev)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                42
            );
        }
    }

    #[test]
    fn link_iter_and_removes() {
        let mut links = unsafe { ThinkerAlloc::new(64) };

        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 42,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 123,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 666,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 333,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));

        for (i, thinker) in links.iter().enumerate() {
            if i == 0 {
                assert_eq!(thinker.object.bad_ref::<TestObject>().x, 42);
            }
            if i == 1 {
                assert_eq!(thinker.object.bad_ref::<TestObject>().x, 123);
            }
            if i == 2 {
                assert_eq!(thinker.object.bad_ref::<TestObject>().x, 666);
            }
            if i == 3 {
                assert_eq!(thinker.object.bad_ref::<TestObject>().x, 333);
            }
        }
        unsafe {
            assert_eq!(
                (*links.buf_ptr.add(3))
                    .as_ref()
                    .unwrap()
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                333
            );
        }

        assert_eq!(links.iter().count(), 4);

        links.remove(3);
        assert_eq!(links.len(), 3);
        assert_eq!(links.iter().count(), 3);

        for (i, num) in links.iter().enumerate() {
            if i == 0 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 42);
            }
            if i == 1 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 123);
            }
            if i == 2 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 666);
            }
        }
        //
        links.remove(1);
        assert_eq!(links.len(), 2);
        assert_eq!(links.iter().count(), 2);

        for (i, num) in links.iter().enumerate() {
            if i == 0 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 42);
            }
            if i == 1 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 666);
            }
        }
    }

    #[test]
    fn link_iter_mut_and_map() {
        let mut links = unsafe { ThinkerAlloc::new(64) };

        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 42,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 123,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 666,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 333,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));

        for (i, num) in links.iter_mut().enumerate() {
            if i == 0 {
                assert_eq!(num.object.bad_mut::<TestObject>().x, 42);
            }
            if i == 2 {
                assert_eq!(num.object.bad_mut::<TestObject>().x, 666);
            }
            if let ThinkerType::Test(mapobj) = &mut num.object {
                mapobj.x = 1;
            }
        }

        let c: Vec<u32> = links
            .iter_mut()
            .map(|n| {
                if let ThinkerType::Test(mapobj) = &n.object {
                    mapobj.x
                } else {
                    0
                }
            })
            .collect();
        assert_eq!(c.len(), 4);

        for i in c {
            assert_eq!(i, 1);
        }
    }

    #[test]
    fn link_iter_mutate() {
        let mut links = unsafe { ThinkerAlloc::new(64) };

        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 42,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 123,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 666,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));
        links.push::<TestObject>(TestObject::create_thinker(
            ThinkerType::Test(TestObject {
                x: 333,
                thinker: NonNull::dangling(),
            }),
            ActionF::None,
        ));

        assert_eq!(links.len(), 4);

        for n in links.iter_mut() {
            if let ThinkerType::Test(mapobj) = &mut n.object {
                mapobj.x = 1;
            }
        }

        links.remove(2);

        for i in links.iter_mut() {
            if let ThinkerType::Test(mapobj) = &i.object {
                assert_eq!(mapobj.x, 1);
            }
        }

        let c: Vec<u32> = links
            .iter_mut()
            .map(|n| {
                if let ThinkerType::Test(mapobj) = &mut n.object {
                    mapobj.x
                } else {
                    0
                }
            })
            .collect();

        assert_eq!(c.len(), 3);
        assert_eq!(c.get(2), Some(&1));
    }
}
