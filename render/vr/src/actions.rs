//! Touch controller input through the OpenXR action system.
//!
//! Two action sets, both attached in the one call the spec allows. `gameplay`
//! is always synced; `menu` layers over it at a higher priority while a UI
//! plane shows, so the runtime hands the buttons and sticks they share to the
//! menu and reports the gameplay ones inactive. Movement, snap turn and firing
//! therefore stop behind a menu with no gate downstream, while the weapon's aim
//! pose keeps arriving from the set below.
//!
//! The runtime resolves the bindings, so the same actions work on any
//! controller whose profile it knows — Touch is what we suggest.

use log::info;
use openxr_sys as sys;
use openxr_sys::Handle as _;

use crate::xr::{self, Instance, XrError, intern_path, write_name};

/// Stick deflection that counts as pushed, for snap turn and menu steps.
/// A nudge should turn: the sticks rest at exactly centre, so there is no
/// drift to guard against.
const STICK_THRESHOLD: f32 = 0.1;
/// Deflection the stick must fall back under before it can snap again.
const STICK_RELEASE: f32 = 0.05;

/// Priority of the menu set over the gameplay set. Both are synced while a UI
/// plane shows; this is what makes the menu win the buttons they share.
const MENU_PRIORITY: u32 = 1;

const TOUCH_PROFILE: &str = "/interaction_profiles/oculus/touch_controller";
const HAND_PROFILE: &str = "/interaction_profiles/ext/hand_interaction_ext";

/// How far one snap turn rotates the player. Snap rather than smooth: a
/// view that turns while the head is still is the usual cause of
/// simulator sickness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnapTurn {
    #[default]
    Deg30,
    Deg45,
}

impl SnapTurn {
    pub fn degrees(self) -> f32 {
        match self {
            Self::Deg30 => 30.0,
            Self::Deg45 => 45.0,
        }
    }
}

/// A tracked pose in the same space as the eye cameras.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Pose {
    /// Metres, play space (right-handed, -Z forward, +Y up).
    pub position: [f32; 3],
    /// xyzw quaternion.
    pub orientation: [f32; 4],
}

/// What the player is asking for this frame, as plain data.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ControllerState {
    /// Left stick: +x right, +y forward, relative to where the player is
    /// looking. Analogue, -1..1, so it strafes as well as walks.
    pub move_axes: [f32; 2],
    /// Whether the left stick currently has an active input source.
    pub move_active: bool,
    /// Right stick, including forward deflection used by optional aim-warp.
    pub turn_axes: [f32; 2],
    pub turn_active: bool,
    /// Snap-turn step asked for this frame, radians, positive left.
    /// Rotates the play space; emitted once per stick push.
    pub turn_radians: f32,
    pub fire: bool,
    pub use_: bool,
    /// Rising edges — one weapon change per press.
    pub weapon_next: bool,
    pub weapon_prev: bool,
    /// Where the shooting hand is pointing. `None` while the runtime has no
    /// pose for it.
    pub aim: Option<Pose>,
    /// Where the off hand is pointing, which is what aims a teleport.
    pub aim_off: Option<Pose>,
    /// Right grip pose for held objects; `None` when its position or orientation is invalid.
    pub grip: Option<Pose>,
    /// Off-hand trigger, held while a teleport is being aimed.
    pub teleport: bool,
    /// Distinguishes releasing the trigger from losing its input source.
    pub teleport_active: bool,
    /// Whether the off hand is in a posture a far-field aim can be read from.
    /// True on a controller, which has no such signal and can always aim.
    pub aim_ready: bool,
    /// Menu steps, one per stick push per direction.
    pub ui_up: bool,
    pub ui_down: bool,
    pub ui_left: bool,
    pub ui_right: bool,
    /// Rising edges: confirm, go back, and open or close the menu.
    pub ui_accept: bool,
    pub ui_back: bool,
    pub ui_menu: bool,
}

/// One step per push per direction: the stick must fall back near centre before
/// that direction fires again, or holding it would repeat at frame rate.
#[derive(Default)]
struct StickLatch {
    /// Direction currently held, `0` when free to fire.
    latched: i8,
}

