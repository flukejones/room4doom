# CHAPTER [6]: Floor and Ceiling Textures
All the names for these textures are in the directory between the `F_START` and `F_END` entries. There is no look-up or meta-structure as with the walls. Each texture is 4096 raw bytes, making a square 64 by 64 pixels, which is pasted onto a floor or ceiling, with the same orientation as the automap would imply, i.e. the first byte is the color at the NW corner, etc. The blocks in the grid are 128 by 128, so four floor tiles will fit in each block.

The data in `F_SKY1` isn't even used since the game engine interprets that special ceiling as see-through to the SKY texture beyond. So the `F_SKY1` entry can have zero length.

As discussed in chapter [5], replacement and/or new-name floors don't work right from pwad files, only in the main `IWAD`.

You can change all the floors and ceilings you want by constructing a new `DOOM.WAD`, but you have to make sure no floor or ceiling uses an entry name which isn't in your `F_` section. And you have to include these four entries, although you can change their contents (pictures): `FLOOR4_8`, `SFLR6_1`, `MFLR8_4`, and `FLOOR7_2`. The first three are needed as backgrounds for the episode end texts. The last is what is displayed "outside" the display window if the display is not full-screen.

## [6-1]: Animated floors
See Chapter [8-4-1] for a discussion of how the animated walls and floors work. Unfortunately, the fact that the floors all need to be lumped together in one wad file means that its not possible to change the animations via a pwad file, unless it contains ALL the floors, which amounts to several hundred k. Plus you can't distribute the original data, so if you want to pass your modification around, it must either have all the floors all-new, or you must create some sort of program which will construct the wad from the original `DOOM.WAD` plus your additions.

[5]: ./Chapter5.md
[8-4-1]: ./Chapter8.md
