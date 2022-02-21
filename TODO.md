[x] - FIXED: Fix Doom II map 4. There seems to be something causing a thinker list invalid ref?
[x] - FIXED: Get the occasional crash from thinker slots running out before level start?
[x] - Implement EV_BuildStairs
[ ] - Implement P_DamageMobj
[ ] - P_KillMobj
[ ] - Implement texture read and prep for use
[X] - FIXED: they work I guess: E3M4 crushers

Split the gameplay out in to a crate. The gameplay requires:
- `p_` files, these are things like thinkers, objects (mobj, lights, sector movers).
- the level data
- the info (state, sprite etc)