impl StickLatch {
    fn step(&mut self, v: f32) -> i8 {
        if self.latched != 0 {
            if v.abs() < STICK_RELEASE {
                self.latched = 0;
            }
            return 0;
        }
        if v.abs() < STICK_THRESHOLD {
            return 0;
        }
        self.latched = if v > 0.0 { 1 } else { -1 };
        self.latched
    }

    /// Arm for the next push without emitting one, for a stick whose owning
    /// set just changed — held across the switch is not a fresh push.
    fn hold(&mut self) {
        self.latched = 1;
    }
}

/// The action sets and their actions, live for the session.
pub struct Actions {
    gameplay: sys::ActionSet,
    menu: sys::ActionSet,
    mv: sys::Action,
    turn: sys::Action,
    fire: sys::Action,
    use_: sys::Action,
    weapon_next: sys::Action,
    weapon_prev: sys::Action,
    /// Shooting aim, teleport aim, and right grip, located together in play space.
    aim_space: sys::Space,
    aim_off_space: sys::Space,
    grip_space: sys::Space,
    teleport: sys::Action,
    aim_ready: sys::Action,
    nav: sys::Action,
    accept: sys::Action,
    back: sys::Action,
    /// One per set, on the same physical button, so it opens the menu from
    /// gameplay and closes it from inside.
    open_menu: sys::Action,
    close_menu: sys::Action,
    snap: SnapTurn,
    turn_latch: StickLatch,
    nav_x: StickLatch,
    nav_y: StickLatch,
    /// One-shot trace: whether the runtime ever refused a sync.
    unfocused_logged: bool,
}

