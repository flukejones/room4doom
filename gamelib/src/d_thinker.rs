use std::alloc::{alloc, alloc_zeroed, dealloc, Layout};
use std::fmt::{self};
use std::marker::PhantomData;
use std::mem::{align_of, size_of};
use std::ptr::{self, null_mut, NonNull};

use crate::level_data::level::Level;
use crate::p_map_object::MapObject;
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

    fn set_thinker_ptr(&mut self, ptr: NonNull<Thinker>) {
        self.thinker = ptr
    }

    fn thinker_ref(&self) -> &Thinker {
        unsafe { self.thinker.as_ref() }
    }

    fn thinker_mut(&mut self) -> &mut Thinker {
        unsafe { self.thinker.as_mut() }
    }
}

/// Bits available in a usize
const BITCOUNT: usize = size_of::<usize>() * 8;

/// Determine the amount of bit blocks required to track the capacity
#[inline(always)]
const fn num_index_blocks(cap: usize) -> usize {
    // -1 because we go from 0-n
    (cap + BITCOUNT - 1) / BITCOUNT
}

/// Bitmasking for array indexing to track which locations are used
struct BitIndex {
    /// Which number of usize block to use, this is usable as an offset from a pointer
    /// for a region of memory used as n*usize
    block: usize,
    /// How many bit_idx to shift left for mask
    bit_shift: usize,
}

impl BitIndex {
    /// usize block / position in usize for bit
    const fn from(idx: usize) -> Self {
        Self {
            block: idx / BITCOUNT,
            bit_shift: idx % BITCOUNT,
        }
    }

    const fn bit_mask(&self) -> usize {
        1 << self.bit_shift
    }

    /// Mask a block of bit_idx and see if the bit required is on
    const fn is_on(&self, block: usize) -> bool {
        ((block >> self.bit_shift) & 1) != 0
    }
}

pub struct ThinkerAlloc {
    /// The main AllocPool buffer
    buf_ptr: NonNull<Thinker>,
    /// Total capacity. Not possible to allocate over this.
    capacity: usize,
    /// Tracks which slots in the buffer are used
    bit_index: NonNull<usize>,
    /// Actual used AllocPool
    len: usize,
    /// The next free slot to insert in
    next_free: usize,
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
            dealloc(self.buf_ptr.as_ptr() as *mut _, layout);
        }
    }
}

impl ThinkerAlloc {
    pub fn new(capacity: usize) -> Self {
        unsafe {
            let size1 = capacity * size_of::<Thinker>();
            let layout1 = Layout::from_size_align_unchecked(size1, align_of::<Thinker>());
            let buf_ptr = alloc(layout1);

            let size2 = size_of::<usize>() * num_index_blocks(capacity);
            let layout2 = Layout::from_size_align_unchecked(size2, align_of::<usize>());
            let bit_ptr = alloc_zeroed(layout2);

            Self {
                buf_ptr: NonNull::new_unchecked(buf_ptr as *mut Thinker),
                capacity,
                bit_index: NonNull::new_unchecked(bit_ptr as *mut usize),
                len: 0,
                next_free: 0,
                head: null_mut(),
            }
        }
    }

    unsafe fn read(&self, idx: usize) -> Thinker {
        debug_assert!(idx < self.capacity);
        let ptr = self.buf_ptr.as_ptr().add(idx);
        ptr.read()
    }

