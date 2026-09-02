//! Everything `game-exe` knows about a headset.
//!
//! The display hands out eye poses and a session; the game has an angle, a
//! menu and a tic command. This is the one place the two are reconciled, so
//! the loop and the display path stay free of it.

use gameplay::{
    AimRay, MAX_AIM_SLOPE, Player, PlayerState, TeleportScratch, aim_teleport, dash_target,
};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{ConfigKey, GameState, KeyCode, SubsystemTrait};
use glam::Vec3;
use input::{InputState, PadState};
use math::{Angle, Bam, FixedT};
use render_backend::{PanelPlacement, RenderStack};
use render_common::{
    EyeView, HandJoint, HandPointer, HandPose, HeadFrame, PlaySpace, TeleportView, UNITS_PER_METRE,
    WeaponHand,
};
use vr::{ControllerState, HandJoints, Pose};

use crate::cheats::Cheats;
use crate::config::{DashDistance, Locomotion, PanelConfig, UserConfig, WarpActivation, WarpStyle};
use crate::d_main::input_responder;

/// Panel distances are config'd in whole centimetres; the display wants metres.
const CM_PER_METRE: f32 = 100.0;

/// `ConfigKey::VrWeaponScale` is stored ×1000 so a menu slider can step it.
const WEAPON_SCALE_DIVISOR: f32 = 1000.0;

/// The weapon hand's aim pointer extends two metres.
const POINTER_LENGTH: f32 = 2.0 * UNITS_PER_METRE;
const WARP_STICK_PRESS: f32 = 0.65;
const WARP_STICK_RELEASE: f32 = 0.25;

#[derive(Clone, Copy)]
enum WarpSignal {
    Activate,
    Hold,
    Release,
    Cancel,
}

#[derive(Debug, PartialEq)]
enum WarpEvent {
    None,
    Aim,
    Commit,
    Dash,
}

/// A cancelled gesture must return to rest before it can start another move.
#[derive(Default)]
struct WarpGesture {
    mode: Option<(WarpStyle, WarpActivation)>,
    armed: bool,
    aiming: bool,
}

impl WarpGesture {
    fn step(&mut self, mode: Option<(WarpStyle, WarpActivation)>, signal: WarpSignal) -> WarpEvent {
        if self.mode != mode {
            self.armed = false;
            self.aiming = false;
            self.mode = mode;
        }
        let Some((style, _)) = mode else {
            return WarpEvent::None;
        };
        match signal {
            WarpSignal::Cancel => {
                self.armed = false;
                self.aiming = false;
                WarpEvent::None
            }
            WarpSignal::Release => {
                self.armed = true;
                if std::mem::take(&mut self.aiming) {
                    WarpEvent::Commit
                } else {
                    WarpEvent::None
                }
            }
            WarpSignal::Activate if self.armed => match style {
                WarpStyle::Aim => {
                    self.aiming = true;
                    WarpEvent::Aim
                }
                WarpStyle::Dash => {
                    self.armed = false;
                    WarpEvent::Dash
                }
            },
            WarpSignal::Hold | WarpSignal::Activate => {
                if self.aiming {
                    WarpEvent::Aim
                } else {
                    WarpEvent::None
                }
            }
        }
    }
}

fn stick_signal([x, y]: [f32; 2], directional: bool) -> WarpSignal {
    let length = x.hypot(y);
    if length < WARP_STICK_RELEASE {
        WarpSignal::Release
    } else if directional || (y > 0.0 && y >= x.abs()) {
        if length >= WARP_STICK_PRESS {
            WarpSignal::Activate
        } else {
            WarpSignal::Hold
        }
    } else {
        WarpSignal::Cancel
    }
}

impl From<PanelConfig> for PanelPlacement {
    fn from(panel: PanelConfig) -> Self {
        Self {
            distance_m: panel.distance_cm as f32 / CM_PER_METRE,
            width_deg: panel.width_deg as f32,
            pitch_deg: panel.pitch_deg as f32,
        }
    }
}

/// Carries the state that only makes sense across frames.
#[derive(Default)]
pub(crate) struct VrBridge {
    /// Player angle (BAM) the gaze slaving last wrote; a mismatch means the
    /// game re-pointed the player (spawn, teleport).
    written_angle: Option<u32>,
    /// Whether a UI plane was showing last frame, for the anchor edge.
    ui_active: bool,
    warp: WarpGesture,
    /// Reused arc scratch so aiming allocates nothing per frame.
    scratch: TeleportScratch,
}