impl Actions {
    /// Builds both action sets and suggests Touch bindings. Must be called
    /// before `xrBeginSession`: attaching is a one-shot per session, so every
    /// set goes in together.
    pub fn new(
        instance: &Instance,
        session: sys::Session,
        snap: SnapTurn,
    ) -> Result<Self, XrError> {
        let gameplay = action_set(instance, "gameplay", "Gameplay", 0)?;
        let menu = action_set(instance, "menu", "Menu", MENU_PRIORITY)?;

        let boolean = sys::ActionType::BOOLEAN_INPUT;
        let vector2 = sys::ActionType::VECTOR2F_INPUT;
        let mv = action(instance, gameplay, "move", "Move", vector2)?;
        let turn = action(instance, gameplay, "turn", "Turn", vector2)?;
        let fire = action(instance, gameplay, "fire", "Fire", boolean)?;
        let use_ = action(instance, gameplay, "use", "Use", boolean)?;
        let weapon_next = action(instance, gameplay, "weapon_next", "Next Weapon", boolean)?;
        let weapon_prev = action(
            instance,
            gameplay,
            "weapon_prev",
            "Previous Weapon",
            boolean,
        )?;
        let aim = action(
            instance,
            gameplay,
            "aim",
            "Aim",
            sys::ActionType::POSE_INPUT,
        )?;
        let aim_off = action(
            instance,
            gameplay,
            "aim_off",
            "Off-hand Aim",
            sys::ActionType::POSE_INPUT,
        )?;
        let grip = action(
            instance,
            gameplay,
            "grip",
            "Weapon Grip",
            sys::ActionType::POSE_INPUT,
        )?;
        let teleport = action(instance, gameplay, "teleport", "Teleport", boolean)?;
        let aim_ready = action(instance, gameplay, "aim_ready", "Aim Ready", boolean)?;
        let open_menu = action(instance, gameplay, "open_menu", "Open Menu", boolean)?;
        let nav = action(instance, menu, "nav", "Navigate", vector2)?;
        let accept = action(instance, menu, "accept", "Select", boolean)?;
        let back = action(instance, menu, "back", "Back", boolean)?;
        let close_menu = action(instance, menu, "close_menu", "Close Menu", boolean)?;

        // Actions repeated across the two sets are deliberate: priority
        // resolves which one the shared source reaches.
        suggest(
            instance,
            TOUCH_PROFILE,
            &[
                (mv, "/user/hand/left/input/thumbstick"),
                (turn, "/user/hand/right/input/thumbstick"),
                (fire, "/user/hand/right/input/trigger/value"),
                (use_, "/user/hand/right/input/a/click"),
                (weapon_next, "/user/hand/right/input/b/click"),
                (weapon_prev, "/user/hand/left/input/x/click"),
                (aim, "/user/hand/right/input/aim/pose"),
                (aim_off, "/user/hand/left/input/aim/pose"),
                (grip, "/user/hand/right/input/grip/pose"),
                (teleport, "/user/hand/left/input/trigger/value"),
                // `menu/click` is the left controller's alone; the right one's
                // equivalent is `system/click`, which the runtime keeps.
                (open_menu, "/user/hand/left/input/menu/click"),
                (nav, "/user/hand/left/input/thumbstick"),
                (nav, "/user/hand/right/input/thumbstick"),
                (accept, "/user/hand/right/input/a/click"),
                (accept, "/user/hand/right/input/trigger/value"),
                (back, "/user/hand/right/input/b/click"),
                (close_menu, "/user/hand/left/input/menu/click"),
            ],
        )?;

        if instance.hand_interaction {
            suggest(
                instance,
                HAND_PROFILE,
                &[
                    (aim, "/user/hand/right/input/aim/pose"),
                    (aim_off, "/user/hand/left/input/aim/pose"),
                    (grip, "/user/hand/right/input/grip/pose"),
                    (fire, "/user/hand/right/input/aim_activate_ext/value"),
                    (use_, "/user/hand/right/input/grasp_ext/value"),
                    (teleport, "/user/hand/left/input/aim_activate_ext/value"),
                    (
                        aim_ready,
                        "/user/hand/left/input/aim_activate_ext/ready_ext",
                    ),
                    (open_menu, "/user/hand/left/input/grasp_ext/value"),
                    (accept, "/user/hand/right/input/aim_activate_ext/value"),
                    (back, "/user/hand/right/input/grasp_ext/value"),
                    (close_menu, "/user/hand/left/input/grasp_ext/value"),
                ],
            )?;
        }

        let sets = [gameplay, menu];
        let attach = sys::SessionActionSetsAttachInfo {
            ty: sys::SessionActionSetsAttachInfo::TYPE,
            next: std::ptr::null(),
            count_action_sets: sets.len() as u32,
            action_sets: sets.as_ptr(),
        };
        // SAFETY: info outlives the call; the sets stay live with us.
        xr::check("xrAttachSessionActionSets", unsafe {
            (instance.attach_session_action_sets)(session, &attach)
        })?;

        // An action space can only be made once the sets are attached.
        let aim_space = pose_space(instance, session, aim)?;
        let aim_off_space = pose_space(instance, session, aim_off)?;
        let grip_space = pose_space(instance, session, grip)?;
        info!("vr: controllers bound, {}° snap turn", snap.degrees());

        Ok(Self {
            gameplay,
            menu,
            mv,
            turn,
            fire,
            use_,
            weapon_next,
            weapon_prev,
            aim_space,
            aim_off_space,
            grip_space,
            teleport,
            aim_ready,
            nav,
            accept,
            back,
            open_menu,
            close_menu,
            snap,
            turn_latch: StickLatch::default(),
            nav_x: StickLatch::default(),
            nav_y: StickLatch::default(),
            unfocused_logged: false,
        })
    }