    unsafe fn drop_item(&mut self, idx: usize) {
        debug_assert!(idx < self.capacity);
        let ptr = self.buf_ptr.as_ptr().add(idx);
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

    fn find_first_free(&self) -> Option<usize> {
        if self.len >= self.capacity {
            return None;
        }
        for idx in 0..self.capacity {
            let bit_idx = BitIndex::from(idx);

            let block = unsafe { *self.bit_index.as_ptr().add(bit_idx.block) };
            if ((block >> bit_idx.bit_shift) & 0b1) == 0 {
                return Some(idx);
            }
        }
        None
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
        let mut bit_idx = BitIndex::from(idx);

        // Check if it's empty, if not, try to find a free slot
        let block = unsafe { *self.bit_index.as_ptr().add(bit_idx.block) };
        if bit_idx.is_on(block) {
            if let Some(slot) = self.find_first_free() {
                idx = slot;
                bit_idx = BitIndex::from(idx);
            } else {
                return None;
            }
        }

        // Create the pointer
        let ptr = unsafe { self.buf_ptr.as_ptr().add(idx) };
        thinker
            .object
            .bad_mut::<T>()
            .set_thinker_ptr(unsafe { NonNull::new_unchecked(ptr) });
        //thinker.func = ActionF::Action1(T::think);

        // then link
        if self.head.is_null() {
            self.head = ptr;
            thinker.prev = ptr;
            thinker.next = ptr;
        } else {
            unsafe {
                let head = &mut *self.head;
                // get tail from head and make sure its prev is the inserted node
                (*head.next).prev = ptr;
                // inserted node's next must be tail (head next)
                thinker.next = head.next;
                // head needs to link to inserted now
                (*head).next = ptr;
                // and inserted previous link to last head
                thinker.prev = head;
                // set head
                self.head = ptr;
            }
        }

        // write the data
        unsafe {
            debug_assert!(idx < self.capacity);
            ptr::write(ptr, thinker);
            *self.bit_index.as_ptr().add(bit_idx.block) |= bit_idx.bit_mask();
        }

        self.len += 1;
        if self.next_free < self.capacity - 1 {
            self.next_free += 1;
        }

        unsafe { Some(NonNull::new_unchecked(ptr)) }
    }

    /// Ensure head is null if the pool is zero length
    fn maybe_reset_head(&mut self) {
        if self.len == 0 {
            self.head = null_mut();
        }
    }

    fn take_no_replace(&mut self, idx: usize) -> Option<Thinker> {
        let bit_idx = BitIndex::from(idx);

        let block = unsafe { *self.bit_index.as_ptr().add(bit_idx.block) };
        if ((block >> bit_idx.bit_shift) & 0b1) == 0 {
            return None;
        }

        self.len -= 1;
        self.next_free = idx; // reuse the slot on next insert

        unsafe {
            // set the slot bit first
            *self.bit_index.as_ptr().add(bit_idx.block) &= !bit_idx.bit_mask();
            // read out the T (bit copy)
            let ret = self.read(idx);
            // then drop that memory
            self.drop_item(idx);
            Some(ret)
        }
    }

    /// Removes the entry at index
    pub fn remove(&mut self, idx: usize) {
        // Need to take so that neighbour nodes can be updated
        if let Some(node) = self.take_no_replace(idx) {
            let prev = node.prev;
            let next = node.next;

            unsafe {
                (*next).prev = prev;
                (*prev).next = next;
            }

            self.maybe_reset_head();
        }
    }

    pub fn iter(&self) -> IterLinks {
        IterLinks::new(unsafe { (*self.head).next })
    }

    pub fn iter_mut(&mut self) -> IterLinksMut {
        IterLinksMut::new(unsafe { (*self.head).next })
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
            if !self.current.is_null() && self.current == self.start {
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
pub enum ThinkerType {
    Test(TestObject),
    Mobj(MapObject),
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
/// In Doom this is managed by Doom's custom allocator, where each location in memory
/// also has a pointer to the locations 'owner'. When Doom does a defrag or any op
/// that moves memory locations it also runs through the owners and updates their
/// pointers. This isn't done in the Rust version.
///
/// Another way to manager Thinkers in a volatile container like a Vec is to use `self.function`
/// to mark for removal (same as Doom), then iterate over the container and only
/// run thinkers not marked for removal, then remove marked thinkers after cycle.
/// This method would have a big impact on iter speed though as there may be many
/// 'dead' thinkers.
///
/// Inserting the `Thinker` in to the game is done in p_tick.c with `P_RunThinkers`.
///
/// On Drop the thinker will unlink itself from whereever it is in the link chain.
///
///  State should live in MapObject. State.action and Think.function are two
///  different functions
///
/// The LinkedList style serves to give the Objects a way to find the next/prev of
/// its neighbours and more, without having to pass in a ref to the Thinker container,
/// or iterate over possible blank spots in memory.
pub struct Thinker {
    prev: *mut Thinker,
    next: *mut Thinker,
    object: ThinkerType,
    func: ActionF,
}

impl Thinker {
    pub fn object(&mut self) -> &mut ThinkerType {
        &mut self.object
    }

    pub fn set_action(&mut self, func: ActionF) {
        self.func = func
    }

    /// Run the `ThinkerType`'s `think()`. If the `think()` returns false then the function pointer is set to None
    /// to mark removal.
    pub fn think(&mut self, level: &mut Level) -> bool {
        let res = match self.func {
            ActionF::Action1(f) => (f)(&mut self.object, level),
            ActionF::Player(_f) => todo!(),
            ActionF::None => false,
        };
        res
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

// impl Drop for Thinker {
//     fn drop(&mut self) {
//         // if this thinker has links in both directions then the thinkers at those
//         // ends must be linked to this thinker, so we need to unlink those from
//         // this thinker, and link them together
//         // self.unlink();
//     }
// }

#[derive(Clone)]
pub enum ActionF {
    None, // actionf_v
    Action1(fn(&mut ThinkerType, &mut Level) -> bool),
    Player(*const dyn Fn(&mut Player, &mut PspDef)), // P_SetPsprite runs this
}

impl fmt::Debug for ActionF {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActionF::None => f.debug_struct("None").finish(),
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

    /// impl of this trait should return true *if* the thinker + object are to be removed
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

    fn thinker_ref(&self) -> &Thinker;

    fn thinker_mut(&mut self) -> &mut Thinker;
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

        let mut l = Level::setup_level(
            &wad,
            crate::d_main::Skill::Baby,
            1,
            1,
            crate::doom_def::GameMode::Shareware,
            &mut [],
            &mut [false; 4],
        );
        let mut x = Thinker {
            prev: null_mut(),
            next: null_mut(),
            object: ThinkerType::Test(TestObject {
                x: 42,
                thinker: NonNull::dangling(),
            }),
            func: ActionF::Action1(TestObject::think),
        };

        assert_eq!(x.think(&mut l), true);

        let ptr = NonNull::from(&mut x);
        x.object.bad_mut::<TestObject>().set_thinker_ptr(ptr);
        assert_eq!(
            x.object.bad_mut::<TestObject>().thinker_mut().think(&mut l),
            true
        );
    }

    #[test]
    fn allocate() {
        let _links = ThinkerAlloc::new(64);
    }

    #[test]
    fn check_next_prev_links() {
        let mut links = ThinkerAlloc::new(64);

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerType::Test(TestObject {
                    x: 42,
                    thinker: NonNull::dangling(),
                }),
                ActionF::None,
            ))
            .unwrap();
        assert!(!links.head.is_null());

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerType::Test(TestObject {
                    x: 666,
                    thinker: NonNull::dangling(),
                }),
                ActionF::None,
            ))
            .unwrap();
        unsafe {
            assert_eq!(links.buf_ptr.as_ref().object.bad_ref::<TestObject>().x, 42);
            assert_eq!(
                (*links.buf_ptr.as_ref().next)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                666
            );

            assert_eq!((*links.head).object.bad_ref::<TestObject>().x, 666);
            assert_eq!((*(*links.head).next).object.bad_ref::<TestObject>().x, 42);
            assert_eq!((*(*links.head).prev).object.bad_ref::<TestObject>().x, 42);
        }

        links
            .push::<TestObject>(TestObject::create_thinker(
                ThinkerType::Test(TestObject {
                    x: 123,
                    thinker: NonNull::dangling(),
                }),
                ActionF::None,
            ))
            .unwrap();

        unsafe {
            // forward
            assert_eq!(links.buf_ptr.as_ref().object.bad_ref::<TestObject>().x, 42);
            assert_eq!(
                (*links.buf_ptr.as_ref().next)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                666
            );
            assert_eq!(
                (*(*links.buf_ptr.as_ref().next).next)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                123
            );
            // back
            assert_eq!((*links.head).object.bad_ref::<TestObject>().x, 123);
            assert_eq!((*(*links.head).prev).object.bad_ref::<TestObject>().x, 666);
            assert_eq!(
                (*(*(*links.head).prev).prev)
                    .object
                    .bad_ref::<TestObject>()
                    .x,
                42
            );
        }

        links.remove(1);
        unsafe {
            assert_eq!((*links.head).object.bad_ref::<TestObject>().x, 123);
            assert_eq!((*(*links.head).prev).object.bad_ref::<TestObject>().x, 42);
        }
    }

    #[test]
    fn link_iter_nand_removes() {
        let mut links = ThinkerAlloc::new(64);

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

        for (i, num) in links.iter().enumerate() {
            if i == 0 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 42);
            }
            if i == 2 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 666);
            }
        }

        assert_eq!(links.iter().count(), 4);

        links.remove(0);
        assert_eq!(links.len(), 3);
        assert_eq!(links.iter().count(), 3);

        for (i, num) in links.iter().enumerate() {
            if i == 0 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 123);
            }
            if i == 2 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 333);
            }
        }

        links.remove(3);
        assert_eq!(links.len(), 2);
        assert_eq!(links.iter().count(), 2);

        for (i, num) in links.iter().enumerate() {
            if i == 0 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 123);
            }
            if i == 1 {
                assert_eq!((*num).object.bad_ref::<TestObject>().x, 666);
            }
        }
    }

    #[test]
    fn link_iter_mut_and_map() {
        let mut links = ThinkerAlloc::new(64);

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
        let mut links = ThinkerAlloc::new(64);

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
