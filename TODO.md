[x] - FIXED: Fix Doom II map 4. There seems to be something causing a thinker list invalid ref?
[x] - FIXED: Get the occasional crash from thinker slots running out before level start?
[x] - Implement EV_BuildStairs
[ ] - Implement P_DamageMobj
[ ] - P_KillMobj
[X] - Implement texture read and prep for use
[X] - FIXED: they work I guess: E3M4 crushers
[X] - Doom2 M9 lift textures
[X] - upper of some textures are screwed
[X] - door in spawn area (behind) is drawn incorrectly
[X] - map 24 Doom 2, texture to right of open area is bad
[X] - map 1 SIGIL, texture in front is borked
[X] - e1m1 right lift upper texture not drawn
[ ] - e1m9 corner lifts aren't cutting bottom of texture?
[ ] - e3m4 crushers textures aren't culling correctly?
[X] - e3m4, player can't fit. The proper setup of line back/front sectors affects this and
      now SubSectorMinMax needs work.
[ ] - animated textures (flattranslation, texturetranslation)
[ ] - P_UpdateSpecials
[-] - EV_Teleport
[X] - EV_DoLockedDoor
[ ] - Skybox
[X] - FIXED: Increased limits. Large sigil levels have rendering issues
[ ] - Really need swept-volume collisions
[ ] - Need the above for use-lines too
[ ] - sound-server using rx/tx channels
[ ] - HUD using rx/tx channels
[ ] - doom2 m8 LowerAndChange, not implemented yet
[ ] - Spans borked in E4M3 after first teleport (look at pad)

[X] - seg render topfrac doesn't seem quite right? Some texture render starts are offset up by one pixel?
[ ] - additional to above, scale_from_view_angle (R_ScaleFromGlobalAngle) results are floats so may be differing to the OG fixed
[ ] - angle_to_screen has an impact on sizing and scale

[X] - Load textures
[ ] - Load sprites
[ ] - fixed-colourmap (special effects)
[ ] - Lump cache isn't actually being fully used, use it!
[ ] - Fix the types in texture module
[ ] - Recheck planes
[ ] - Colours seem off for darker areas?


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

- e2m4 quick teleport test
- doom2 m4 redkey area teleport test

```rust
mid = self.pixhigh - 1.0;
...
                    if mid >= yl {
                        if seg.sidedef.toptexture != usize::MAX {
                            let texture_column =
                                textures.texture_column(seg.sidedef.toptexture, texture_column);
                            let mut dc = DrawColumn::new(
                                texture_column,
                                textures.get_light_colourmap(
                                    &seg.v1,
                                    &seg.v2,
                                    self.wall_lights,
                                    self.rw_scale,
                                ),
                                dc_iscale,
                                self.rw_x,
                                self.rw_toptexturemid,
                                yl as i32, // -1 affects the top of lines without mid texture
                                // HERE IS A SOURCE OF ISSUES
                                mid as i32 + 1,
                            );
                            dc.draw_column(textures, canvas);
                        }

                        rdata.portal_clip.ceilingclip[self.rw_x as usize] = mid;
                    }
```
yl, yh, pishigh, pixlow - all not quite right.