impl VrBridge {
    /// Fold one frame of headset input into the game, and report what the
    /// display path needs. Call once per frame, before the tic runs.
    pub(crate) fn run<I, S, H, F>(
        &mut self,
        screen: &mut RenderStack<u32>,
        game: &mut Game,
        input: &mut InputState,
        menu: &mut impl SubsystemTrait,
        machines: &mut GameSubsystem<I, S, H, F>,
        cheats: &mut Cheats,
        config: &UserConfig,
        menu_active: bool,
    ) -> HeadFrame
    where
        I: SubsystemTrait,
        S: SubsystemTrait,
        H: SubsystemTrait,
        F: SubsystemTrait,
    {
        screen.set_panels(config.vr.hud_panel.into(), config.vr.screen_panel.into());

        let ui_active = menu_active
            || matches!(
                game.gamestate,
                GameState::Intermission | GameState::Finale | GameState::DemoScreen
            );
        if ui_active && !self.ui_active {
            screen.anchor_panels();
        }
        self.ui_active = ui_active;

        let weapon_scale =
            game.config_values[ConfigKey::VrWeaponScale as usize] as f32 / WEAPON_SCALE_DIVISOR;
        let Some(session) = screen.session() else {
            return HeadFrame::default();
        };
        session.set_ui_active(ui_active);

        // Frame opening refreshes actions, joints, and eyes at the same predicted time.
        let eyes = screen.begin_frame().unwrap_or_default();
        let Some(session) = screen.session() else {
            return HeadFrame::default();
        };
        self.slave_angle_to_gaze(session, game);
        let body_yaw = session.playspace_yaw();
        let controls = session.controls();
        let joints = if config.vr.show_hands {
            session.hand_joints()
        } else {
            [None, None]
        };
        let hand = controls.grip.map(|pose| WeaponHand {
            position: pose.position,
            units_per_pixel: weapon_scale,
        });

        let hands = self.hands_in_world(game, eyes[0].as_ref(), &joints, [None, controls.aim]);
        screen.set_hands(hands);
        // Must be latched before the tic that reads it.
        game.players[game.consoleplayer].aim = self.shot_ray(game, eyes[0].as_ref(), controls.aim);
        let teleport = self.drive_teleport(
            &controls,
            game,
            eyes[0].as_ref(),
            body_yaw,
            config,
            ui_active,
        );
        self.drive_ui(&controls, game, input, menu, machines, cheats, config);
        HeadFrame {
            eyes,
            hand,
            teleport,
        }
    }

    /// Where the shooting hand puts a shot, in the map units gameplay traces
    /// in. `None` leaves shots on the body with vanilla auto-aim.
    fn shot_ray(&self, game: &Game, eye: Option<&EyeView>, pose: Option<Pose>) -> Option<AimRay> {
        let pose = pose?;
        let space = self.play_space(game, eye?)?;
        let ([x, y, z], [dx, dy, dz]) = space.map_ray(pose.position, pose.orientation);

        // Rise over run, so a hand aimed at the zenith has no run to divide by.
        let run = (dx * dx + dy * dy).sqrt();
        let slope = if run > f32::EPSILON {
            (dz / run).clamp(-MAX_AIM_SLOPE, MAX_AIM_SLOPE)
        } else {
            MAX_AIM_SLOPE * dz.signum()
        };
        Some(AimRay {
            origin: (
                FixedT::from_f32(x),
                FixedT::from_f32(y),
                FixedT::from_f32(z),
            ),
            angle: Angle::<Bam>::new(dy.atan2(dx)),
            slope: FixedT::from_f32(slope),
        })
    }

    /// Hand skeletons and controller/hand aim pointers in world map units.
    fn hands_in_world(
        &self,
        game: &Game,
        eye: Option<&EyeView>,
        joints: &[Option<HandJoints>; 2],
        aims: [Option<Pose>; 2],
    ) -> Option<HandPose> {
        if joints.iter().all(Option::is_none) && aims.iter().all(Option::is_none) {
            return None;
        }
        let space = self.play_space(game, eye?)?;
        // A radius is a distance, not a position: it scales, it does not fold.
        Some(HandPose {
            joints: joints.map(|hand| {
                hand.map(|hand| {
                    hand.map(|joint| HandJoint {
                        position: space.render_point(joint.position),
                        radius: joint.radius * UNITS_PER_METRE,
                    })
                })
            }),
            pointers: aims.map(|aim| {
                aim.map(|pose| {
                    let (origin, dir) = space.render_ray(pose.position, pose.orientation);
                    HandPointer {
                        origin,
                        tip: (Vec3::from(origin) + Vec3::from(dir) * POINTER_LENGTH).into(),
                    }
                })
            }),
        })
    }