    /// Syncs the active sets and reads every action. Call once a frame, only
    /// while the session is focused — the runtime reports actions inactive
    /// otherwise, which reads as "no input".
    pub fn sync(
        &mut self,
        instance: &Instance,
        session: sys::Session,
        ui_active: bool,
    ) -> Result<ControllerState, XrError> {
        let sets = [
            active_set(self.gameplay),
            active_set(if ui_active { self.menu } else { self.gameplay }),
        ];
        let sync = sys::ActionsSyncInfo {
            ty: sys::ActionsSyncInfo::TYPE,
            next: std::ptr::null(),
            count_active_action_sets: if ui_active { 2 } else { 1 },
            active_action_sets: sets.as_ptr(),
        };
        // SAFETY: info outlives the call.
        let result = unsafe { (instance.sync_actions)(session, &sync) };
        // Not focused is normal (menu, headset off): no input, not an error.
        if result == sys::Result::SESSION_NOT_FOCUSED {
            if !self.unfocused_logged {
                self.unfocused_logged = true;
                info!("vr: session not focused; controllers idle until it is");
            }
            self.turn_latch = StickLatch::default();
            self.nav_x = StickLatch::default();
            self.nav_y = StickLatch::default();
            return Ok(ControllerState::default());
        }
        xr::check("xrSyncActions", result)?;

        // The set that does not own the sticks reads them centred, so latch its
        // pair or a stick held across the switch steps as the menu opens.
        if ui_active {
            self.turn_latch.hold();
        } else {
            self.nav_x.hold();
            self.nav_y.hold();
        }
        let nav = vector2(instance, session, self.nav)?.unwrap_or_default();
        let (nav_x, nav_y) = (self.nav_x.step(nav[0]), self.nav_y.step(nav[1]));
        let movement = vector2(instance, session, self.mv)?;
        let turning = vector2(instance, session, self.turn)?;
        let turn_axes = turning.unwrap_or_default();
        // A forward push used to aim a warp should not also snap the view.
        let turn = self
            .turn_latch
            .step(if turn_axes[0].abs() > turn_axes[1].abs() {
                turn_axes[0]
            } else {
                0.0
            });
        let teleport = boolean(instance, session, self.teleport)?;
        Ok(ControllerState {
            move_axes: movement.unwrap_or_default(),
            move_active: movement.is_some(),
            turn_axes,
            turn_active: turning.is_some(),
            // Positive is left, matching the runtime's yaw.
            turn_radians: -f32::from(turn) * self.snap.degrees().to_radians(),
            fire: boolean(instance, session, self.fire)?.held,
            use_: boolean(instance, session, self.use_)?.held,
            weapon_next: boolean(instance, session, self.weapon_next)?.pressed,
            weapon_prev: boolean(instance, session, self.weapon_prev)?.pressed,
            aim: None,
            aim_off: None,
            grip: None,
            teleport: teleport.held,
            teleport_active: teleport.active,
            aim_ready: boolean_or(instance, session, self.aim_ready, true)?,
            ui_right: nav_x > 0,
            ui_left: nav_x < 0,
            ui_up: nav_y > 0,
            ui_down: nav_y < 0,
            ui_accept: boolean(instance, session, self.accept)?.pressed,
            ui_back: boolean(instance, session, self.back)?.pressed,
            ui_menu: boolean(instance, session, self.open_menu)?.pressed
                || boolean(instance, session, self.close_menu)?.pressed,
        })
    }

    /// Shooting aim, teleport aim, and right grip at the frame's predicted time in `base`.
    /// Invalid poses are `None`; runtime failures retain the concrete OpenXR result.
    pub fn poses(
        &self,
        instance: &Instance,
        session: sys::Session,
        base: sys::Space,
        at: sys::Time,
    ) -> Result<[Option<Pose>; 3], XrError> {
        let spaces = [self.aim_space, self.aim_off_space, self.grip_space];
        let locate = sys::SpacesLocateInfo {
            ty: sys::SpacesLocateInfo::TYPE,
            next: std::ptr::null(),
            base_space: base,
            time: at,
            space_count: 3,
            spaces: spaces.as_ptr(),
        };
        let mut data = [sys::SpaceLocationData {
            location_flags: sys::SpaceLocationFlags::EMPTY,
            pose: identity_pose(),
        }; 3];
        let mut locations = sys::SpaceLocations {
            ty: sys::SpaceLocations::TYPE,
            next: std::ptr::null_mut(),
            location_count: 3,
            locations: data.as_mut_ptr(),
        };
        // SAFETY: all spaces belong to this session and the fixed arrays remain live for the call.
        xr::check("xrLocateSpaces", unsafe {
            (instance.locate_spaces)(session, &locate, &mut locations)
        })?;
        Ok(data.map(|location| {
            let valid = sys::SpaceLocationFlags::POSITION_VALID
                | sys::SpaceLocationFlags::ORIENTATION_VALID;
            location.location_flags.contains(valid).then(|| {
                let (p, q) = (location.pose.position, location.pose.orientation);
                Pose {
                    position: [p.x, p.y, p.z],
                    orientation: [q.x, q.y, q.z, q.w],
                }
            })
        }))
    }

