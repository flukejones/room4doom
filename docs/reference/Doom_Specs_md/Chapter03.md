# CHAPTER [3]: Directory Overview
This is a list of most of the directory entries. It would take 2000 lines to list every single entry, and that would be silly. All the ST entries are for status bar pictures, so why list every one? And the naming convention for the 700 sprites is easy (see chapter [5][5]), so there's no need to list them all individually.

- `PLAYPAL` contains fourteen 256 color palettes, used while playing Doom.
- `COLORMAP` maps colors in the palette down to darker ones, for areas of less than maximum brightness (quite a few of these places, huh?).
- `ENDOOM` is the text message displayed when you exit to DOS.
- `DEMOx` x=1-3, are the demos which will play if you just sit and watch.
- `E1M1` etc, to E3M9, along with its 10 subsequent entries, defines the map data for a single level or mission.
- `TEXTURE1` is a list of wall type names used in the SIDEDEF portion of each level , and their composition data, i.e. what wall patches make up each texture.
- `TEXTURE2` contains the walls that are only in the registered version.
- `PNAMES` is the list of wall patches, which are referenced by number in the TEXTURE1/2 resources.
- `GENMIDI` has the names of every General Midi standard instrument in order from 0-127. Anyone know more...?
- `DMXGUS` obviously has to do with Gravis Ultra Sound. It's a text file, easy to read. Just extract it (WadTool works nicely).
- `D_ExMy` is the music for episode x level y.
- `D_INTER` is the music played on the summary screen between levels.
- `D_INTRO` is the 4 second music played when the game starts.
- `D_INTROA` is also introductory music.
- `D_VICTOR` is the music played on the victory text-screen after an episode.
- `D_BUNNY` is music for while a certain rabbit has his story told...
- `DP_xxxxx` DP and DS come in pairs and are the sound effects. DP_ are the PC
- `DS_xxxxx` speaker sounds, DS_ are the sound card sounds.

All the remaining entries in the directory, except the floor textures at the end, and the "separators" like S_START, refer to resources which are pictures, in the doom/wad picture format described in chapter [5][5]. The floor textures are also pictures, but in a raw format described in chapter [6][6].

The next seven are full screen (320 by 200 pixel) pictures:

- `HELP1` The ad-screen that says Register!, with some screen shots.
- `HELP2` The actual help, all the controls explained.
- `TITLEPIC` Maybe this is the title screen? Gee, I dunno...
- `CREDIT` The credits, the people at id Software who created this great game.
- `VICTORY2` The screen shown after a victorious end to episode 2.
- `PFUB1` A nice little rabbit minding his own peas and queues...
- `PFUB2` ...maybe a hint of what's waiting in Doom Commercial version.
- `ENDx` x=0-6, "THE END" text, with (x) bullet holes.
- `AMMNUMx` x=0-9, are the gray digits used in the status bar for ammo count.
- `STxxxxxx` are small pictures and text used on the status bar.
- `M_xxxxxx` are text messages (yes, in picture format) used in the menus.
- `BRDR_xxx` are tiny two pixel wide pictures use to frame the viewing window when it is not full screen.
- `WIxxxxxx` are pictures and messages used on the summary screen after the completion of a level.
- `WIMAPx` x=0-2, are the summary-screen maps used by each episode.
- `S_START` has 0 length and is right before the item/monster "sprite" section. See chapter [5] for the naming convention used here.
- `S_END` is immediately after the last sprite.
- `P_START` marks the beginning of the wall patches.
- `P1_START` before the first of the shareware wall patches.
- `P1_END` after the last of the shareware wall patches.
- `P2_START` before the first of the registered wall patches.
- `P2_END` before the first of the registered wall patches.
- `P_END` marks the end of the wall patches.
- `F_START` marks the beginning of the floors.
- `F1_START` before the first shareware floor texture.
- `F1_END` after the last shareware floor texture.
- `F2_START` before the first registered floor texture.
- `F2_END` after the last registered floor texture.
- `F_END` marks the end of the floors.

And that's the end of the directory.

It is possible to include other entries and resources in a wad file, e.g. an entry called CLOWNS could point to a resource that includes the level creator's name, date of completion, or a million other things. None of these non-standard entries will be used by DOOM, nor will they cause it problems. Some of the map editors currently out add extra entries. There is a debate going on right now as to the merits of these extras. Since they are all non-standard, and potentially confusing, for now I'm in favor of not using any extra entries, and instead passing along a text file with a pwad. However, I can see some possible advantages, and I might change my mind...

[5]: ./Chapter5.md
[6]: ./Chapter6.md