    /// Where the player's play space sits in the world this frame.
    fn play_space(&self, game: &Game, eye: &EyeView) -> Option<PlaySpace> {
        let player = &game.players[game.consoleplayer];
        let mobj = player.mobj()?;
        Some(PlaySpace {
            origin: [mobj.x.to_f32(), mobj.y.to_f32(), mobj.z.to_f32()],
            eye_z: player.viewz.to_f32(),
            yaw: Angle::<Bam>::new(eye.playspace_yaw),
        })
    }

    /// Resolve the configured warp gesture without letting a menu, loss of
    /// tracking or change of settings turn cancellation into a release.
    fn drive_teleport(
        &mut self,
        controls: &ControllerState,
        game: &mut Game,
        eye: Option<&EyeView>,
        body_yaw: f32,
        config: &UserConfig,
        ui_active: bool,
    ) -> Option<TeleportView> {
        let mode = (config.vr.locomotion == Locomotion::Teleport)
            .then_some((config.vr.warp_style, config.vr.warp_activation));
        let allowed = !ui_active
            && !controls.ui_menu
            && !game.paused
            && game.gamestate == GameState::Level
            && game.players[game.consoleplayer].player_state == PlayerState::Live
            && eye.is_some();
        let aim = match config.vr.warp_activation {
            WarpActivation::RightStickForward => controls.aim,
            WarpActivation::LeftTrigger | WarpActivation::LeftStickForward => controls.aim_off,
        };
        let signal = if !allowed {
            WarpSignal::Cancel
        } else if config.vr.warp_style == WarpStyle::Dash {
            if controls.move_active {
                stick_signal(controls.move_axes, true)
            } else {
                WarpSignal::Cancel
            }
        } else if aim.is_none()
            || (config.vr.warp_activation != WarpActivation::RightStickForward
                && !controls.aim_ready)
        {
            WarpSignal::Cancel
        } else {
            match config.vr.warp_activation {
                WarpActivation::LeftTrigger if controls.teleport_active => {
                    if controls.teleport {
                        WarpSignal::Activate
                    } else {
                        WarpSignal::Release
                    }
                }
                WarpActivation::LeftStickForward if controls.move_active => {
                    stick_signal(controls.move_axes, false)
                }
                WarpActivation::RightStickForward if controls.turn_active => {
                    stick_signal(controls.turn_axes, false)
                }
                _ => WarpSignal::Cancel,
            }
        };
        let event = self.warp.step(mode, signal);
        if event == WarpEvent::None {
            return None;
        }
        let level = game.level.as_ref()?;
        let player = game.players[game.consoleplayer].mobj()?;
        if event == WarpEvent::Dash {
            let [right, forward] = controls.move_axes;
            // The player's angle includes headset yaw; dash follows the body.
            let (sin, cos) = Angle::<Bam>::new(body_yaw).sin_cos();
            let direction = Vec3::new(
                forward * cos + right * sin,
                forward * sin - right * cos,
                0.0,
            )
            .normalize_or_zero();
            if let Some(target) = dash_target(
                level,
                direction,
                Player::warp_distance(config.vr.dash_distance == DashDistance::Run),
                player,
            ) {
                game.players[game.consoleplayer].start_teleport(target);
            }
            return None;
        }
        let space = self.play_space(game, eye?)?;
        let aim = aim?;
        let (origin, dir) = space.render_ray(aim.position, aim.orientation);
        let resolved = aim_teleport(
            level,
            &mut self.scratch,
            Vec3::from(origin),
            Vec3::from(dir),
            player,
        );

        if event == WarpEvent::Commit {
            if resolved.valid {
                game.players[game.consoleplayer].start_teleport(resolved.landing);
            }
            return None;
        }
        Some(TeleportView {
            origin,
            dir,
            flight_time: resolved.flight_time,
            landing: resolved.landing.into(),
            valid: resolved.valid,
        })
    }

    /// The player faces where they look — unless the game re-pointed them, in
    /// which case the play space turns to the heading it imposed instead.
    fn slave_angle_to_gaze(&mut self, session: &mut vr::Session, game: &mut Game) {
        let look = session.look_yaw();
        let Some(mobj) = game.players[game.consoleplayer].mobj_mut() else {
            return;
        };
        if self.written_angle == Some(mobj.angle.to_bam()) {
            mobj.angle = Angle::new(look);
        } else {
            session.align_playspace(mobj.angle.rad());
        }
        self.written_angle = Some(mobj.angle.to_bam());
    }

