------------------------------------------------------------------------------

```
                           T H E   U N O F F I C I A L
=================     ===============     ===============   ================
\\ . . . . . . .\\   //. . . . . . .\\   //. . . . . . .\\  \\. . .\\// . .//
||. . ._____. . .|| ||. . ._____. . .|| ||. . ._____. . .|| || . . .\/ . ..||
|| . .||   ||. . || || . .||   ||. . || || . .||   ||. . || ||. . . . . . .||
||. . ||   || . .|| ||. . ||   || . .|| ||. . ||   || . .|| || . | . . . ..||
|| . .||   ||. _-|| ||-_ .||   ||. . || || . .||   ||. _-|| ||-_.|\ . . . .||
||. . ||   ||-'  || ||  `-||   || . .|| ||. . ||   ||-'  || ||  `|\_ . .|..||
|| . _||   ||    || ||    ||   ||_ . || || . _||   ||    || ||   |\ `-_/| .||
||_-' ||  .|/    || ||    \|.  || `-_|| ||_-' ||  .|/    || ||   | \  / -_.||
||    ||_-'      || ||      `-_||    || ||    ||_-'      || ||   | \  / | '||
||    `'         || ||         `'    || ||    `'         || ||   | \  / |  ||
||            .===' `===.         .==='.`===.         .===' /==. |  \/  |  ||
||         .=='   \_|-_ `===. .==='   _|_   `===. .===' _-|/   `==  \/  |  ||
||      .=='    _-'    `-_  `='    _-'   `-_    `='  _-'   `-_  /|  \/  |  ||
||   .=='    _-'          `-__\._-'         `-_./__-'         `' |. /|  |  ||
||.=='    _-'                                                     `' | /==.||
=='    _-'         S         P         E         C         S          \/  `==
\   _-'                                                                `-_  /
 `''                                                                      ``'
                       Release v1.666 - December 15th, 1994
                   Written by: Matthew S Fell (msfell@aol.com)

          "The poets talk about love, ...but what I talk about is DOOM,
                  because in the end, DOOM is all that counts."
            - Alex Machine/George Stark/Stephen King, _The Dark Half_
```

------------------------------------------------------------------------------



DISCLAIMER
----------

These specs are to aid in informing the public about the games
DOOM and DOOM 2, by id Software.  In no way should this promote your
killing yourself, killing others, or killing in any other fashion.
Additionally, the author does not claim ANY responsibility
regarding ANY illegal activity concerning this file, or indirectly related
to this file.  The information contained in this file only reflects
id Software indirectly, and questioning id Software regarding any
information in this file is not recommended.

COPYRIGHT NOTICE
----------------

This article is Copyright 1994 by Matt Fell.  All rights reserved.
You are granted the following rights:

1. To make copies of this work in original form, so long as
  1. the copies are exact and complete;
  2. the copies include the copyright notice and these paragraphs
     in their entirety;
  3. the copies give obvious credit to the author, Matt Fell;
  4. the copies are in electronic form.
2. To distribute this work, or copies made under the provisions
   above, so long as
  1. this is the original work and not a derivative form;
  2. you do not charge a fee for copying or for distribution;
  3. you ensure that the distributed form includes the copyright
     notice, this paragraph, the disclaimer of warranty in
     their entirety and credit to the author;
  4. the distributed form is not in an electronic magazine or
     within computer software (prior explicit permission may be
     obtained from the author);
  5. the distributed form is the NEWEST version of the article to
     the best of the knowledge of the distributor;
  6. the distributed form is electronic.

You may not distribute this work by any non-electronic media,
including but not limited to books, newsletters, magazines, manuals,
catalogs, and speech.  You may not distribute this work in electronic
magazines or within computer software without prior written explicit
permission.  These rights are temporary and revocable upon written, oral,
or other notice by the author. This copyright notice shall be governed
by the laws of the state of Ohio.

If you would like additional rights beyond those granted above,
write to the author at "msfell@aol.com" on the Internet.

CONTENTS
--------

