- [X] : Implement P_DamageMobj
- [ ] : P_KillMobj
- [ ] : Really need swept-volume collisions
- [ ] : Need the above for use-lines too
- [ ] : sound-server using rx/tx channels
- [ ] : HUD using rx/tx channels
- [ ] : angle_to_screen has an impact on sizing and scale
- [ ] : Load sprites
- [ ] : fixed-colourmap (special effects)
- [ ] : Fix the types in texture module
- [ ] : Colours seem off for darker areas?
- [ ] : Doom 2 M4 chopper window bottom plane is not done?

- [-] : P_UpdateSpecials (todo: level timer)
- [-] : EV_Teleport (todo: telefrag stuff)
- [-] : Recheck planes

- [X] : P_PlayerInSpecialSector
- [x] : FIXED: Fix Doom II map 4. There seems to be something causing a thinker list invalid ref?
- [x] : FIXED: Get the occasional crash from thinker slots running out before level start?
- [x] : Implement EV_BuildStairs
- [X] : Implement texture read and prep for use
- [X] : FIXED: they work I guess: E3M4 crushers
- [X] : Doom2 M9 lift textures
- [X] : upper of some textures are screwed
- [X] : door in spawn area (behind) is drawn incorrectly
- [X] : map 24 Doom 2, texture to right of open area is bad
- [X] : map 1 SIGIL, texture in front is borked
- [X] : e1m1 right lift upper texture not drawn
- [X] : e1m9 corner lifts aren't cutting bottom of texture?
- [X] : e3m4 crushers textures aren't culling correctly?
- [X] : e3m4, player can't fit. The proper setup of line back/front sectors affects this and now SubSectorMinMax needs work.
- [X] : animated textures (flattranslation, texturetranslation)
- [X] : EV_DoLockedDoor
- [X] : Skybox
- [X] : FIXED: Increased limits. Large sigil levels have rendering issues
- [X] : doom2 m8 LowerAndChange, not implemented yet
- [X] : Spans borked in E4M3 after first teleport (look at pad)
- [X] : render_bsp_node is way too grabby. It's pulling subsectors that arn't actually in view
- [X] : head-bob causes textures to jitter
- [X] : seg render topfrac doesn't seem quite right? Some texture render starts are offset up by one pixel?
- [X] : Load textures
- [X] : FIXED: some tme ago: Lump cache isn't actually being fully used, use it!

Game and doom-lib are now separate crates with these criteria:
- "game" controls input and ticcmd, rendering, and menu display
- "game" will also check state and show appropriate screens (like demo, intermission, menus)
- "doom-lib" is pure gamestate - things like level control, map objects, thinkers etc.

Refactor softwware renderer to use and Arc for texture share, pass in the `SoftwareRederer` to sub functions
such as in seg render.

Adjust the lines like this:
```rust
ev_build_stairs(line.clone(), StairKind::Turbo16, level);
line.special = 0;
```

Find a better way to do `let level = unsafe { &mut *thing.level };`
