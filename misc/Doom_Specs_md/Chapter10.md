-------------------------------
CHAPTER [10]: The DOOM.EXE File
-------------------------------

  Via pwads, a great many characteristics of the DOOM environment can
be changed: maps, pictures, sounds, etc. But there are also a lot of
neat things that can be done by patching the DOOM.EXE file itself.
There is a large collection of data at the end of the EXE file, and by
patching some bytes, we can turn literal values into variables. For
example, the player has a 16-unit "radius" which prevents him from
entering very small passageways. The player's radius can be made 1 and
his "height" 1, so he can enter mouse-sized crawlspaces. There are a
lot more exciting examples, such as invisible monsters, cyber-demons
that look like players, super-fast shotguns, and a hundred others, but
I won't describe all of that here. See appendix [A-4] for some EXE
utilities and documents. Here I will simply give the data that has
been figured out to date.
  I freely mix hex and decimal numbers below. Hopefully you can tell from
the context. All of the stuff below applies to registered version 1.2,
and some of it applies to version 1.666 also. This chapter has not yet
been completely updated for 1.666, but it soon will be.

[10-1]: Version 1.2 DOOM.EXE Data Segment Overview
==================================================

  The data begins at 0x6f414 (455700) and continues to the end of the
file, 0x8db27 (580391). Here's an overview of the sections:

start length what

6f414  3d30  TEXT STRINGS
73412  1a34  various unknowns, probably to do with I/O, sound, mouse, etc.
74bf8 10000  looks like hard-coded math tables, for speed?
84bf8   148  misc.
84d40    82  gamma correction messages
84dc2   280  "are you sure you want to quit" messages
85042   3a2  MENUS (new game, load game, etc.)
853e4   140  ?
85524   36c  configuration options and defaults, like in DEFAULT.CFG
85890   174  ?
85a04    60  ?
85a64    54  ?
85ab8    c4  ?
85b7c    20  max ammo at start, and ammo per thing
85b9c    c0  ammo type and frame #s for the weapons
85c5c   188  ANIMATED WALLS and FLOORS
85de4   258  SWITCH-WALLS
8603c    c0  ?
860fc    d4  ?
861d0   500  5 colormaps for use with the gamma correction setting 0-4
866e4    fc  ?
867e0    40  pointers to chatmacros, "Green:", etc.
86820    88  pointers to level names, used on Automap
868a8    d8  splat mark coordinates for end-level screen
86980   5a8  wimap patch animations for end-level screen
86f28   224  SONG NAMES list of pointers
8714c   8b8  SOUND TABLE
87a04   1a4  SPRITE NAMES list of pointers
87ba8  3800  STATE TABLE
8b3a8    20  ?
8b3c8  2368  THING TABLE
8d730   3fd  ?

[10-2]: Version 1.666 DOOM.EXE Data Segment Overview
====================================================


[10-3]: Detail on some EXE Data Structures
==========================================

  More detail on some of the data follows. The "names" of each section
are the hexadecimal offsets to the start of that data, in the registered
versions 1.2 and 1.666 of DOOM.EXE. 1.2 offsets are to the left of the
asterisk, 1.666 to the right. "Integer" means a 4-byte <long> integer
in hi-lo format, unless otherwise noted (e.g. "2-byte short integer").

6f414 *** 82a14

  START OF DATA. Several times I'll refer to "pointers". All of these
pointers are integers. Add the values of these pointers to $6f414 or
$82a14 depending on the version, and you'll get the location of what's
being pointed to.
  Note: there's also at least one other kind of pointer in here, with
larger values, that point to a location in the code, NOT the data. I call
these "code-pointers" for now. I know it's a lame term.

6f414 *** a2228

  TEXT STRINGS. They all start on 4-byte boundaries, i.e. at xxxx0/4/8/c.
$00 ends the string. Then the next one starts at the next boundary, so a 4
byte string is followed by $00, then 3 bytes of random junk, then the next
string.

73140

  I think this is the last string, "TZ"

73144

  Misc. stuff I haven't investigated. Some of it has to do with sound card
stuff and mice and joysticks, because at 7384c is "DMXGUS.INI" and at 74ba8
are pointers which point to the strings "None", "PC_Speaker", "Adlib", etc.

74bf8

  64k of precisely ordered numbers, which leads me to believe they are
pre-calculated math tables, to speed up some floating point operations
used in the screen draw routine. Any other guesses?

