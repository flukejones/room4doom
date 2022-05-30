# Shooting stuff

The essence of shooting is tracing a path from V1 to V2 and seeing if we hit anything
along the way.

A thing initiates shooting by calling `P_AimLineAttack` first to see if we hit anything
then `P_LineAttack` to do the damage plus display trajectories.

In the call from `P_AimLineAttack` -> `P_PathTraverse(PTR_AimTraverse)`, the `PTR_AimTraverse`
function has the role of setting the target and the aimslope towards it.

`P_BulletSlope` is the function called when a player fires their weapon. Punching and
chainsaw have different methods (`A_Punch`, `A_Saw`), after which `P_GunShot` is called
to initiate the actual attack shot.

MISSILERANGE = 32*64*1.0;

## Line of sight

A lot of shooting and exploding action requires line-of-sight checks to be made to see if the target actually can be hit. The first thing that must be done is see if there is a possible target - this doesn't check LoS - then if a target is found we traverse the sectors between to see if LoS fails.

The LoS basically converts the targetting to a 2D plane side on with the shooter in one part and target in another, then uses good old graphing slope calculations to see if the targetting line passes over or below the wall lines.

The major part of this is `P_CrossSubsector`. This is called via chain 

1. `P_CheckSight`, checks the ject table to see if any sectors can reject the check early (precursor to PVS).
  - also sets up the starting Z height, top and bottom limits, and a trace
2. Proceed on to `P_CrossBSPNode`, this does a descent through the BSP, checking if the splitting lines are crossed then if a subsector is reached calls `P_CrossSubsector`

In the rust rewrite the the BSP descent is used for the first task of finding a target, this is `BSPTrace`, and it keeps a list of all subsectors crossed on the way to whichever point - this list can be reused, skipping the `P_CrossBSPNode` above. So we would call `P_CrossSubsector` on each of the subsectors in this list.

`P_CheckSight` is used in a number of places, almost all enemy except exploding:
- `PIT_RadiusAttack` (explody things, to check if the explosion actually can reach something)
- `P_CheckMeleeRange`
- `P_CheckMissileRange`
- `P_LookForPlayers`
- `A_Look`
- `A_Chase`
- `A_CPosRefire` (basically stand still and fire unless a random val returns)
- `A_SpidRefire` (as above)
- `A_Fire`
- `A_VileAttack`

Shooting by the player is done slightly differently as the player is restricted by actual display LoS. When the player shoots, first a target is looked for, then a check is done on the shot travel slope. The first part (`P_AimLineAttack`) does check walls along the way for solid lines then early exits if hit. If no solid line is hit and a target is found then the previously mentioned slope check proceeds.

Player aiming checks 3 times in slightly different player angles to see if a target can be found.

Both `P_AimLineAttack` and `P_LineAttack` are used for shooting.

## Player functions

The players ability to fire or change weapons is controled by state-machine much like the demon/general animation is. For example (`state_t`):

```
	{SPR_PISG, 0, 1, {A_WeaponReady}, S_PISTOL, 0, 0},		   // S_PISTOL
	{SPR_PISG, 0, 1, {A_Lower}, S_PISTOLDOWN, 0, 0},		   // S_PISTOLDOWN
	{SPR_PISG, 0, 1, {A_Raise}, S_PISTOLUP, 0, 0},			   // S_PISTOLUP
	{SPR_PISG, 0, 4, {NULL}, S_PISTOL2, 0, 0},				   // S_PISTOL1
	{SPR_PISG, 1, 6, {A_FirePistol}, S_PISTOL3, 0, 0},		   // S_PISTOL2
	{SPR_PISG, 2, 4, {NULL}, S_PISTOL4, 0, 0},				   // S_PISTOL3
	{SPR_PISG, 1, 5, {A_ReFire}, S_PISTOL, 0, 0},			   // S_PISTOL4
	{SPR_PISF, 32768, 7, {A_Light1}, S_LIGHTDONE, 0, 0},	   // S_PISTOLFLASH
```
where fields like `S_PISTOL` are the state number and the function pointer `A_WeaponReady` is what is called in the thinker turn.

- `A_WeaponReady`, checks if ticmd `BT_ATTACK` exists then shoots. Also does attack frame and weapon-swing
  - `P_FireWeapon`
- `A_ReFire`
- `A_CheckReload`
- `P_CheckAmmo`
- `A_Lower` and `A_Raise`
- `P_BringUpWeapon`

`P_FireWeapon` sets the player map object in to `S_PLAY_ATK1`, this would show the player sprite in attack position in multiplayer (or in a mirror if Doom had them).

`P_SetPsprite` calls the state function pointer (`state->action.acp2`), and this is called in every state change.

P_SetupPsprites

`P_MovePsprites` is called in thinking turn of player. This in turn calls `P_SetPsprite`, leading to callback function (and `A_WeaponReady` with its firing stuff)

## Method call chains

- A_FirePistol
  - P_BulletSlope
    - P_AimLineAttack
  - P_GunShot
    - P_LineAttack

- P_AimLineAttack
  - P_PathTraverse(PTR_AimTraverse)
- PTR_AimTraverse
  - P_LineOpening

- P_LineAttack
  - P_PathTraverse(PTR_ShootTraverse)

- PTR_ShootTraverse
  - P_ShootSpecialLine
  - P_LineOpening
  - P_SpawnPuff
  - P_SpawnBlood
  - P_DamageMobj


```C
//
// P_AimLineAttack
//
fixed_t P_AimLineAttack(mobj_t *t1, angle_t angle, fixed_t distance) {
  fixed_t x2;
  fixed_t y2;

  angle >>= ANGLETOFINESHIFT;
  shootthing = t1;

  x2 = t1->x + (distance >> FRACBITS) * finecosine[angle];
  y2 = t1->y + (distance >> FRACBITS) * finesine[angle];
  // Used in PTR_ShootTraverse to determine the slope up/down towards target
  // this has the visual effect of bullet-holes appearing in correct place or a
  // missile tracking up/down to the target.
  shootz = t1->z + (t1->height >> 1) + 8 * FRACUNIT;

  // can't shoot outside view angles
  topslope = 100 * FRACUNIT / 160;
  bottomslope = -100 * FRACUNIT / 160;

  attackrange = distance;
  linetarget = NULL;

  P_PathTraverse(t1->x, t1->y, x2, y2, PT_ADDLINES | PT_ADDTHINGS,
                 PTR_AimTraverse);

  if (linetarget)
    return aimslope;

  return 0;
}

//
// P_LineAttack
// If damage == 0, it is just a test trace
// that will leave linetarget set.
//
void P_LineAttack(mobj_t *t1, angle_t angle, fixed_t distance, fixed_t slope,
                  int damage) {
  fixed_t x2;
  fixed_t y2;

  angle >>= ANGLETOFINESHIFT;
  shootthing = t1;
  la_damage = damage;
  x2 = t1->x + (distance >> FRACBITS) * finecosine[angle];
  y2 = t1->y + (distance >> FRACBITS) * finesine[angle];
  shootz = t1->z + (t1->height >> 1) + 8 * FRACUNIT;
  attackrange = distance;
  aimslope = slope;

  P_PathTraverse(t1->x, t1->y, x2, y2, PT_ADDLINES | PT_ADDTHINGS,
                 PTR_ShootTraverse);
}
```