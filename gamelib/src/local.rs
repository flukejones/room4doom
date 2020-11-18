use crate::map_object::MapObject;
use std::any::Any;

/// P_MOBJ
pub static ONFLOORZ: i32 = i32::MIN;
/// P_MOBJ
pub static ONCEILINGZ: i32 = i32::MAX;

pub static MAXHEALTH: i32 = 100;
pub static VIEWHEIGHT: i32 = 41;

/// Enum of function callbacks
pub enum ActionF {
    actionf_v, // NULL thinker
    actionf_p1(Box<dyn FnMut(&Box<dyn Any>)>),
    actionf_p2(Box<dyn FnMut(&Box<dyn Any>, &Box<dyn Any>)>), // Unused?
}