84bfc

  3 pointers to the episode 1/2/3 end texts, "Once you beat...", "You've
done it...", and "The loathsome Spiderdemon is dead..."

84c24

  pointer to the string "doom.wad"

84c74

  pointer to the string "default.cfg"

84c78

  8 integers: 1, 25, 50, 24, 40, 640, 1280, 320

84c98

  2 code-pointers

84ccc

  29 integers, with values like 90 and 135 and 180. Angles?

84d40

  "Gamma correction OFF", 00s, "Gamma correction level 1", ... 4. Each
occupies $1a bytes.

84dc2

  8 text messages used to confirm quitting, each uses $50 bytes

85042

  MENUS. I know this controls to some extent which menu pictures are used
for which menu, but I haven't figured it all out yet.

853e4

  14 ints: 42, 22, 23, 24, 28, 29, 31, 40, zeros

8541c

  256 bytes, values from 00-ff, no two the same, "random" order.

85524

  The configuration options. Each is 5 integers: a pointer to a string,
like "mouse_sensitivity", a code-pointer, the default value for that
option, a 0 or 1 (1 for all the "key_" options), and a 0. It would be
pretty dense to do anything with this, I think.

85890

  About 117 integers, with a definite structure, but I can't figure it
out, and changing/experimenting seems to do nothing.

85a64

  21 sets of 4 bytes: 0, 0, 1, 0, 320, 168, "33", 0, 1, $(b2 26 26 2e),
$(ff 63 fd ff), a pointer that points to the $(b2...), 0, 1, "ema", 0, 0,
1, 0, 1, "xma". All these are unchanged from version 0.99 through 1.2,
except the pointer obviously.

85ab8

  Ints: 0, -1, -1, 0, 0, 0, 0, 4, 7, 10, 12, 14, 15, 15, 0, 0, 112, 96, 64,
176, then 16 that are members of this set {-65536, -47000, 0, 47000, 65536},
then 4, 5, 6, 7, 0, 1, 2, 3, 8, 3, 1, 5, 7

85b7c *** 95714

  AMMO AMOUNTS. 8 integers: 200, 50, 300, 50, 10, 4, 20, 1. The first four
are the maximum initial capacity for ammo, shells, cells, and rockets. The
backpack doubles these amounts. The second four are how many ammo in a
clip, shells, rockets/rocket, and cells/cell item. Boxes have 5x as much.

859bc *** 95734

  AMMO TABLE. 8 sets of 6 integers (9 sets in 1.666):

  version 1.2                             version 1.666

Punch     5  4  3  2  5  0              Punch     5  4  3  2  5  0
Pistol    0 12 11 10 13 17              Pistol    0 12 11 10 13 17
Shotgun   1 20 19 18 21 30              Shotgun   1 20 19 18 21 30
Chaingun  0 34 33 32 35 38              Chaingun  0 51 50 49 52 55
Laucher   3 42 41 40 43 46              Laucher   3 59 58 57 60 63
Plasma    2 59 58 57 60 62              Plasma    2 76 75 74 77 79
BFG       2 66 65 64 67 71              BFG       2 83 82 81 84 88
Chainsaw  5 53 52 50 54  0              Chainsaw  5 70 69 67 71  0
				  Super-Shotgun   1 34 33 32 35 47

  The first number of each set is the ammo type. Type 5 never runs out.
The next three numbers are 3 state #s (see the STATE TABLE below) for the
pictures displayed when moving while holding that weapon. You know, the
"bobbing weapon" effect? Fifth is the first state of the "shoot" sequence
for that weapon, and last is the first state of the "firing" sequence. The
"firing" pictures are the ones that are lit up, fire coming out, etc.

85c5c *** 9580c

  ANIMATED WALLS and FLOORS. Each is 26 bytes: an integer, a 8-byte string,
$00, a 8-byte string, $00, and a final integer.

0 NUKAGE3  NUKAGE1  8
0 FWATER4  FWATER1  8
0 SWATER4  SWATER1  8
0 LAVA4    LAVA1    8
0 BLOOD4   BLOOD1   8
		       <---- v1.666 has four more:  0 RROCK08  RROCK05  8
