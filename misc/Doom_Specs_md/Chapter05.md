# CHAPTER [5]: Pictures' Format
The great majority of the entries if the directory reference resources that are in a special picture format. The same format is used for the sprites (monsters, items), the wall patches, and various miscellaneous pictures for the status bar, menu text, inter-level map, etc. The floor and ceiling textures are NOT in this format, they are raw data; see chapter [6].

After much experimenting, it seems that sprites and floors cannot be added or replaced via pwad files. However, wall patches can (whew!). This is apparently because all the sprites' entries must be in one "lump", in the `IWAD` file, between the `S_START` and `S_END` entries. And all the floors have to be listed between `F_START` and `F_END`. If you use those entries in pwads, either nothing will happen, or an error will occur. There are also `P_START` and `P_END` entries in the directory, which flank the wall patch names, so how come they work in pwads? I think it is somehow because of the `PNAMES` resource, which lists all the wall patch names that are to be used. Too bad there aren't `SNAMES` and `FNAMES` resources!

It is still possible to change and manipulate the sprites and floors, its just more difficult to do, and very difficult to figure out a scheme for potential distribution of changes. The `DOOM.WAD` file must be changed, and that is a pain.

All the sprites follow a naming scheme. The first four letters are the sprite's name, or and abbreviation. `TROO` is for imps, `BKEY` is for the blue key, etc. See [4-2-1] for a list of them.

For most things, the unanimated ones, the next two characters of the sprite's name are `A0`, like `SUITA0`, the radiation suit. For simple animated things, there will be a few more sprites, e.g. `PINVA0`, `PINVB0`, `PINVC0`, and `PINVD0` are the four sprites for the Invulnerability power-up. Monsters are the most complicated. They have several different sequences, for walking, firing, dying, etc, and they have different sprites for different angles. `PLAYA1`, `PLAYA2A8`, `PLAYA3A7`, `PLAYA4A6`, and `PLAYA5` are all for the first frame of the sequence used to display a walking (or running) player. 1 is the view from the front, 2 and 8 mean from front-right and front-left (the same sprite is used, and mirrored appropriately), 3 and 7 the side, 5 the back.

Each picture has three sections, basically. First, a four-integer header. Then a number of long-integer pointers. Then the picture pixel color data.

## [5-1]: Header
The header has four fields:

1. Width. The number of columns of picture data.
2. Height. The number of rows.
3. Left offset. The number of pixels to the left of the center; where the
      first column gets drawn.
4. Top offset. The number of pixels above the origin; where the top row is.

The width and height define a rectangular space or limits for drawing a picture within. To be "centered", 3. is usually about half of the total width. If the picture had 30 columns, and 3. was 10, then it would be off-center to the right, especially when the player is standing right in front of it, looking at it. If a picture has 30 rows, and 4. is 60, it will appear to "float" like a blue soul-sphere. If 4. equals the number of rows, it will appear to rest on the ground. If 4. is less than that for an object, the bottom part of the picture looks awkward.

With walls patches, 3. is always `(columns/2)-1`, and 4. is always `(rows)-5`. This is because the walls are drawn consistently within their own space (There are two integers in each `SIDEDEF` which can offset the beginning of a wall's texture).

Finally, if 3. and 4. are `NEGATIVE` integers, then they are the absolute coordinates from the top-left corner of the screen, to begin drawing the picture, assuming the `VIEW` is `FULL-SCREEN` (the full 320x200). This is only done with the picture of the doom player's current weapon - fist, chainsaw, bfg9000, etc. The game engine scales the picture down appropriately if the view is less than full-screen.

## [5-2]: Pointers
After the header, there are `N = (# of columns)` long integers (4 bytes each). These are pointers to the data for each COLUMN. The value of the pointer represents the offset in bytes from the first byte of the picture resource.

## [5-3]: Pixel Data
Each column is composed of some number of BYTES (NOT integers), arranged in "posts":

The first byte is the row to begin drawing this post at. 0 means whatever height the header (4) upwards-offset describes, larger numbers move correspondingly down.

The second byte is how many colored pixels (non-transparent) to draw, going downwards.

Then follow (# of pixels) + 2 bytes, which define what color each pixel is, using the game palette. The first and last bytes AREN'T drawn, and I don't know why they are there. Probably just leftovers from the creation process on the NExT machines. Only the middle (# of pixels in this post) are drawn, starting at the row specified in byte 1 of the post.

After the last byte of a post, either the column ends, or there is another post, which will start as stated above.

255 (hex FF) ends the column, so a column that starts this way is a null column, all "transparent". Goes to the next column.

Thus, transparent areas can be defined for either items or walls.

[4-2-1]: ./Chapter4.md
[6]: ./Chapter6.md
