//! Bare-hand joints through `XR_EXT_hand_tracking`.
//!
//! Each hand is available while the runtime reports active tracking, independently
//! of the selected controller or gesture profile.

use log::{debug, info, warn};
use openxr_sys as sys;
use openxr_sys::Handle as _;

use crate::xr::{self, Instance, XrError};

/// Joints per hand, in `XR_EXT_hand_tracking` order.
pub const JOINT_COUNT: usize = sys::HAND_JOINT_COUNT_EXT;

/// One joint, in the same space as the eye cameras.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Joint {
    pub position: [f32; 3],
    pub orientation: [f32; 4],
    /// Metres; 0 when the runtime reports none.
    pub radius: f32,
}

/// A hand's joints for one frame. `None` per hand when untracked.
pub type HandJoints = [Joint; JOINT_COUNT];

/// The two trackers, created once per session.
pub struct Hands {
    trackers: [sys::HandTrackerEXT; 2],
    joints: [Option<HandJoints>; 2],
}

impl Hands {
    /// `None` when the runtime does not offer hand tracking.
    pub fn new(instance: &Instance, session: sys::Session) -> Option<Result<Self, XrError>> {
        let ext = instance.hand_tracking.as_ref()?;
        let mut trackers = [sys::HandTrackerEXT::NULL; 2];
        for (i, hand) in [sys::HandEXT::LEFT, sys::HandEXT::RIGHT]
            .into_iter()
            .enumerate()
        {
            let create = sys::HandTrackerCreateInfoEXT {
                ty: sys::HandTrackerCreateInfoEXT::TYPE,
                next: std::ptr::null(),
                hand,
                hand_joint_set: sys::HandJointSetEXT::DEFAULT,
            };
            // SAFETY: info outlives the call; out-param written on success.
            if let Err(e) = xr::check("xrCreateHandTrackerEXT", unsafe {
                (ext.create)(session, &create, &mut trackers[i])
            }) {
                return Some(Err(e));
            }
        }
        info!("vr: hand trackers created");
        Some(Ok(Self {
            trackers,
            joints: [None; 2],
        }))
    }

    /// Re-reads both hands. Call once per frame, after the frame's
    /// predicted display time is known.
    pub fn locate(&mut self, instance: &Instance, space: sys::Space, at: sys::Time) {
        let Some(ext) = instance.hand_tracking.as_ref() else {
            return;
        };
        for (i, tracker) in self.trackers.iter().enumerate() {
            let info = sys::HandJointsLocateInfoEXT {
                ty: sys::HandJointsLocateInfoEXT::TYPE,
                next: std::ptr::null(),
                base_space: space,
                time: at,
            };
            let mut raw = [sys::HandJointLocationEXT::default(); JOINT_COUNT];
            let mut locations = sys::HandJointLocationsEXT {
                ty: sys::HandJointLocationsEXT::TYPE,
                next: std::ptr::null_mut(),
                is_active: sys::FALSE,
                joint_count: JOINT_COUNT as u32,
                joint_locations: raw.as_mut_ptr(),
            };
            // SAFETY: info and the joint array outlive the call.
            let located = xr::check("xrLocateHandJointsEXT", unsafe {
                (ext.locate)(*tracker, &info, &mut locations)
            });
            if let Err(e) = located {
                warn!("vr: {e}");
                self.joints[i] = None;
                continue;
            }
            let active = locations.is_active == sys::TRUE;
            if active != self.joints[i].is_some() {
                debug!("vr: hand {i} tracking active={active}");
            }
            if !active {
                self.joints[i] = None;
                continue;
            }
            self.joints[i] = Some(std::array::from_fn(|j| {
                let joint = raw[j];
                Joint {
                    position: [
                        joint.pose.position.x,
                        joint.pose.position.y,
                        joint.pose.position.z,
                    ],
                    orientation: [
                        joint.pose.orientation.x,
                        joint.pose.orientation.y,
                        joint.pose.orientation.z,
                        joint.pose.orientation.w,
                    ],
                    radius: joint.radius,
                }
            }));
        }
    }

    /// Joints located this frame; `None` per hand while it is untracked.
    pub fn joints(&self) -> &[Option<HandJoints>; 2] {
        &self.joints
    }

    /// # Safety
    /// `instance` must still be live.
    pub unsafe fn destroy(&self, instance: &Instance) {
        let Some(ext) = instance.hand_tracking.as_ref() else {
            return;
        };
        for tracker in self.trackers {
            // SAFETY: caller contract; nothing uses the tracker after.
            let r = unsafe { (ext.destroy)(tracker) };
            if r != sys::Result::SUCCESS {
                warn!("vr: xrDestroyHandTrackerEXT failed: {}", r.into_raw());
            }
        }
    }
}
