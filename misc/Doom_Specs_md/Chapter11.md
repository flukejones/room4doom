# APPENDIX [A-1]: Backus-Naur Form definitions of WAD elements

  The descriptions below use a modified Backus-Naur Form (BNF) notation.
Each entry looks like

```
<keyword>       := description          ;type or comment (optional)
                   description cont'd.  ;type or comment (optional)
```

  Descriptions composed of more than one sequential keyword or element
are usually listed with one element per line. This is for clarity and also
allows each succesive element to be assigned different types without extra
lines.

```
<keyword>       := <whatever>           ;<type>
```

  is a shorthand for

```
<keyword>       := <whatever>
<whatever>      := <type>
```

  The description is one or more of the following predefined types,
and/or previously or subsequently defined keywords.

```
<byte>          is an unsigned 8-bit integer (0 to 255).
<char>          is a signed 8-bit integer (-128 to 127).
<ushort>        is an unsigned 16-bit integer in lo-hi format (0 to 65535)
<short>         is a signed 16-bit integer (-32768 to 32767).
<long>          is a signed 32-bit integer (-2147483648 to 2147483647).
<string8>       is an ASCII string of from 1 to 8 bytes. If its length is
                  less than 8 bytes, the remainder are zeros (hex 00).
```

  Any of these may be followed by a range: <byte:1..99> means a byte
restricted to the range 1 to 99 inclusive. A single number means that
value is literally included: <byte:192> inserts that 8-bit value.


  { } are used to enclose a group of elements.

  | is used to separate choices - exactly one of the choices applies.

  [ ] are used following an element or group of elements to indicate
an array. Usually a literal value or a keyword will be used to denote
how many members the array has. <rook> [666] means that the element
<rook> is repeated 666 times in sequence. {<Scylla> <Charybdis>} [zeus]
means that whatever the value of <zeus> is, there are that many pairs
of <Scylla> and <Charybdis>. [1..16] indicates the value may be from
1 to 16 inclusive, and [...] indicates an indefinite number.

  A literal string "ABCD" may appear, in which case those ASCII characters
are directly inserted.

