# TODO

- [ ] Check that WadSubSector parse type is correct. Is it really i16?

## BUGS

- [ ] Demons don't wake on spawn when they should?
- [x] Telefrags don't work. Does work but ignored for demons
  - [ ] Push demons apart if they are spawned together (teleports) or maybe prevent them teleporting if it's not clear?
- [-] Demons shouldn't open locked doors (Actual Doom isue)
- [x] Floating blood? Happens near doors and looks like an error between hi/lo clipping (can't reproduce since massive rework)
- [-] Monster rotations when walking towards off-by-one? Unsure
- [-] Aim/shoot tries to hit low enemy even if portal blocks view
- [x] panicked at 'attempt to add with overflow', render-soft/src/segs.rs:477:18 -- `yl = (self.topfrac + HEIGHTUNIT) as i32 + 1;` -- Nuked it all with visplane removal
- [ ] Total kills: doesn't count for player if they shoot a barrel and that barrel kills a demon.

## Graphics

- [ ] OpenGL renderer
- [ ] Vulkan renderer
- [x] Widescreen (software)
  - [x] Correct FOV for proper 4:3 scale drawing (segs/flats)
  - [x] Adjust all sprites to correct aspect of screen
  - [x] apply ratio scaling to scale_from_view_angle()
  - [x] apply ratio scaling to draw_column_style_flats() distscale var
  - [x] apply ratio scaling to projection in bsp module
  - [x] apply ratio scaling to fov_scale in bsp module
  - [ ] Adjust lightmaps to match
  - [ ] Add display res selection
  - [ ] Menus and HUD scaling + ratio correction

## GAMEPLAY STUFF

- [x] Nightmare respawn
  - [x] P_RespawnSpecials()
  - [x] Add to queue in P_RemoveMobj()
  - [x] Respawn demons
  - [ ] Adjust trooper/bruiser speeds (gamestate)
- [ ] Limit skull count on map from elementals shooting them out
- [ ] Wad mobj flags a different to in-game info flags...
- [ ] Implement UMAPINFO support https://doomwiki.org/wiki/UMAPINFO
- [-] Really need swept-volume collisions (done half-arsed)
- [-] P_KillMobj (target/source stuff)
- [x] P_UpdateSpecials (todo: level timer)
- [ ] gameplay interact: needs access to `players[]`

## CORE/FEATURE FUNCTIONALITY

- [x] All gameplay features for Doom and Doom II
- [x] Status bar (face, health, armour, keys, ammo, cheats)
- [x] Powerup/damage palette effects
- [x] Thinkers for all things
- [-] HUD (Done except for multiplayer chat)
  - [x] Show "Found a secret" message
- [-] Menu screens (partial. New game, quit)
- [x] Intermissions and finale
  - [x] Stats
  - [x] Episode end text
  - [ ] Bunny screen
  - [ ] Doom II cast
- [ ] Automap
- [x] Demo playback
  - [ ] tic cmds are not deterministic due to movement and position being f32?
        The movement speed and friction is correct. Lets look at the timing of cmds within the main loop
- [ ] Save/load game
- [-] Sound:
  - [x] Verify positional sound
  - [x] Verify distance and cutoff
  - [x] Check the volumes (had to divide midi track vol in half)
  - [ ] Add the pitch shift
  - [ ] Maybe use the `usefulness` field..
  - [ ] OPL2 emulation (a lot of work here)
  - [ ] Load music from extra wads (needs `UMAPINFO` parsing)

## IMPROVEMENTS

- [ ] Make responders use ticcmds to ensure they are generic
- [ ] Analyse the game further to allow more use of `unwrap_unchecked()` where we know for sure the data is initialised and valid.
- [ ] The Thinker data access methods really should return `Option<T>`
- [ ] Thinkers: For inner data get, add a compile-time cfg opt to panic, or return Option.
- [ ] Need to reset sector sound targets if player dies
- [ ] refactor the stair-builder loop to use lines iter. It currently needs two mutable accesses to data in a loop
  - let target = unsafe { (\*target).object_mut().mobj() };// make a shortcut for this
- [ ] Make skulls attempt to scale inanimate objects. This is related to objects taking the full Z-axis
      . It's (currently) not possible to "step" on top of another object
- [ ] Where aiming/shooting at an object the shooter should be a point while target + radius is considered

## BOOM stuff to consider

- [ ] Lump name `SWITCHES`, extend the switch list
- [ ] Lump name `ANIMATED`, extend the animated texture list
- [ ] Lump name `TRANMAP` for transparency?
- [ ] New linedef flag, bit 9, PassThru, that allows one push to activate several functions simultaneously.
- [ ] Generalized linedef types added in range 2F80H - 7FFFH
- [ ] Generalized sector types using bits 5-11 of the sector type field

## DONE

- [x] Fix the types in texture module
- [x] P_TeleportMove teleport_move
- [x] Check textures are correctly sized an aligned
- [x] Find the cause of missing draw columns in large maps like e6m6 (float precision in `angle_to_screen()`)
- [x] Convert static arrays in much of the renderer structs to Vec since at high res they can potentially overflow the stack
- [x] Statusbar doomguy face god-mode
- [x] BFG spray
- [x] Pain elemental attacks and die
- [x] Archvile attacks and chase
- [x] Archvile raise the dead
- [x] Clipping under sprites (like hanging things)
- [x] Step over sprites that are short
- [x] Prevent player getting stuck in door if standing next to when close (change_sector() needs to check object radius is crossing over segs)
- [x] E1M5 lamps are on window height not floor (they get raised by the lifting floor in other sector: Note: something with height clipping height_clip() and floorz)
- [x] E2M1 after second teleport there is a missing plane line at top of floor drop
- [x] Doom 2 M13 `thread '<unnamed>' panicked at 'called `Result::unwrap()`on an`Err` value: "Parameter 'size' is invalid"', sound/sdl2/src/lib.rs:344:74`
- [x] M30 `thread 'main' panicked at 'attempt to divide by zero', /home/luke/Projects/room4doom/intermission/doom/src/lib.rs:257:40`
- [x] Step over things if higher
- [x] e2m8 `thread 'main' panicked at 'index out of bounds: the len is 0 but the index is 0', render/software/src/things.rs:401:21`
- [x] Revenant rockets head off in wrong direction
- [x] angle_to_screen has an impact on sizing and scale (note: needed a float tweak and floor())
- [x] Statusbar doomguy face attacked-from-angle
- [x] Screen wipe
- [x] Shade screen with red palettes for increased damage (not the "took damage" flash)
- [x] Player sprite isn't rendered with 'static' if player is invisible
- [x] Chaingun guy doesn't do burst-fire (shoots, but not sustained) (remove invalid state set)
- [x] Monsters don't activate when player is real close (related to angle fix?)
- [x] Colours seem off for darker areas? (was incorrect float conversion on zlight table creation)
- [x] Re-implement warp
- [x] Player can get stuck in doors if the close on the edge of bounds
      the thinker list fucnction for the door sector runs it doesn't see it
- [x] Shadow-pinkies (use p_random instead of rewriting framebuffer)
- [x] Sprites colourmap/light selection (spectors)
- [x] Average the lines in a sector for sound origins (determined by center of sector AABB)
- [x] Don't shoot sky
- [x] Explosions shouldn't hit above or below (sight check)
- [x] E1M5 Candelebra not on ground?
- [x] Lift sounds for E5M3 don't stop
- [x] Shots from demons don't push the player
- [x] Sight angle incorrect for any mobj not 90-270 degrees:

```rust
if !all_around {
    let angle = point_to_angle_2(xy, self.xy).rad() - self.angle.rad();
    if angle.abs() > PI && self.xy.distance(xy) > MELEERANGE {
        continue;
    }
}
```

changed to:

```rust
if !all_around {
    let xy = point_to_angle_2(xy, self.xy).unit(); // Using a unit vector to remove world
    let v1 = self.angle.unit();                    // Get a unit from mobj angle
    let angle = v1.angle_between(xy).abs();        // then use glam to get angle between (it's +/- for .abs())
    if angle > FRAC_PI_2 && self.xy.distance(xy) > MELEERANGE {
        continue;
    }
}
```

- [x] Reduce use of `*player` and unsafe
- [x] Sector sound origin for specials
  - [x] iterate sector lines to find max bounding box for sector and set sound_origin
- [x] INFLOAT, floatok, skulls
- [x] EV_DoDonut (E2M2)
- [x] EV_StopPlat - requires tracking some active platforms.
- [x] Some buttons no-longer change? (Shouldn't have been if else-if)
- [x] P_RecursiveSound - use flood through sectors. P_NoiseAlert
- [x] Sector sound targets - done with P_RecursiveSound
- [x] Recheck planes (seems correct now)
- [x] Make powerups count down
- [x] Load sprites
- [x] fixed-colourmap (special effects like rad suit, invuln, player-hit)
- [x] Sound state machine on new thread
- [x] Move "extra light" arg up the call chain to lightmap selection functions
- [x] P_TouchSpecialThing
- [x] Shooting
- [x] Shooting special items/lines
- [x] Thing collisions
- [x] EV_Teleport (todo: telefrag stuff)
- [x] Sound distances (SDL helpers?)
- [x] sound-server using rx/tx channels
- [x] track the playing sound sources and the channels they are on
- [x] Music:
  - [x] Convert MUS to MIDI
  - [x] Play using basic SDL2 + fluidsynth
  - [x] Play using GUS via SDL2 + timidity (`DMXGUS` lump)
- [x] A method to find all SubSectors in a radius
      This is required for explosion type stuff
- [x] FIXED: Needed to pass actual endpoint Vec2 for check: Thing collisions are basic, they need "drift" off the center of a thing
- [x] FIXED: incorrect mover. E5M1 second "eye" to shoot doesn't react
- [x] Sprite rotations
- [x] FIXED: Sprite sorting: impl Ord on VisSprite for rust iiter sorting
- [x] Sprite clipping (issues with planes, doors)
- [x] FIXED: Teleport telefrag killing entire sector not just spawn point: Radius check
- [x] Implement P_DamageMobj
- [x] Doom 2 M4 chopper window bottom plane is not done?
- [x] E1M4 round room, floor strip inside of door steps isn't getting a visplane? + Same as D2M4. Both checked in Slade. Fixed by ignoring missing texture to ensure
      visplane is built.
- [x] Fixed: needed to null-check :-| Doom 2 teleports cause segfault
- [x] P_PlayerInSpecialSector
- [x] FIXED: Fix Doom II map 4. There seems to be something causing a thinker list invalid ref?
- [x] FIXED: Get the occasional crash from thinker slots running out before level start?
- [x] Implement EV_BuildStairs
- [x] Implement texture read and prep for use
- [x] FIXED: they work I guess: E3M4 crushers
- [x] Doom2 M9 lift textures
- [x] upper of some textures are screwed
- [x] door in spawn area (behind) is drawn incorrectly
- [x] map 24 Doom 2, texture to right of open area is bad
- [x] map 1 SIGIL, texture in front is borked
- [x] e1m1 right lift upper texture not drawn
- [x] e1m9 corner lifts aren't cutting bottom of texture?
- [x] e3m4 crushers textures aren't culling correctly?
- [x] e3m4, player can't fit. The proper setup of line back/front sectors affects this and now SubSectorMinMax needs work.
- [x] animated textures (flattranslation, texturetranslation)
- [x] EV_DoLockedDoor
- [x] Skybox
- [x] FIXED: Increased limits. Large sigil levels have rendering issues
- [x] doom2 m8 LowerAndChange, not implemented yet
- [x] Spans borked in E4M3 after first teleport (look at pad)
- [x] render_bsp_node is way too grabby. It's pulling subsectors that arn't actually in view
- [x] head-bob causes textures to jitter
- [x] seg render topfrac doesn't seem quite right? Some texture render starts are offset up by one pixel?
- [x] Load textures
- [x] FIXED: some tme ago: Lump cache isn't actually being fully used, use it!

Game and doom-lib are now separate crates with these criteria:
"game" controls input and ticcmd, rendering, and menu display
"game" will also check state and show appropriate screens (like demo, intermission, menus)
"doom-lib" is pure gamestate things like level control, map objects, thinkers etc.

Adjust the lines like this:

```rust
ev_build_stairs(line.clone(), StairKind::Turbo16, level);
line.special = 0;
```

Find a better way to do `let level = unsafe { &mut *thing.level };`
