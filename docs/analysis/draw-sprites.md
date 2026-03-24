# Sprites

Sprites in Doom are used to represent static objects, items, weapons, projectiles, and demons.

The call chain is:
1. `R_InitSprites`, taking an argument as a list of sprite names to use
2. `R_InitSpriteDefs`, builds the definitions required to use sprites (such as frames and angles)
3. `R_InstallSpriteLump`, builds the frames used for one animation (as arg)

The above is intiated by `P_Init` -> `R_InitSprites` and is started on game start, showing `P_Init: Init Playloop state.` in DOS. The sprite names list is a const array `sprnames` in `info.c`. This name array is currently 138 names.

## Player sprites: Overlays
- pspdef_t
- psprnum_t
- R_DrawPlayerSprites

## Drawing
First:

- BSP traverse --> R_AddSprites -> R_ProjectSprite -> R_NewVisSprite (creates a link on list)

This makes a list of vissprite_t which is itself a doubly linked list `vissprites[MAXVISSPRITES]`, this is
then sorted in the first step of drrawing. Drawing then proceeds from the head of the sorted links. It's pretty fast and efficient as it deals only with pointers as opposed to copying a large struct in/out of spots.

R_ProjectSprite relies on the view angle for cos/sin.

projection is the screen width / 2;

Second, in R_RenderPlayerView:

- R_DrawMasked
  --> R_SortVisSprites
  --> R_DrawSprite
  --> R_RenderMaskedSegRange
  -- > R_DrawPlayerSprites (overlay sprites like weapons)
    --> R_DrawPSprite


// TODO:
- negonearray, what it does
- vissprites,
- vissprite_p
- vissprite_t
- newvissprite
- sprites


// Code TODO:
- negonearray should be in rendering
- R_InitSpriteDefs in lib
- R_InstallSpriteLump in lib
- sprites in lib/src/pic/

thing->frame is masked with various things.