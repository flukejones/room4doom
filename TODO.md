- [ ] Really need swept-volume collisions
- [ ] sound-server using rx/tx channels
- [ ] HUD using rx/tx channels
- [ ] angle_to_screen has an impact on sizing and scale
- [ ] Load sprites
- [ ] fixed-colourmap (special effects)
- [ ] Fix the types in texture module
- [ ] Colours seem off for darker areas?
- [ ] Menu screens
- [ ] Automap
- [ ] Status bar drawing
- [ ] Sound state machine on new thread

- [-] Sprites colourmap/light selection
- [-] P_TouchSpecialThing
- [-] Shooting
- [-] Shooting special items/lines
- [-] Thing collisions
- [-] P_KillMobj
- [-] P_UpdateSpecials (todo: level timer)
- [-] EV_Teleport (todo: telefrag stuff)
- [-] Recheck planes

- [+] A method to find all SubSectors in a radius
        This is required for explosion type stuff
- [+] FIXED: Needed to pass actual endpoint Vec2 for check: Thing collisions are basic, they need "drift" off the center of a thing
- [+] FIXED: incorrect mover. E5M1 second "eye" to shoot doesn't react
- [+] Sprite rotations
- [+] FIXED: Sprite sorting: impl Ord on VisSprite for rust iiter sorting
- [+] Sprite clipping (issues with planes, doors)
- [+] FIXED: Teleport telefrag killing entire sector not just spawn point: Radius check
- [+] Implement P_DamageMobj
- [+] Doom 2 M4 chopper window bottom plane is not done?
- [+] E1M4 round room, floor strip inside of door steps isn't getting a visplane?
        + Same as D2M4. Both checked in Slade. Fixed by ignoring missing texture to ensure
          visplane is built.
- [+] Fixed: needed to null-check :-| Doom 2 teleports cause segfault
- [+] P_PlayerInSpecialSector
- [+] FIXED: Fix Doom II map 4. There seems to be something causing a thinker list invalid ref?
- [+] FIXED: Get the occasional crash from thinker slots running out before level start?
- [+] Implement EV_BuildStairs
- [+] Implement texture read and prep for use
- [+] FIXED: they work I guess: E3M4 crushers
- [+] Doom2 M9 lift textures
- [+] upper of some textures are screwed
- [+] door in spawn area (behind) is drawn incorrectly
- [+] map 24 Doom 2, texture to right of open area is bad
- [+] map 1 SIGIL, texture in front is borked
- [+] e1m1 right lift upper texture not drawn
- [+] e1m9 corner lifts aren't cutting bottom of texture?
- [+] e3m4 crushers textures aren't culling correctly?
- [+] e3m4, player can't fit. The proper setup of line back/front sectors affects this and now SubSectorMinMax needs work.
- [+] animated textures (flattranslation, texturetranslation)
- [+] EV_DoLockedDoor
- [+] Skybox
- [+] FIXED: Increased limits. Large sigil levels have rendering issues
- [+] doom2 m8 LowerAndChange, not implemented yet
- [+] Spans borked in E4M3 after first teleport (look at pad)
- [+] render_bsp_node is way too grabby. It's pulling subsectors that arn't actually in view
- [+] head-bob causes textures to jitter
- [+] seg render topfrac doesn't seem quite right? Some texture render starts are offset up by one pixel?
- [+] Load textures
- [+] FIXED: some tme ago: Lump cache isn't actually being fully used, use it!

Game and doom-lib are now separate crates with these criteria:
- "game" controls input and ticcmd, rendering, and menu display
- "game" will also check state and show appropriate screens (like demo, intermission, menus)
- "doom-lib" is pure gamestate - things like level control, map objects, thinkers etc.

Adjust the lines like this:
```rust
ev_build_stairs(line.clone(), StairKind::Turbo16, level);
line.special = 0;
```

Find a better way to do `let level = unsafe { &mut *thing.level };`
