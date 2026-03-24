# CHAPTER [8]: Some Important Non-picture Resources
## [8-1]: PLAYPAL
There are 14 palettes here, each is 768 bytes = 256 rgb triples. That is, the first three bytes of a palette are the red, green, and blue portions of color 0. And so on.

Note that standard VGA boards whose palettes only encompass 262,144 colors only accept values of 0-63 for each channel (rgb), so the values would need to be divided by 4.

Palette 0 is the one that is used for almost everything.

Palettes 10-12 are used (briefly) when an item is picked up, the more items that are picked up in quick succession, the brighter it gets, palette 12 being the brightest.

Palette 13 is used while wearing a radiation suit.

Palettes 3, 2, then 0 again are used after getting berserk strength.

If the player is hurt, then the palette shifts up to X, then comes "down" one every half second or so, to palette 2, then palette 0 (normal) again. What X is depends on how badly the player got hurt: Over 100% damage (add health loss and armor loss), `X=8`. 93%, `X=7`. 81%, `X=6`. 55%, `X=5`. 35%, `X=4`. 16%, `X=2`.

## [8-2]: COLORMAP
This contains 34 sets of 256 bytes, which "map" the colors "down" in brightness. Brightness varies from sector to sector. At very low brightness, almost all the colors are mapped to black, the darkest gray, etc. At the highest brightness levels, most colors are mapped to their own values, i.e. they don't change.

In each set of 256 bytes, byte 0 will have the number of the palette color to which original color 0 gets mapped.

The colormaps are numbered 0-33. Colormaps 0-31 are for the different brightness levels, 0 being the brightest (light level 248-255), 31 being the darkest (light level 0-7).

Colormap 32 is used for every pixel in the display window (but not the status bar), regardless of sector brightness, when the player is under the effect of the "Invulnerability" power-up. This map is all whites/greys.

Colormap 33 is all black for some reason.

## [8-3]: DEMO[1-3]
These are the demos that will be shown if you start doom, and do nothing else. Demos can be created using the devparm parameter:

`DOOM -devparm -record DEMONAME`

The extension `.LMP` is automatically added to the `DEMONAME`. Other parameters may be used simultaneously, such as `-skill [1-5]`, `-warp [1-3] [1-9]`, `-file [pwad_filename]`, etc. The demos in the WAD are in exactly the same format as these LMP files, so a LMP file may be simply pasted or assembled into a WAD, and if its length and pointer directory entries are correct, it will work.

This is assuming the same version of the game, however. For some illogical reason, demos made with 1.1 doom don't work in 1.2 doom, and vice versa. If I had a pressing need to convert an old demo, I might try to figure out why, but I don't.

The game only accesses `DEMO1`, `DEMO2`, and `DEMO3`, so having more than that in a pwad file is pointless.

## [8-4]: TEXTURE1 and TEXTURE2
These resources contains a list of the wall names used in the various `SIDEDEFS` sections of the level data. Each wall name actually references a meta-structure, defined in this list. `TEXTURE2` has all the walls that are only in the registered version.

First is a table of pointers to the start of the entries. There is a long integer (say, `N`) which is the number of entries in the `TEXTURE` resource. Then follow N long integers which are the offsets in bytes from the beginning of the `TEXTURE` resource to the start of that texture's definition entry.

Then follow N texture entries, which each consist of a 8-byte name field and then a variable number of 2-byte integer fields:

1. The name of the texture, used in `SIDEDEFS`, e.g. "`STARTAN3`".
2. always 0.
3. always 0.
4. total width of texture
5. total height of texture

  The fourth and fifth fields define a "space" (usually 128 by 128 or 64 by 72 or etc...) in which individual wall patches are placed to form the overall picture. This is done because there are some wall patches that are used in several different walls, like computer screens, etc. Note that to tile properly in the vertical direction on a very tall wall, a texture has to have height 128, the maximum. The maximum width is 256. The sum of the sizes of all the wall patches used in a single texture must be <= 64k.

6. always 0.
7. always 0.
8. Number of 5-field patch descriptors that follow. This is why each texture
    entry has variable length. Many entries have just 1 patch, one has 64!

