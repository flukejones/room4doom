# TODO

## FUNCTIONALITY

- [-] Menu screens (partial)
- [-] Intermissions and finale (intermission has no animated text. No finale screens yet)
- [+] Screen wipe
- [ ] HUD (gameplay crate inserts messages in to player struct)
- [ ] Automap
- [+] Status bar drawing (framework in place)
- [ ] Save/load game

## BUGS

- [+] Monsters don't activate when player is real close (related to angle fix?)
- [ ] Don't pickup armour shards if have max armour
- [ ] Total kills seems incorrect
- [-] Demons shouldn't open locked doors (Actual Doom isue)
- [ ] panicked at 'attempt to add with overflow', render-soft/src/segs.rs:477:18 -- `yl = (self.topfrac + HEIGHTUNIT) as i32 + 1;`
- [ ] angle_to_screen has an impact on sizing and scale
- [ ] Fix the types in texture module
- [+] Colours seem off for darker areas? (was incorrect float conversion on zlight table creation)
- [ ] The fix for players stuck in doors caused floaty blood

## IMPROVEMENTS

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
- [ ] Shade screen with red palettes for increased damage (not the "took damage" flash)

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
