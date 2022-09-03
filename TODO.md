# TODO

- [X] Convert static arrays in much of the renderer structs to Vec since at high res they
      can potentially overflow the stack
- [X] Archvile attacks and chase
- [ ] Archvile raise the dead
- [ ] Pain elemental attacks and die
- [ ] Doom 2 boss brain thing
  - [x] Can't target it, probably because it's not visible? There's that thing in front?
- [ ] Wad mobj flags a different to in-game info flags...
- [ ] Implement UMAPINFO support https://doomwiki.org/wiki/UMAPINFO

## FUNCTIONALITY

- [-] Menu screens (partial. Other functionality required before completion)
- [-] Intermissions and finale (intermission has no animated text. Finale needs cast for Doom II, and bunny for Doom)
- [X] Screen wipe
- [X] HUD (Done except for multiplayer chat)
- [ ] Automap
- [X] Status bar
- [ ] Demo playback
- [ ] Save/load game
- [ ] Sound:
  - [ ] Verify positional sound
  - [ ] Verify distance and cutoff
  - [ ] Check the volumes (had to divide midi track vol in half)
  - [ ] Add the pitch shift
  - [ ] Maybe use the `usefulness` field..
  - [ ] OPL2 emulation (a lot of work here)

## BUGS

- [ ] Aim/shoot tries to hit low enemy even if portal blocks view
- [X] Doom 2 M13 `thread '<unnamed>' panicked at 'called `Result::unwrap()` on an `Err` value: "Parameter 'size' is invalid"', sound/sdl2/src/lib.rs:344:74`
- [X] M30 `thread 'main' panicked at 'attempt to divide by zero', /home/luke/Projects/room4doom/intermission/doom/src/lib.rs:257:40`
- [ ] panicked at 'attempt to add with overflow', render-soft/src/segs.rs:477:18 -- `yl = (self.topfrac + HEIGHTUNIT) as i32 + 1;`
- [X] e2m8 `thread 'main' panicked at 'index out of bounds: the len is 0 but the index is 0', render/software/src/things.rs:401:21`
- [X] Revenant rockets head off in wrong direction
- [ ] Don't pickup armour shards if have max armour
- [ ] Total kills: doesn't count for player if they shoot a barrel and that barrel kills a demon.
- [-] Demons shouldn't open locked doors (Actual Doom isue)
- [ ] angle_to_screen has an impact on sizing and scale
- [ ] Fix the types in texture module
- [ ] The fix for players stuck in doors is causing floaty blood

## IMPROVEMENTS

- [ ] Statusbar doomguy face attacked-from-angle
- [ ] Make responders use ticcmds to ensure they are generic
- [ ] Analyse the game further to allow more use of `unwrap_unchecked()` where we know for sure the
  data is initialised and valid.
- [ ] The Thinker data access methods really should return `Option<T>`
- [ ] Thinkers: For inner data get, add a compile-time cfg opt to panic, or return Option.
- [ ] Need to reset sector sound targets if player dies
- [ ] refactor the stair-builder loop to use lines iter. It currently needs two mutable accesses to data in a loop
    - let target = unsafe { (*target).object_mut().mobj() };// make a shortcut for this
- [ ] Make skulls attempt to scale inanimate objects. This is related to objects taking the full Z-axis
    . It's (currently) not possible to "step" on top of another object
- [ ] Step over things if higher
- [ ] Where aiming/shooting at an object the shooter should be a point while target + radius is considered

## BOOM stuff to consider

- [ ] Lump name `SWITCHES`, extend the switch list
- [ ] Lump name `ANIMATED`, extend the animated texture list
- [ ] Lump name `TRANMAP` for transparency?
- [ ] New linedef flag, bit 9, PassThru, that allows one push to activate several functions simultaneously.
- [ ] Generalized linedef types added in range 2F80H - 7FFFH
- [ ] Generalized sector types using bits 5-11 of the sector type field

## PARTIAL-COMPLETE

- [-] Really need swept-volume collisions (done half-arsed)
- [-] P_KillMobj (target/source stuff)
- [-] P_UpdateSpecials (todo: level timer)

## DONE

