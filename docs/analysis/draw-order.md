The main drawing of solid segs routine creates a list of
columns which are masked (texture with transparency), and
also marks the top/bottom of each column if it's a portal.

Drawing proceeds:
1. R_RenderBSPNode
2. R_DrawPlanes
3. R_DrawMasked
    a. R_SortVisSprites
    b. R_DrawSprite
    c. R_RenderMaskedSegRange
    d. R_DrawPlayerSprites

Masked columns share data with sprite rendering and plane rendering.

`r_plane.c` contains:

```c
#define MAXOPENINGS	SCREENWIDTH*64     // generally 320*64
short			openings[MAXOPENINGS]; // uint8 basically, because screen width is under 255
short*			lastopening;
```

`R_ClearPlanes` is called on each frame to clear plane stuff, including `lastopening = openings;`
to set `lastopening` to the start of `openings` array.

And in `r_segs.c`:

This file keeps a pointer `*maskedtexturecol` in to `openings`

For each `R_StoreWallRange` call:

```c
    maskedtexture = true;
    // Each of these are pointers
    ds_p->maskedtexturecol = maskedtexturecol = lastopening - rw_x;
    lastopening += rw_stopx - rw_x;
```

where `lastopening` is a pointer to a location in `openings`.

// TODO: sprite clipping info

