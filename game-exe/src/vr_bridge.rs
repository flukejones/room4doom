//! Everything `game-exe` knows about a headset.
//!
//! The display hands out eye poses and a session; the game has an angle, a
//! menu and a tic command. This is the one place the two are reconciled, so
//! the loop and the display path stay free of it.

use gameplay::{AimRay, MAX_AIM_SLOPE, PlayerState, TeleportScratch, aim_teleport};
use gamestate::Game;
use gamestate::subsystems::GameSubsystem;
use gamestate_traits::{ConfigKey, GameState, KeyCode, SubsystemTrait};
use glam::Vec3;
use input::{InputState, PadState};
use math::{Angle, Bam, FixedT};
use render_backend::{PanelPlacement, RenderStack};
use render_common::{
    EyeView, HandJoint, HandPose, HeadFrame, PlaySpace, TeleportView, UNITS_PER_METRE, WeaponHand,
};
use vr::{ControllerState, HandJoints, Pose};

use crate::cheats::Cheats;
use crate::config::{Locomotion, PanelConfig, UserConfig};
use crate::d_main::input_responder;

/// Panel distances are config'd in whole centimetres; the display wants metres.
const CM_PER_METRE: f32 = 100.0;

/// `ConfigKey::VrWeaponScale` is stored ×1000 so a menu slider can step it.
const WEAPON_SCALE_DIVISOR: f32 = 1000.0;

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
    /// Whether a teleport arc was being aimed last frame; releasing the stick
    /// from that state is what commits the move.
    aiming: bool,
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
        self.slave_angle_to_gaze(session, game);
        let controls = session.controls();
        let joints = config.vr.show_hands.then(|| session.hand_joints());
        let hand = controls.aim.map(|pose| WeaponHand {
            position: pose.position,
            units_per_pixel: weapon_scale,
        });

        // The runtime paces the display, so the frame opens here and every eye
        // is located at once.
        let eyes = screen.begin_frame().unwrap_or_default();

        let hands = joints.and_then(|joints| self.hands_in_world(game, eyes[0].as_ref(), &joints));
        screen.set_hands(hands);
        // Must be latched before the tic that reads it.
        game.players[game.consoleplayer].aim = self.shot_ray(game, eyes[0].as_ref(), controls.aim);
        let teleport = self.drive_teleport(&controls, game, eyes[0].as_ref(), config, ui_active);
        self.drive_ui(&controls, game, input, menu, machines, cheats);
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

    /// The tracked hands in world map units, or `None` when neither is tracked.
    fn hands_in_world(
        &self,
        game: &Game,
        eye: Option<&EyeView>,
        joints: &[Option<HandJoints>; 2],
    ) -> Option<HandPose> {
        if joints.iter().all(Option::is_none) {
            return None;
        }
        let space = self.play_space(game, eye?)?;
        // A radius is a distance, not a position: it scales, it does not fold.
        Some(joints.map(|hand| {
            hand.map(|hand| {
                hand.map(|joint| HandJoint {
                    position: space.render_point(joint.position),
                    radius: joint.radius * UNITS_PER_METRE,
                })
            })
        }))
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

    /// The teleport arc to draw, if one is being aimed. Committing the move is
    /// this function's job too: the arc follows the off hand while its trigger
    /// is held and fires when the trigger is let go.
    fn drive_teleport(
        &mut self,
        controls: &ControllerState,
        game: &mut Game,
        eye: Option<&EyeView>,
        config: &UserConfig,
        ui_active: bool,
    ) -> Option<TeleportView> {
        if config.vr.locomotion != Locomotion::Teleport {
            return None;
        }
        // An open menu or lost tracking abandons the aim rather than committing
        // it — neither is the player choosing to go.
        if ui_active {
            self.aiming = false;
            return None;
        }
        // Without the posture gate an incidental near pinch would teleport.
        let (Some(eye), Some(aim), true) = (eye, controls.aim_off, controls.aim_ready) else {
            self.aiming = false;
            return None;
        };
        let space = self.play_space(game, eye)?;
        let level = game.level.as_ref()?;
        let player = game.players[game.consoleplayer].mobj()?;
        let (origin, dir) = space.render_ray(aim.position, aim.orientation);
        let resolved = aim_teleport(
            level,
            &mut self.scratch,
            Vec3::from(origin),
            Vec3::from(dir),
            player,
        );

        if self.aiming && !controls.teleport {
            self.aiming = false;
            if resolved.valid {
                game.players[game.consoleplayer].start_teleport(resolved.landing);
            }
            return None;
        }
        self.aiming = controls.teleport;
        self.aiming.then_some(TeleportView {
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
            move_axes: controls.move_axes,
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