```

<WAD file>      := "PWAD"|"IWAD" 
                   <numlumps> 
                   <infotableofs> 
                   <lumps> 
                   <directory>

<numlumps>      := <long>               ;number of lumps in WAD file
<infotableofs>  := <long>               ;file offset to directory start

<lumps>         := <lump> [numlumps]
<lump>          :=                      ;see different kinds below

<directory>     := {<lumpinfo> | <otherinfo>} [numlumps]
<lumpinfo>      := <filepos>            ;<long>
                   <size>               ;<long>
                   <name>               ;<string8>

<otherinfo>     := <marker> | <label>
<marker>        := <dummynumber>        ;<long> with any value
                   <long:0>
                   <"S_START" | etc>    ;<string8>

<label>         := {<"E"> <episode> <"M"> <mission>} | {<"MAP"> <level>}
<episode>       := "1"|"2"|"3"
<mission>       := "1"|"2"|"3"|"4"|"5"|"6"|"7"|"8"|"9"
<level>         := "01"|"02"|"03"|"04"|"05"|"06"|"07"|"08"|"09"|"10"
                   |"11"|"12"|"13"|"14"|"15"|"16"|"17"|"18"|"19"|"20"
                   |"21"|"22"|"23"|"24"|"25"|"26"|"27"|"28"|"29"|"30"
                   |"31"|"32"

------
different kinds of lumps:
------

<PLAYPAL>       := <palette> [14]
<palette>       := {<red> <green> <blue>} [256]
<red>           := <byte>
<green>         := <byte>
<blue>          := <byte>

------

<COLORMAP>      := <color_map> [34]
<color_map>     := <mapping> [256]
<mapping>       := <byte>

------

<ENDOOM>        := <character_cell> [1000]
<character_cell>:= <color_attributes>           ;<byte>
                   <character>                  ;<byte>

------

<demo>          := <header>
                   <gametic_data>
                   <byte:128>
<header>        := {<header_12> | <header_16>}  ;different versions
<header_12>     := <skill>
                   <episode>
                   <map>
                   <player> [4]
<header_16>     := <version>
                   <skill>
                   <episode>
                   <map> 
                   <mode>
                   <respawn>
                   <fast>
                   <nomonsters>
                   <viewpoint>
                   <player> [4]
<skill>         := <byte:0..4>
<episode>       := {<byte:1..3> | <byte:1>}     ;DOOM 1 or DOOM 2
<map>           := {<byte:1..9> | <byte:1..32>} ;DOOM 1 or DOOM 2
<player>        := <byte:0..1>          ;0 means not present, 1 means present
<version>       := <byte:104..106>      ;versions 1.4, 1.5, 1.6 (also 1.666)
<mode>          := <byte:0..2}          ;cooperative|deathmatch|altdeath
<respawn>       := <byte>               ;0 is off, non-zero is on
<fast>          := <byte>               ;0 is off, non-zero is on
<nomonsters>    := <byte>               ;0 is off, non-zero is on
<viewpoint>     := <byte:0..3>          ;shown from this player's view

<gametic_data>  := <gametic> [...]
<gametic>       := <player_move> [1..4] ;1-4 is # of players present in demo
<player_move>   := <forward>            ;<char>
                   <strafe>             ;<char>
                   <turn>               ;<char>
                   <use>                ;<byte>

------

<GENMIDI>       := "#OPL_II#"
                   <instr_data> [150]
                   <instr_name> [150]
<instr_data>    := <byte> [36]          ;format unknown to me
<instr_name>    := <byte> [32]          ;padded with 0s

------

<DMXGUS>        := pointless to describe here, see section [7-5]

------

<song>          := "MUS"
                   <byte:26>
                   <music_length>       ;<ushort>
                   <music_start>        ;<ushort>
                   <primary_channels>   ;<ushort>
                   <secondary_channels> ;<ushort>
                   <num_instr_patches>  ;<ushort>
                   <ushort:0>
                   <instr_patches>
                   <music data>
<instr_patches> := <instr_patch> [num_instr_patches]
<instr_patch>   := <ushort>             ;Drum patch #s 28 less than in DMXGUS

<music data>    := ???

------

<soundeffect>   := <ushort:3>
                   <ushort:11025>       ;sampling rate
                   <num_samples>        ;<ushort>
                   <ushort:0>
                   <samples>
<samples>       := <sample> [num_samples]       ;<byte>

------

<PC_sound>      := <ushort:0>
                   <num_PC_samples>     ;<ushort>
                   <PC_samples>
<PC_samples>    := <PC_sample> [num_PC_samples]
<PC_sample>     := <byte>               ;seem to range [0..96]

------

<TEXTURE1>      := <num_textures>       ;<long>
                   <tex_offsets>
                   <tex_entries>
<tex_offsets>   := <tex_offset> [num_textures]
<tex_offset>    := <long>
<tex_entries>   := <tex_entry> [num_textures]
<tex_entry>     := <tex_name>           ;<string8>
                   <short:0>
                   <short:0>
                   <tex_width>          ;<short>
                   <tex_height>         ;<short>
                   <short:0>
                   <short:0>
                   <num_patches>        ;<short>
                   <patches>            
<patches>       := <patch> [num_patches]
<patch>         := <x_offset>           ;all are <short>
                   <y_offset>
                   <pname_number>       ;lookup in <PNAMES> for picture
                   <short:1>            ;supposedly <stepdir>
                   <short:0>            ;supposedly <color_map>

------

<PNAMES>        := <num_pnames>         ;<long>
                   <pnames>
<pnames>        := <pname> [num_pnames]
<pname>         := <string8>]           ;match the <name> from the
                                        ;<lumpinfo> of a <picture>

------

<picture>       := <header>
                   <pointers>           ;offsets to <column> starts
                   <pixel_data>
<header>        := <width>              ;all are <short>
                   <height>
                   <left_offset>
                   <top_offset>
<pointers>      := <pointer> [width]    ;<long>
<pixel_data>    := <column> [width]
<column>        := <post> [...] 
                   <byte:255>           ;255 (0xff) ends the column
<post>          := <rowstart>           ;<byte>
                   <num_pixels>         ;<byte>
                   <unused>             ;<byte>
                   <pixels>
                   <unused>             ;<byte>
<pixels>        := <pixel> [num_pixels] ;<byte>

------

<flat>          := <colorbyte> [4096]   ;<byte>

------

<maplevel>      := <THINGS> 
                   <LINDEDEFS> 
                   <SIDEDEFS> 
                   <VERTEXES> 
                   <SEGS> 
                   <SSECTORS> 
                   <NODES> 
                   <SECTORS> 
                   <REJECT> 
                   <BLOCKMAP>

<THINGS>        := <thing> [...]
<thing>         := <x_position>         ;all are <short>
                   <y_position>
                   <angle>
                   <type>
                   <options>

<LINEDEFS>      := <linedef> [...]
<linedef>       := <vertex_start>       ;all are <short>
                   <vertex_end>
                   <flags>
                   <function>
                   <tag>
                   <sidedef_right>
                   <sidedef_left>       ;if <short: -1> there's no left side

<SIDEDEFS>      := <sidedef> [...]
<sidedef>       := <xoffset>            ;<short>
                   <yoffset>            ;<short>
                   <uppertexture>       ;<string8>
                   <lowertexture>       ;<string8>
                   <middletexture>      ;<string8>
                   <sector_ref>         ;<short>

<VERTEXES>      := <vertex> [...]
<vertex>        := <X_coord>            ;both are <short>
                   <Y_coord>    

<SEGS>          := <seg> [...]          ;<segs> stored by <subsector> order
<seg>           := <vertex_start>       ;all are <short>
                   <vertex_end>
                   <bams>
                   <line_num>
                   <segside>
                   <segoffset>

<SSECTORS>      := <subsector> [...]
<subsector>     := <numsegs>            ;both are <short>
                   <start_seg>

<NODES>         := <node> [...]
<node>          := <x>                  ;first four are <short>
                   <y>
                   <dx>
                   <dy>
                   <bbox> [2]
                   <child> [2]
<bbox>          := <boxtop>             ;all are <short>
                   <boxbottom>
                   <boxleft>
                   <boxright>
<child>         := <ushort>             ;if 0x8000 it's a subsector

<SECTORS>       := <sector> [...]
<sector>        := <floorheight>        ;<short>
                   <ceilingheight>      ;<short>
                   <floorpic>           ;<string8>
                   <ceilingpic>         ;<string8>
                   <lightlevel>         ;<short>
                   <special_sector>     ;<short>
                   <tag>                ;<short>

<REJECT>        := <bitarray>           ;see [4-10] for this one

<BLOCKMAP>      := <xorigin>            ;<short>
                   <yorigin>            ;<short>
                   <xblocks>            ;<short>
                   <yblocks>            ;<short>
                   <listoffsets>
                   <blocklists>
<listoffsets>   := <listoffset> [numofblocks]
<listoffset>    := <ushort>
<numofblocks>   := <short>              ;note it equals <xblocks> * <yblocks>
<blocklists>    := <blocklist> [numofblocks]
<blocklist>     := <short: 0>           ;for dynamic thinglist pointer
                   <lines_in_block>
                   <short: -1>
<lines_in_block>:= <linedef_num> [...]  ;the numbers of all the <linedef>s
                                        ;that are in the block
<linedef_num>   := <short>
```