    /// Runtime handles only; the session outlives this.
    ///
    /// # Safety
    /// `instance` must still be live.
    pub unsafe fn destroy(&self, instance: &Instance) {
        for space in [self.aim_space, self.aim_off_space, self.grip_space] {
            // SAFETY: caller contract; nothing uses the space afterwards.
            let r = unsafe { (instance.destroy_space)(space) };
            if r != sys::Result::SUCCESS {
                log::warn!("vr: xrDestroySpace failed: {}", r.into_raw());
            }
        }
        for set in [self.gameplay, self.menu] {
            // SAFETY: caller contract; nothing uses the set afterwards.
            let r = unsafe { (instance.destroy_action_set)(set) };
            if r != sys::Result::SUCCESS {
                log::warn!("vr: xrDestroyActionSet failed: {}", r.into_raw());
            }
        }
    }
}

/// Suggest every binding for one interaction profile. The call replaces any
/// earlier suggestion for that profile, so each is given its full set at once.
fn suggest(
    instance: &Instance,
    profile: &str,
    pairs: &[(sys::Action, &str)],
) -> Result<(), XrError> {
    // SAFETY: instance is live for every path interned here.
    let bindings = unsafe {
        pairs
            .iter()
            .map(|&(action, path)| {
                Ok(sys::ActionSuggestedBinding {
                    action,
                    binding: intern_path(instance, path)?,
                })
            })
            .collect::<Result<Vec<_>, XrError>>()?
    };
    // SAFETY: instance is live.
    let profile = unsafe { intern_path(instance, profile)? };
    let suggestion = sys::InteractionProfileSuggestedBinding {
        ty: sys::InteractionProfileSuggestedBinding::TYPE,
        next: std::ptr::null(),
        interaction_profile: profile,
        count_suggested_bindings: bindings.len() as u32,
        suggested_bindings: bindings.as_ptr(),
    };
    // SAFETY: the suggestion and its bindings outlive the call.
    xr::check("xrSuggestInteractionProfileBindings", unsafe {
        (instance.suggest_interaction_profile_bindings)(instance.handle, &suggestion)
    })
}

/// A space tracking a pose action. Legal only once the sets are attached.
fn pose_space(
    instance: &Instance,
    session: sys::Session,
    action: sys::Action,
) -> Result<sys::Space, XrError> {
    let info = sys::ActionSpaceCreateInfo {
        ty: sys::ActionSpaceCreateInfo::TYPE,
        next: std::ptr::null(),
        action,
        subaction_path: sys::Path::NULL,
        pose_in_action_space: identity_pose(),
    };
    let mut space = sys::Space::NULL;
    // SAFETY: info outlives the call; out-param written on success.
    xr::check("xrCreateActionSpace", unsafe {
        (instance.create_action_space)(session, &info, &mut space)
    })?;
    Ok(space)
}