1 BLODGR4  BLODGR1  8                               0 SLIME04  SLIME01  8
1 SLADRIP4 SLADRIP1 8                               0 SLIME08  SLIME05  8
1 BLODRIP4 BLODRIP1 8                               0 SLIME12  SLIME09  8
1 FIREWALL FIREWALA 8
1 GSTFONT3 GSTFONT1 8
1 FIRELAVA FIRELAV3 8
1 FIREBLU2 FIREBLU1 8
1 ROCKRED3 ROCKRED1 8
		       <---- V1.666 has four more:  1 BFALL4   BFALL1   8
						    1 SFALL4   SFALL1   8
						    1 WFALL4   WFALL1   8
						    1 DBRAIN4  DBRAIN1  8

  Obviously the 0/1 means floor or wall. The first string is the name of
the animation cycle's LAST listed texture, the second string is the FIRST
listed texture. The cycle includes them and all entries between them in
whichever wad file is in effect (It doesn't have to be DOOM.WAD, a pwad
with new TEXTURE1 and 2 resources works quite nicely). The final 8
doesn't seem to mean much.

85dc8

  A -1 then a bunch of zeros, maybe space for another animation cycle?

85de4 *** 95a64

  SWITCH WALL NAMES. Each is 20 bytes: an 8-byte string, 00, another
string, 00, and a 2-byte short integer. There are 28 switch names here
in v1.2 and 39 switch names in v1.666. When a switch is pulled, the game
checks to see if the wall texture is on this list. If it is, it changes
the wall texture to the corresponding alternate texture. The <short>
is 1, 2, or 3. 1 means it's in all versions, 2 means only registered
DOOM 1 and DOOM 2, 3 means DOOM 2 only.

86028

  20 zeros, again, room for one more?

8603c ***

  48 integers: 3 0 2 1 3 0 2 0 3 1 2 0 0 0 0 0
	       2 0 2 1 0 0 0 0 3 1 3 0 0 0 0 0
	       2 0 3 1 2 1 3 1 2 1 3 0 0 0 0 0

860fc ***

  50 integers, all are either 50 or -50.

861d0 ***

  5 sets of 256 bytes, each is a COLORMAP, for the gamma correction
settings OFF, 1, 2, 3, 4.

866d0 ***

  5 integers: 1, 0, -1, 0, 0

866e4 ***

  13 sets of 5 - 10 bytes, each set terminated by a $FF

8675e ***

  $74 $20

86760 ***

  13 pointers to the stuff at 866e4. An integer '0' between each pointer.

867c8 ***

  6 integers: -1, -1, 0, -1, 0, 1

867e0 ***

  10 pointers to the 10 default chatmacros, then 4 pointers, to "Green:",
"Indigo:", "Brown:", "Red:"

86820 ***

  AUTOMAP LEVEL NAMES. 27 pointers to the level names used on the automap.

8689c ***

  The ascii letters "gibr" - the keys for sending messages in multiplayer.

868a8 ***

  SPLAT MARK COORDINATES. At what screen coordinates to place the WISPLAT
picture on the end-level screen, for th 27 levels. 54 integers, 27 pairs.
e1m1 x, e1m1 y, ..., e3m9 y.

86980, 86bb0, 86da8 ***

  END-LEVEL MAP ANIMATIONS. Each is 14 integers. The first one is (0, 11,
3, 224, 104, 0, 0, 0, 0, 0, 0, 0, 0, 0). The first number is 0 for all the
ones on maps 0 and 2 (episodes 1 and 3), and it's 2 for map 1. The 11 is
always 11 except the last one of map 2 is 8. The 3 means 3 pictures are
involved in the animation, e.g WIA00100, WIA00101, and WIA00102. 224 and 104
are the x and y coordinates. The sixth number is not 0 for map 1 - it's
from 1 to 8. This controls the way the Tower of Mystery "appears". All the
other numbers are always 0.

86ef8 ***

  Three integers, how many animations for WIMAP0, 1, 2 respectively.

86f04 ***

  Three pointers, to the starts of the animations for WIMAP0, 1, 2
respectively.

8714c ***

  SOUND TABLE. 61 and 1/2 sounds are listed here. Each is 9 integers: a
pointer to the string which is the sound's "name", then a 0 or 1, then
a value ranging from 32 to 128, then 0, -1, -1, 0, 0, 0. The names are
"pistol", "shotgn", ... "hoof", "metal", "chgun". Prefix DS or DP and you
get the entries in DOOM.WAD for the sound data. The "chgun" is the 1/2 -
there's no "DSCHGUN" in doom.wad, and the entry in this table is incomplete
anyway, lacking the all-important 0, -1, -1, 0, 0, 0 ending :-). There seem
to be a few glitches in the way the sounds were fit into the whole scheme,
this is just one of them.

