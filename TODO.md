[x] - FIXED: Fix Doom II map 4. There seems to be something causing a thinker list invalid ref?
[x] - FIXED: Get the occasional crash from thinker slots running out before level start?
[x] - Implement EV_BuildStairs
[ ] - Implement P_DamageMobj
[ ] - P_KillMobj
[ ] - Implement texture read and prep for use
[X] - FIXED: they work I guess: E3M4 crushers
[ ] - Doom2 M9 lift textures

[X] - Load textures
[ ] - Load sprites

Game and doom-lib are now separate crates with these criteria:
- "game" controls input and ticcmd, rendering, and menu display
- "game" will also check state and show appropriate screens (like demo, intermission, menus)
- "doom-lib" is pure gamestate - things like level control, map objects, thinkers etc.