1. [Introduction][1]
  1. [id Software's Copyright][1-1]
  2. [What's New][1-2]
2. [The Basics][2]
  1. [Pwads][2-1]
  2. [DOOM version information][2-2]
  3. [Terminology conventions][2-3]
3. [List of DOOM.WAD Directory Entries][3]
4. [The Levels][4]
  1. [ExMy or MAPxy][4-1]
  2. [THINGS][4-2]
    1. [Thing Types][4-2-1]
    2. [Thing Sizes][4-2-2]
    3. [Thing Options][4-2-3]
  3. [LINEDEFS][4-3]
    1. [Linedef Flags][4-3-1]
    2. [Linedef Types][4-3-2]
  4. [SIDEDEFS][4-4]
  5. [VERTEXES][4-5]
  6. [SEGS][4-6]
  7. [SSECTORS][4-7]
  8. [NODES][4-8]
  9. [SECTORS][4-9]
    1. [Special Sector Types][4-9-1]
  10. [REJECT][4-10]
  11. [BLOCKMAP][4-11]
5. [Graphics][5]
  1. [Picture Format][5-1]
6. [Flats (Floor and Ceiling Textures)][6]
  1. [Animated Floors][6-1], see [8-4-1][8-4-1]
7. [Sounds and Music][7]
  1. [PC Speaker Sound Effects][7-1]
  2. [Soundcard Sound Effects][7-2]
  3. [Music][7-3]
  4. [GENMIDI][7-4]
  5. [DMXGUS][7-5]
8. [Miscellaneous Lumps][8]
  1. [PLAYPAL][8-1]
  2. [COLORMAP][8-2]
  3. [ENDOOM][8-3]
  4. [TEXTURE1 and TEXTURE2][8-4]
    1. [Animated Walls][8-4-1]
    2. [The SKY Textures][8-4-2]
  5. [PNAMES][8-5]
  6. [DEMOs][8-6]
    1. [Level changes from 1.2 to 1.666 DOOM.WAD][8-6-1]
9. [Savegame Files][9]

10. [The DOOM.EXE File][10]
  1. [Version 1.2 DOOM.EXE Data Segment Overview][10-1]
  2. [Version 1.666 DOOM.EXE Data Segment Overview][10-2]
  3. [Detail on some EXE Data Structures][10-3]

APPENDICES

1. [Backus-Naur Form definitions of wad elements][A-1]
2. [Engine limits][A-2]
3. [DOOM.WAD changes and errors][A-3]
4. [A BLOCKMAP algorithm][A-4]
5. [Other helpful documents][A-5]
6. [Acknowledgments][A-6]

[1]: ./Chapter01.md
[1-1]: ./Chapter01.md
[1-2]: ./Chapter01.md


[2]: ./Chapter02.md
[2-1]: ./Chapter02.md
[2-2]: ./Chapter02.md
[2-3]: ./Chapter02.md

[3]: ./Chapter03.md

[4]: ./Chapter04.md
[4-1]: ./Chapter04.md
[4-2]: ./Chapter04.md
[4-2-1]: ./Chapter04.md
[4-2-2]: ./Chapter04.md
[4-2-3]: ./Chapter04.md
[4-3]: ./Chapter04.md
[4-3-1]: ./Chapter04.md
[4-3-2]: ./Chapter04.md
[4-4]: ./Chapter04.md
[4-5]: ./Chapter04.md
[4-6]: ./Chapter04.md
[4-7]: ./Chapter04.md
[4-8]: ./Chapter04.md
[4-9]: ./Chapter04.md
[4-9-1]: ./Chapter04.md
[4-10]: ./Chapter04.md
[4-11]: ./Chapter04.md

[5]: ./Chapter05.md
[5-1]: ./Chapter05.md

[6]: ./Chapter06.md
[6-1]: ./Chapter06.md

[7]: ./Chapter07.md
[7-1]: ./Chapter07.md
[7-2]: ./Chapter07.md
[7-3]: ./Chapter07.md
[7-4]: ./Chapter07.md
[7-5]: ./Chapter07.md

[8]: ./Chapter08.md
[8-1]: ./Chapter08.md
[8-2]: ./Chapter08.md
[8-3]: ./Chapter08.md
[8-4]: ./Chapter08.md
[8-4-1]: ./Chapter08.md
[8-4-2]: ./Chapter08.md
[8-5]: ./Chapter08.md
[8-6]: ./Chapter08.md
[8-6-1]: ./Chapter08.md

[9]: ./Chapter09.md

[10]: ./Chapter10.md
[10-1]: ./Chapter10.md
[10-2]: ./Chapter10.md
[10-3]: ./Chapter10.md


[A-1]: ./Chapter11.md
[A-2]: ./Chapter12.md
[A-3]: ./Chapter13.md
[A-4]: ./Chapter14.md
[A-5]: ./Chapter15.md
[A-6]: ./Chapter16.md