    /// Menu navigation goes through the responder the keyboard already uses,
    /// so the menu itself knows nothing about controllers.
    fn drive_ui<I, S, H, F>(
        &self,
        controls: &ControllerState,
        game: &mut Game,
        input: &mut InputState,
        menu: &mut impl SubsystemTrait,
        machines: &mut GameSubsystem<I, S, H, F>,
        cheats: &mut Cheats,
        config: &UserConfig,
    ) where
        I: SubsystemTrait,
        S: SubsystemTrait,
        H: SubsystemTrait,
        F: SubsystemTrait,
    {
        let dead = game.players[game.consoleplayer].player_state == PlayerState::Dead;
        for key in ui_keys(controls, dead) {
            if !input_responder(key, game, menu, machines, cheats) {
                input.events.unset_kb(key);
            }
        }
        input.events.set_pad(PadState {
            move_axes: if config.vr.locomotion == Locomotion::Smooth {
                controls.move_axes
            } else {
                [0.0, 0.0]
            },
            fire: controls.fire,
            // Vanilla reborns on use alone; the trigger has to reach it too.
            use_: controls.use_ || (dead && controls.fire),
            weapon_next: controls.weapon_next,
            weapon_prev: controls.weapon_prev,
        });
    }
}

/// Keys the controller's UI edges stand in for.
fn ui_keys(controls: &ControllerState, dead: bool) -> impl Iterator<Item = KeyCode> {
    [
        (controls.ui_up, KeyCode::Up),
        (controls.ui_down, KeyCode::Down),
        (controls.ui_left, KeyCode::Left),
        (controls.ui_right, KeyCode::Right),
        // Death is gameplay, not a UI plane, so confirm is not gated on one.
        (
            controls.ui_accept || (dead && controls.fire),
            KeyCode::Return,
        ),
        (controls.ui_back || controls.ui_menu, KeyCode::Escape),
    ]
    .into_iter()
    .filter_map(|(pressed, key)| pressed.then_some(key))
}

#[cfg(test)]
mod tests {
    use super::{WarpEvent, WarpGesture, WarpSignal, stick_signal};
    use crate::config::{WarpActivation, WarpStyle};

    #[test]
    fn aim_release_commits_but_cancellation_and_control_changes_do_not() {
        let mode = Some((WarpStyle::Aim, WarpActivation::LeftTrigger));
        let mut gesture = WarpGesture::default();
        // Held across startup or a menu is not a new gesture.
        assert_eq!(gesture.step(mode, WarpSignal::Activate), WarpEvent::None);
        gesture.step(mode, WarpSignal::Release);
        assert_eq!(gesture.step(mode, WarpSignal::Activate), WarpEvent::Aim);
        assert_eq!(gesture.step(mode, WarpSignal::Release), WarpEvent::Commit);

        gesture.step(mode, WarpSignal::Activate);
        assert_eq!(gesture.step(mode, WarpSignal::Cancel), WarpEvent::None);
        assert_eq!(gesture.step(mode, WarpSignal::Release), WarpEvent::None);
        gesture.step(mode, WarpSignal::Activate);
        let changed = Some((WarpStyle::Aim, WarpActivation::RightStickForward));
        assert_eq!(gesture.step(changed, WarpSignal::Release), WarpEvent::None);
        gesture.step(changed, WarpSignal::Activate);
        assert_eq!(gesture.step(None, WarpSignal::Release), WarpEvent::None);
    }

    #[test]
    fn dash_flick_requires_return_to_rest_and_aim_requires_forward() {
        let mode = Some((WarpStyle::Dash, WarpActivation::LeftTrigger));
        let mut gesture = WarpGesture::default();
        gesture.step(mode, stick_signal([0.0, 0.0], true));
        assert_eq!(
            gesture.step(mode, stick_signal([-0.9, 0.0], true)),
            WarpEvent::Dash
        );
        assert_eq!(
            gesture.step(mode, stick_signal([0.0, -0.9], true)),
            WarpEvent::None
        );
        gesture.step(mode, stick_signal([0.0, 0.0], true));
        assert_eq!(
            gesture.step(mode, stick_signal([0.0, -0.9], true)),
            WarpEvent::Dash
        );

        let mode = Some((WarpStyle::Aim, WarpActivation::LeftStickForward));
        gesture.step(mode, stick_signal([0.0, 0.0], false));
        assert_eq!(
            gesture.step(mode, stick_signal([0.0, 0.9], false)),
            WarpEvent::Aim
        );
        // Moving sideways cancels; it is not a stick release and must not warp.
        assert_eq!(
            gesture.step(mode, stick_signal([0.9, 0.0], false)),
            WarpEvent::None
        );
        assert_eq!(
            gesture.step(mode, stick_signal([0.0, 0.0], false)),
            WarpEvent::None
        );
    }
}