<<<<<<< HEAD
- [X] Shade screen with red palettes for increased damage (not the "took damage" flash)
- [X] Player sprite isn't rendered with 'static' if player is invisible
- [X] Chaingun guy doesn't do burst-fire (shoots, but not sustained) (remove invalid state set)
- [X] Monsters don't activate when player is real close (related to angle fix?)
- [X] Colours seem off for darker areas? (was incorrect float conversion on zlight table creation)
- [X] Re-implement warp
- [X] Player can get stuck in doors if the close on the edge of bounds
the thinker list fucnction for the door sector runs it doesn't see it
- [X] Shadow-pinkies (use p_random instead of rewriting framebuffer)
- [X] Sprites colourmap/light selection (spectors)
- [X] Average the lines in a sector for sound origins (determined by center of sector AABB)
- [X] Don't shoot sky
- [X] Explosions shouldn't hit above or below (sight check)
- [X] E1M5 Candelebra not on ground?
- [X] Lift sounds for E5M3 don't stop
- [X] Shots from demons don't push the player
- [X] Sight angle incorrect for any mobj not 90-270 degrees:
=======
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

>>>>>>> 27a03f379dfc (Add last few Doom II functions)
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
- [X] Reduce use of `*player` and unsafe
- [X] Sector sound origin for specials
    - [X] iterate sector lines to find max bounding box for sector and set sound_origin
- [X] INFLOAT, floatok, skulls
- [X] EV_DoDonut (E2M2)
- [X] EV_StopPlat - requires tracking some active platforms.
- [X] Some buttons no-longer change? (Shouldn't have been if else-if)
- [X] P_RecursiveSound - use flood through sectors. P_NoiseAlert
- [X] Sector sound targets - done with P_RecursiveSound
- [X] Recheck planes (seems correct now)
- [X] Make powerups count down
- [X] Load sprites
- [X] fixed-colourmap (special effects like rad suit, invuln, player-hit)
- [X] Sound state machine on new thread
- [X] Move "extra light" arg up the call chain to lightmap selection functions
- [X] P_TouchSpecialThing
- [X] Shooting
- [X] Shooting special items/lines
- [X] Thing collisions
- [X] EV_Teleport (todo: telefrag stuff)
- [X] Sound distances (SDL helpers?)
- [X] sound-server using rx/tx channels
- [X] track the playing sound sources and the channels they are on
- [X] Music:
  - [X] Convert MUS to MIDI
  - [X] Play using basic SDL2 + fluidsynth
  - [X] Play using GUS via SDL2 + timidity (`DMXGUS` lump)
- [X] A method to find all SubSectors in a radius
        This is required for explosion type stuff
- [X] FIXED: Needed to pass actual endpoint Vec2 for check: Thing collisions are basic, they need "drift" off the center of a thing
- [X] FIXED: incorrect mover. E5M1 second "eye" to shoot doesn't react
- [X] Sprite rotations
- [X] FIXED: Sprite sorting: impl Ord on VisSprite for rust iiter sorting
- [X] Sprite clipping (issues with planes, doors)
- [X] FIXED: Teleport telefrag killing entire sector not just spawn point: Radius check
- [X] Implement P_DamageMobj
- [X] Doom 2 M4 chopper window bottom plane is not done?
- [X] E1M4 round room, floor strip inside of door steps isn't getting a visplane?
        + Same as D2M4. Both checked in Slade. Fixed by ignoring missing texture to ensure
          visplane is built.
- [X] Fixed: needed to null-check :-| Doom 2 teleports cause segfault
- [X] P_PlayerInSpecialSector
- [X] FIXED: Fix Doom II map 4. There seems to be something causing a thinker list invalid ref?
- [X] FIXED: Get the occasional crash from thinker slots running out before level start?
- [X] Implement EV_BuildStairs
- [X] Implement texture read and prep for use
- [X] FIXED: they work I guess: E3M4 crushers
- [X] Doom2 M9 lift textures
- [X] upper of some textures are screwed
- [X] door in spawn area (behind) is drawn incorrectly
- [X] map 24 Doom 2, texture to right of open area is bad
- [X] map 1 SIGIL, texture in front is borked
- [X] e1m1 right lift upper texture not drawn
- [X] e1m9 corner lifts aren't cutting bottom of texture?
- [X] e3m4 crushers textures aren't culling correctly?
- [X] e3m4, player can't fit. The proper setup of line back/front sectors affects this and now SubSectorMinMax needs work.
- [X] animated textures (flattranslation, texturetranslation)
- [X] EV_DoLockedDoor
- [X] Skybox
- [X] FIXED: Increased limits. Large sigil levels have rendering issues
- [X] doom2 m8 LowerAndChange, not implemented yet
- [X] Spans borked in E4M3 after first teleport (look at pad)
- [X] render_bsp_node is way too grabby. It's pulling subsectors that arn't actually in view
- [X] head-bob causes textures to jitter
- [X] seg render topfrac doesn't seem quite right? Some texture render starts are offset up by one pixel?
- [X] Load textures
- [X] FIXED: some tme ago: Lump cache isn't actually being fully used, use it!

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