879ec ***

  pointer to start of SOUND TABLE.

879f0 ***

  Integer = 150. 150 whats?

87a04 ***

  SPRITE NAME POINTERS. 105 pointers to the strings "TROO", "SHTG", ...,
"SMRT".

87ba8 *** 9834c

  STATE TABLE. 512 entries in v1.2, 967 entries in v1.666. Each entry
is 28 bytes in 7 integers:

(1)     sprite number 0-..., lookup in sprite name pointers list.
(2)     sprite frame, 0="A" in a sprite lump, 1="B", etc.
(3)     duration, how many gametics this state is displayed until
	  it looks for the next. -1 (0xffffffff) is forever.
(4)     a "code pointer" which indicates what action(s) accompany
	  the displaying of this state.
(5)     next state in sequence. 0 means no next state, sequence is done.
(6)     always 0, has no effect.
(7)     always 0, has no effect.


8b3a8 ***

  Two integers: 1, 0, then 6 code-pointers.

8b3c8 *** 9ed10

  THING TABLE. 103 entries in v1.2 which are each 88 bytes = 22 integers.
136 entries in v1.666, which are each 92 bytes = 23 integers.

(1)     Thing number, as used in maps. See [4-2-1]. Some of them are
	  equal to -1, e.g. the players' entry, and all projectiles.
(2)     "Spawn" state. State number (from STATE TABLE) for when this
	  thing first appears.
(3)     Health. Inanimates can't be killed, so it doesn't apply to them.
(4)     "Moving" state. First state # of monsters pursuing, etc.
(5)     "See player" sound. For monsters who become activated. Also for
	  projectiles' first sound. Note that sounds are 1-..., not 0-...
	  0 indicates no sound.
(6)     Reaction Time. Lower is faster.
(7)     "Attack" sound.
(8)     "Pain" state.
(9)     Painchance. The chance out of 256 that a monster will be disrupted
	  when it gets hurt. Otherwise, they keep attacking.
(10)    "Pain" sound.
(11)    "Close attack" state.
(12)    "Distance attack" state.
(13)    "Death" state, or "explode" for projectiles.
(14)    "Explosive death" state, only some monsters can be "mushed".
(15)    "Death" sound, or "explode" for projectiles.
(16)    Speed of movement. Projectiles' speed are * 65536.
(17)    Horizontal size (radius) * 65536
(18)    Height * 65536
(19)    Mass
(20)    Missile damage. Also, the Lost Soul has a 3 here, for it's attack.
(21)    "Act" sound, for wandering monsters.
(22)    Flags, see below
(23)    "Respawn" state, for monsters being ressurected. VERSION 1.666 ONLY

  Flags. 0 = condition is false. 1 = condition is true.

  bit   flagname        effect on thing

  0     Special         it is a gettable thing (ammo, health, etc.)
  1     Solid           creatures can't pass through (but projectiles can)
  2     Shootable       can be hurt (note barrels have this set)
  3     NoSector        totally invisible
  4     NoBlockmap
  5
  6     (InPain)        ?
  7
  8     SpawnCeiling    hung from ceiling
  9     NoGravity       floating monsters and not-on-ground things
  10    Dropoff         doesn't automatically hug floor if "jump" off ledge
  11    Pickup          can pick up gettable items
  12    (NoClip)        walks through walls
  13
  14    Float           floating monsters
  15    (Semi-NoClip)   climb tall steps
  16    Missile         projectiles
  17    (Disappearing   ?
	 Weapon)
  18    Shadow          semi-invisible like Spectres
  19    NoBlood         uses PUFF instead of BLUD when hurt (e.g. barrels)
  20    (SlideHelpless) ?
  21
  22    CountKill       Monster: counts toward KILLS ratio on inter-level
  23    CountItem       Artifact: counts toward ITEMS on inter-level screen
  24    (Running)       ?
  25    NotDMatch       this thing doesn't get spawned in deathmatch modes
  26    Color0          \ 00 = green stays green  01 = change to dark greys
  27    Color1          / 10 = change to browns   11 = change to dark reds
  28-                   unused

8d730 *** n/a

  Misc junk I can't figure out.

8db27 *** a7b99

  End of DOOM.EXE