fn identity_pose() -> sys::Posef {
    sys::Posef {
        orientation: sys::Quaternionf {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        },
        position: sys::Vector3f {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    }
}

fn action_set(
    instance: &Instance,
    name: &str,
    localized: &str,
    priority: u32,
) -> Result<sys::ActionSet, XrError> {
    let mut create = sys::ActionSetCreateInfo {
        ty: sys::ActionSetCreateInfo::TYPE,
        next: std::ptr::null(),
        action_set_name: [0; sys::MAX_ACTION_SET_NAME_SIZE],
        localized_action_set_name: [0; sys::MAX_LOCALIZED_ACTION_SET_NAME_SIZE],
        priority,
    };
    write_name(&mut create.action_set_name, name);
    write_name(&mut create.localized_action_set_name, localized);
    let mut out = sys::ActionSet::NULL;
    // SAFETY: info outlives the call.
    xr::check("xrCreateActionSet", unsafe {
        (instance.create_action_set)(instance.handle, &create, &mut out)
    })?;
    Ok(out)
}

fn active_set(action_set: sys::ActionSet) -> sys::ActiveActionSet {
    sys::ActiveActionSet {
        action_set,
        subaction_path: sys::Path::NULL,
    }
}

fn action(
    instance: &Instance,
    set: sys::ActionSet,
    name: &str,
    localized: &str,
    ty: sys::ActionType,
) -> Result<sys::Action, XrError> {
    let mut create = sys::ActionCreateInfo {
        ty: sys::ActionCreateInfo::TYPE,
        next: std::ptr::null(),
        action_name: [0; sys::MAX_ACTION_NAME_SIZE],
        action_type: ty,
        count_subaction_paths: 0,
        subaction_paths: std::ptr::null(),
        localized_action_name: [0; sys::MAX_LOCALIZED_ACTION_NAME_SIZE],
    };
    write_name(&mut create.action_name, name);
    write_name(&mut create.localized_action_name, localized);
    let mut out = sys::Action::NULL;
    // SAFETY: info outlives the call.
    xr::check("xrCreateAction", unsafe {
        (instance.create_action)(set, &create, &mut out)
    })?;
    Ok(out)
}

fn get_info(action: sys::Action) -> sys::ActionStateGetInfo {
    sys::ActionStateGetInfo {
        ty: sys::ActionStateGetInfo::TYPE,
        next: std::ptr::null(),
        action,
        subaction_path: sys::Path::NULL,
    }
}

/// A button's two readings: `held` for things that repeat while down,
/// `pressed` for the rising edge, so one push is one action.
struct Button {
    held: bool,
    pressed: bool,
    active: bool,
}

fn boolean_state(
    instance: &Instance,
    session: sys::Session,
    action: sys::Action,
) -> Result<sys::ActionStateBoolean, XrError> {
    let info = get_info(action);
    let mut state = sys::ActionStateBoolean {
        ty: sys::ActionStateBoolean::TYPE,
        next: std::ptr::null_mut(),
        current_state: sys::FALSE,
        changed_since_last_sync: sys::FALSE,
        last_change_time: sys::Time::from_nanos(0),
        is_active: sys::FALSE,
    };
    // SAFETY: info and out struct outlive the call.
    xr::check("xrGetActionStateBoolean", unsafe {
        (instance.get_action_state_boolean)(session, &info, &mut state)
    })?;
    Ok(state)
}

fn boolean(
    instance: &Instance,
    session: sys::Session,
    action: sys::Action,
) -> Result<Button, XrError> {
    let state = boolean_state(instance, session, action)?;
    let held = state.is_active == sys::TRUE && state.current_state == sys::TRUE;
    Ok(Button {
        held,
        pressed: held && state.changed_since_last_sync == sys::TRUE,
        active: state.is_active == sys::TRUE,
    })
}

/// A boolean that falls back to `absent` when the live profile binds no source
/// for it: an input a profile lacks must not read as "off".
fn boolean_or(
    instance: &Instance,
    session: sys::Session,
    action: sys::Action,
    absent: bool,
) -> Result<bool, XrError> {
    let state = boolean_state(instance, session, action)?;
    if state.is_active != sys::TRUE {
        return Ok(absent);
    }
    Ok(state.current_state == sys::TRUE)
}

fn vector2(
    instance: &Instance,
    session: sys::Session,
    action: sys::Action,
) -> Result<Option<[f32; 2]>, XrError> {
    let info = get_info(action);
    let mut state = sys::ActionStateVector2f {
        ty: sys::ActionStateVector2f::TYPE,
        next: std::ptr::null_mut(),
        current_state: sys::Vector2f {
            x: 0.0,
            y: 0.0,
        },
        changed_since_last_sync: sys::FALSE,
        last_change_time: sys::Time::from_nanos(0),
        is_active: sys::FALSE,
    };
    // SAFETY: info and out struct outlive the call.
    xr::check("xrGetActionStateVector2f", unsafe {
        (instance.get_action_state_vector2f)(session, &info, &mut state)
    })?;
    if state.is_active != sys::TRUE {
        return Ok(None);
    }
    Ok(Some([state.current_state.x, state.current_state.y]))
}
