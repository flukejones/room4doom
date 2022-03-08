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
[ ] - animated textures
[ ] - EV_Teleport
[ ] - EV_DoLockedDoor
[ ] - Skybox
[ ] - Large sigil levels have rendering issues
[ ] - Really need swept-volume collisions
[ ] - Need the above for use-lines too
[ ] - sound-server using rx/tx channels
[ ] - HUD using rx/tx channels

[X] - seg render topfrac doesn't seem quite right? Some texture render starts are offset up by one pixel?
[ ] - additional to above, scale_from_view_angle (R_ScaleFromGlobalAngle) results are floats so may be differing to the OG fixed
[ ] - angle_to_screen has an impact on sizing and scale

[X] - Load textures
[ ] - Load sprites
[ ] - fixed-colourmap (special effects)

Game and doom-lib are now separate crates with these criteria:
- "game" controls input and ticcmd, rendering, and menu display
- "game" will also check state and show appropriate screens (like demo, intermission, menus)
- "doom-lib" is pure gamestate - things like level control, map objects, thinkers etc.

`self.players[self.consoleplayer].cheats = Cheat::Noclip as u32;` is set in `game.rs`
