use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI};

use crate::p_local::VIEWHEIGHT;
use crate::p_map_object::MapObject;
use crate::r_bsp::Bsp;
use angle::Angle;
use player::Player;
use sdl2::render::Canvas;
use sdl2::surface::Surface;
use wad::Wad;

pub mod angle;
pub mod d_thinker;
pub mod doom_def;
pub mod entities;
pub mod flags;
pub mod info;
pub mod p_enemy;
pub mod p_local;
pub mod p_map;
pub mod p_map_object;
pub mod p_player_sprite;
pub mod p_spec;
pub mod player;
pub mod r_bsp;
pub mod r_segs;
pub mod sounds;

pub struct Game<'c> {
    _wad:           Wad,
    map:            Bsp,
    /// Each player in the array may be controlled
    players:        [Player<'c>; 4],
    /// Tracks which players are currently active, set by d_net.c loop
    player_in_game: [bool; 4],
    map_objects:    Vec<MapObject<'c>>,
    deathmatch:     i32, // only if started as net death
}

impl<'c> Game<'c> {
    pub fn new(options: GameOptions) -> Game<'c> {
        let mut wad = Wad::new(options.iwad);
        wad.read_directories();
        let mut map = Bsp::new(options.map.unwrap_or("E1M1".to_owned()));
        map.load(&wad);

        let player_thing1 = &map.get_things()[0];
        let player_thing2 = &map.get_things()[1];
        let player_thing3 = &map.get_things()[2];
        let player_thing4 = &map.get_things()[3];

        let players = [
            Player::new(
                player_thing1.pos.clone(),
                map.point_in_subsector(&player_thing1.pos)
                    .unwrap()
                    .sector
                    .floor_height as f32
                    + VIEWHEIGHT as f32,
                Angle::new(player_thing1.angle * PI / 180.0),
                map.point_in_subsector(&player_thing1.pos).unwrap(),
                None,
            ),
            Player::new(
                player_thing2.pos.clone(),
                map.point_in_subsector(&player_thing2.pos)
                    .unwrap()
                    .sector
                    .floor_height as f32
                    + VIEWHEIGHT as f32,
                Angle::new(player_thing2.angle * PI / 180.0),
                map.point_in_subsector(&player_thing2.pos).unwrap(),
                None,
            ),
            Player::new(
                player_thing3.pos.clone(),
                map.point_in_subsector(&player_thing3.pos)
                    .unwrap()
                    .sector
                    .floor_height as f32
                    + VIEWHEIGHT as f32,
                Angle::new(player_thing3.angle * PI / 180.0),
                map.point_in_subsector(&player_thing3.pos).unwrap(),
                None,
            ),
            Player::new(
                player_thing4.pos.clone(),
                map.point_in_subsector(&player_thing4.pos)
                    .unwrap()
                    .sector
                    .floor_height as f32
                    + VIEWHEIGHT as f32,
                Angle::new(player_thing4.angle * PI / 180.0),
                map.point_in_subsector(&player_thing4.pos).unwrap(),
                None,
            ),
        ];

        //MapObject::p_spawn_player(player_thing, &map, &mut players);

        Game {
            _wad: wad,
            map,
            players,
            player_in_game: [false; 4],
            map_objects: Vec::with_capacity(200),
            deathmatch: 0,
        }
    }

    // D_RunFrame is the main loop, calls many functions:
    //  - Screen wipe maybe
    //    + I_UpdateNoBlit
    //    + M_Drawer
    //    + I_FinishUpdate
    //  - I_StartFrame
    //  - TryRunTics
    //  - S_UpdateSounds
    //  - D_Display then:
    //    + screen wipe or
    //    + I_FinishUpdate // page flip or blit buffer
    // D_DoomLoop
    //
    // D_Display
    //

    pub fn do_tic(&mut self) {
        //
    }

    /// I_UpdateNoBlit
    pub fn i_update_no_blit(&mut self, canvas: &mut Canvas<Surface>, dt: FP) {
        self.map.clear_clip_segs();

        // The state machine will handle which state renders to the surface
        //self.states.render(dt, &mut self.canvas);
        let player_subsect =
            self.map.point_in_subsector(&self.player.xy).unwrap();
        self.player.viewz = player_subsect.sector.floor_height as f32 + 41.0;
        self.player.sub_sector = player_subsect; //DPtr::new(player_subsect);

        canvas.clear();
        self.map
            .draw_bsp(&self.player, self.map.start_node(), canvas);
    }

    fn i_finish_update(&self, canvas: &mut Canvas<Surface>, dt: FP) {
        canvas.present();

        let texture_creator = canvas.texture_creator();
        let t = canvas.into_surface().as_texture(&texture_creator).unwrap();

        canvas.copy(&t, None, None).unwrap();
        //self.draw_automap();
        canvas.present();
    }
}

/// R_PointToDist
fn point_to_dist(x: f32, y: f32, object: &Player) -> f32 {
    let mut dx = (x - object.xy.x()).abs();
    let mut dy = (y - object.xy.y()).abs();

    if dy > dx {
        let temp = dx;
        dx = dy;
        dy = temp;
    }

    let dist = (dx.powi(2) + dy.powi(2)).sqrt();
    dist
}

/// R_ScaleFromGlobalAngle
// All should be in rads
fn scale(
    visangle: Angle,
    rw_normalangle: Angle,
    rw_distance: f32,
    view_angle: Angle,
) -> f32 {
    static MAX_SCALEFACTOR: f32 = 64.0;
    static MIN_SCALEFACTOR: f32 = 0.00390625;

    let anglea = Angle::new(FRAC_PI_2 + visangle.rad() - view_angle.rad()); // CORRECT
    let angleb = Angle::new(FRAC_PI_2 + visangle.rad() - rw_normalangle.rad()); // CORRECT

    let sinea = anglea.sin(); // not correct?
    let sineb = angleb.sin();

    //            projection
    //m_iDistancePlayerToScreen = m_HalfScreenWidth / HalfFOV.GetTanValue();
    let p = 160.0 / (FRAC_PI_4).tan();
    let num = p * sineb; // oof a bit
    let den = rw_distance * sinea;

    let mut scale = num / den;

    if scale > MAX_SCALEFACTOR {
        scale = MAX_SCALEFACTOR;
    } else if MIN_SCALEFACTOR > scale {
        scale = MIN_SCALEFACTOR;
    }
    scale
}