```
        1. x offset from top-left corner of texture space defined in field
           4/5 to start placement of this patch
        2. y offset
        3. number, from 0 to whatever, of the entry in the PNAMES resource,
           which contains the name from the directory, of the wall patch to
           use...
        4. always 1, is for something called "stepdir"...
        5. always 0, is for "colormap"...
```

The texture's entry ends after the last of its patch descriptors.

Note that patches can have transparent parts, since they are in the same picture format as everything else. Thus there can be (and are) transparent wall textures. These should only be used on a border between two sectors, to avoid the "displaying nothing" problems.

Here is how one can add walls, while still retaining any of the original ones it came with: in a pwad, have replacement entries for `PNAMES` and `TEXTURE2`. These will be the same as the originals, but with more entries, for the wall patches and assembled textures that you're adding. Then have entries for every new name in `PNAMES`, as well as old names which you want to associate to new pictures. You don't need to use the `P_START` and `P_END` entries.

### [8-4-1]: Animated walls
It is possible to change the walls and floors that are animated, like the green blocks with a sewer-like grate that's spewing green slime (`SLADRIPx`). The game engine sets up as many as 8 animation cycles for walls based on the entries in the `TEXTURE` resources, and up to 5 based on what's between `F_START` and `F_END`. The entries in FirstTexture and LastTexture, below, and all the entries between them (in the order that they occur in a `TEXTURE` list), are linked. If one of them is called by a sidedef, that sidedef will change texture to the next in the cycle about 5 times a second , going back to First after Last. Note that the entries between First and Last need not be the same in number as in the original, nor do they have to follow the same naming pattern, though that would probably be wise. E.g. one could set up `ROCKRED1`, `ROCKREDA`, `ROCKREDB`, `ROCKREDC`, `ROCKREDD`, `ROCKREDE`, `ROCKRED3` for a 7-frame animated wall!

If First and Last aren't in either `TEXTURE`, no problem. Then that cycle isn't used. But if First is, and Last either isn't or is listed `BEFORE` First, then an error occurs.

| FirstTexture | LastTexture | Normal # of frames |
|:------------:|:-----------:|:-------------------|
|   BLODGR1    |   BLODGR4   | 4                  |
|   BLODRIP1   |  BLODRIP4   | 4                  |
|   FIREBLU1   |  FIREBLU2   | 2                  |
|   FIRELAV3   |  FIRELAVA   | 2                  |
|   FIREMAG1   |  FIREMAG3   | 3                  |
|   FIREWALA   |  FIREWALL   | 3                  |
|   GSTFONT1   |  GSTFONT3   | 3                  |
|   ROCKRED1   |  ROCKRED3   | 3                  |
|   SLADRIP1   |  SLADRIP3   | 3                  |

(floor/ceiling animations) -

| FirstTexture | LastTexture | Normal # of frames |
|:------------:|:-----------:|:-------------------|
|   NUKAGE1    |   NUKAGE3   | 3                  |
|   FWATER1    |   FWATER3   | 3                  |
|   SWATER1    |   SWATER4   | 4                  |
|    LAVA1     |    LAVA4    | 4                  |
|    BLOOD1    |   BLOOD3    | 3                  |

Note that the `SWATER` entries aren't in the regular `DOOM.WAD.`
## [8-5]: PNAMES
This is a lookup table for the numbers in `TEXTURE[1 or 2]` to reference to an actual entry in the directory which is a wall patch (in the picture format described in chapter [5]).

The first two bytes of the `PNAMES` resource is an integer `P` which is how many entries there are in the list.

Then come P 8-byte names, each of which duplicates an entry in the directory. If a patch name can't be found in the directory (including the external pwad's directories), an error will occur. This naming of resources is apparently not case-sensitive, lowercase letters will match uppercase.

The middle integer of each 5-integer "set" of a `TEXTURE1` entry is something from 0 to whatever. Number 0 means the first entry in this `PNAMES` list, 1 is the second, etc...

Thanks for reading the "Official" DOOM Specs!

```
----------- Hank Leukart ------------ |   "Official" DOOM FAQ v5.5 Writer
--- (ap641@cleveland.freenet.edu) --- |   FAQ by E-mail or "ftp.uwp.edu"
------------------------------------- |  Support your shareware companies!
------------------------------------- |      REGISTER your shareware!
```

[5]: ./Chapter5.md
