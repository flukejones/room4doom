# DeHackEd / BEX Specification

## Overview

DeHackEd is a patching system that modifies Doom's hardcoded game data at load time. Originally a DOS tool that binary-patched the executable, it evolved into a text-based lump format (`DEHACKED`) embedded in WAD files. It allows modders to change monster stats, animation frames, weapon behavior, ammo values, gameplay constants, and text strings — without writing new code.

BEX (Boom EXtended) adds named sections (`[STRINGS]`, `[PARS]`, `[CODEPTR]`) for more readable and maintainable patches. MBF and MBF21 further extend the format with new action functions and thing flags.

DeHackEd can only **modify** existing entries in Doom's data tables, not add new ones. However, extended ports (DSDA-doom, Woof) allow entries beyond vanilla limits (things 138+, frames 967+, sprites 138+) by pre-allocating extra slots.

Common uses in PWADs:
- Custom monsters (new stats, animations, attacks via frame/codepointer reassignment)
- Sprite remapping (reassign which sprite a thing type uses via frame modifications)
- Level names and intermission text (via `[STRINGS]`)
- Gameplay tweaks (monster health, speed, projectile damage)
- Par times (via `[PARS]`)

## Header

```
Patch File for DeHackEd v3.0
Doom version = 21
Patch format = 6
```

- `Doom version`: 12 = Doom 1.2, 16 = Doom 1.6, 19 = Doom 1.9, 21 = Doom 2 1.9
- `Patch format`: 6 is the standard text format
- Lines starting with `#` are comments

## Thing Block

Defines monster/item/decoration properties. Index is 1-based (Thing 1 = mobjinfo[0]).

```
Thing <number> (<name>)
<field> = <value>
```

| DeHackEd Field | mobjinfo_t Field | Type | Description |
|---|---|---|---|
| ID # | doomednum | i32 | Editor number for map placement |
| Initial frame | spawnstate | i32 | First frame when spawned |
| Hit points | spawnhealth | i32 | Health |
| First moving frame | seestate | i32 | Frame when alerted |
| Alert sound | seesound | i32 | Sound index when alerted |
| Reaction time | reactiontime | i32 | Tics before first attack |
| Attack sound | attacksound | i32 | Sound on attack |
| Injury frame | painstate | i32 | Frame when hurt |
| Pain chance | painchance | i32 | 0-255, chance of pain state |
| Pain sound | painsound | i32 | Sound when hurt |
| Close attack frame | meleestate | i32 | Melee attack frame |
| Far attack frame | missilestate | i32 | Ranged attack frame |
| Death frame | deathstate | i32 | Frame on death |
| Exploding frame | xdeathstate | i32 | Frame on gib death |
| Death sound | deathsound | i32 | Sound on death |
| Speed | speed | i32 | Movement speed (fixed-point for projectiles) |
| Width | radius | i32 | Collision radius (fixed-point, 16.16) |
| Height | height | i32 | Collision height (fixed-point, 16.16) |
| Mass | mass | i32 | Mass for thrust calculations |
| Missile damage | damage | i32 | Damage dealt by projectile |
| Action sound | activesound | i32 | Ambient/idle sound |
| Bits | flags | u32 | MF_ flags (see below) |
| Respawn frame | raisestate | i32 | Frame for archvile resurrect |

### MBF21 Extensions

| Field | Description |
|---|---|
| Retro Bits | Additional flags (e.g., CASTSHADOW) |
| Name1 | Display name |
| Plural1 | Plural display name |

### Thing Flags (Bits field)

Can be specified as decimal, hex, or as `FLAG1+FLAG2+FLAG3` names:

| Flag | Value | Name |
|---|---|---|
| 0x00000001 | SPECIAL | Pickup item |
| 0x00000002 | SOLID | Blocks movement |
| 0x00000004 | SHOOTABLE | Can be damaged |
| 0x00000008 | NOSECTOR | Invisible |
| 0x00000010 | NOBLOCKMAP | Not in blockmap |
| 0x00000020 | AMBUSH | Deaf/ambush |
| 0x00000040 | JUSTHIT | Try attack on next tic |
| 0x00000080 | JUSTATTACKED | Will take at least one step |
| 0x00000100 | SPAWNCEILING | Spawn on ceiling |
| 0x00000200 | NOGRAVITY | No gravity |
| 0x00000400 | DROPOFF | Can cross tall dropoffs |
| 0x00000800 | PICKUP | For MBF items |
| 0x00001000 | NOCLIP | No collision |
| 0x00002000 | SLIDE | Slides along walls |
| 0x00004000 | FLOAT | Flying monster |
| 0x00008000 | TELEPORT | Partial invisibility |
| 0x00010000 | MISSILE | Projectile |
| 0x00020000 | DROPPED | Dropped item |
| 0x00040000 | SHADOW | Partial invisibility |
| 0x00080000 | NOBLOOD | No blood on hit |
| 0x00100000 | CORPSE | Slides off ledges |
| 0x00200000 | INFLOAT | Float up/down to target |
| 0x00400000 | COUNTKILL | Counts toward kill % |
| 0x00800000 | COUNTITEM | Counts toward item % |
| 0x01000000 | SKULLFLY | Skull in flight |
| 0x02000000 | NOTDMATCH | Not spawned in deathmatch |
| 0x04000000 | TRANSLATION1 | Color translation bit 1 |
| 0x08000000 | TRANSLATION2 | Color translation bit 2 |
| 0x10000000 | TOUCHY | MBF: dies on contact |
| 0x20000000 | BOUNCES | MBF: bouncing projectile |
| 0x40000000 | FRIEND | MBF: friendly monster |
| 0x80000000 | TRANSLUCENT | Boom: rendered translucent |

## Frame Block

Defines animation states. Index is 0-based.

```
Frame <number>
<field> = <value>
```

| DeHackEd Field | state_t Field | Type | Description |
|---|---|---|---|
| Sprite number | sprite | i32 | Index into sprnames[] table |
| Sprite subnumber | frame | i32 | Frame within sprite + FF_FULLBRIGHT (0x8000) |
| Duration | tics | i32 | Tics to display (-1 = infinite) |
| Next frame | nextstate | i32 | State to transition to |
| Unknown 1 | misc1 | i32 | Action function parameter 1 |
| Unknown 2 | misc2 | i32 | Action function parameter 2 |

Sprite subnumber encodes: bits 0-14 = frame letter (0=A, 1=B, ...), bit 15 (0x8000) = full brightness.

## Weapon Block

Index: 0=Fist, 1=Pistol, 2=Shotgun, 3=Chaingun, 4=Rocket, 5=Plasma, 6=BFG, 7=Chainsaw, 8=SSG.

```
Weapon <number> (<name>)
<field> = <value>
```

| DeHackEd Field | weaponinfo_t Field | Type | Description |
|---|---|---|---|
| Ammo type | ammo | i32 | Ammo type index (0-3, 5=noammo) |
| Deselect frame | upstate | i32 | Frame when lowering |
| Select frame | downstate | i32 | Frame when raising |
| Bobbing frame | readystate | i32 | Idle/ready frame |
| Shooting frame | atkstate | i32 | Attack frame |
| Firing frame | flashstate | i32 | Muzzle flash frame |

## Ammo Block

Index: 0=Bullets, 1=Shells, 2=Cells, 3=Rockets.

```
Ammo <number> (<name>)
<field> = <value>
```

| DeHackEd Field | Type | Description |
|---|---|---|
| Per ammo | i32 | Ammo per clip pickup |
| Max ammo | i32 | Maximum carriable |

## Sound Block

```
Sound <number>
<field> = <value>
```

| DeHackEd Field | Type | Description |
|---|---|---|
| Offset | i32 | Unused (was DOS memory offset) |
| Zero/One | i32 | singularity flag |
| Value | i32 | priority |
| Zero 1-4 | i32 | Various unused fields |
| Neg. One 1-2 | i32 | Unused |

## Misc Block

Global gameplay constants.

```
Misc 0
<field> = <value>
```

| DeHackEd Field | Type | Description |
|---|---|---|
| Initial Health | i32 | Starting health (100) |
| Initial Bullets | i32 | Starting bullets (50) |
| Max Health | i32 | Max from health bonuses (200) |
| Max Armor | i32 | Max from armor bonuses (200) |
| Green Armor Class | i32 | Green armor class (1) |
| Blue Armor Class | i32 | Blue armor class (2) |
| Max Soulsphere | i32 | Max health from soulsphere (200) |
| Soulsphere Health | i32 | Health given by soulsphere (100) |
| Megasphere Health | i32 | Health set by megasphere (200) |
| God Mode Health | i32 | Health set by IDDQD (100) |
| IDFA Armor | i32 | Armor from IDFA (200) |
| IDFA Armor Class | i32 | Armor class from IDFA (2) |
| IDKFA Armor | i32 | Armor from IDKFA (200) |
| IDKFA Armor Class | i32 | Armor class from IDKFA (2) |
| BFG Cells/Shot | i32 | Cells consumed per BFG shot (40) |
| Monsters Infight | i32 | 0=normal, 1=infight all, 202=disabled |

## Text Block

Replaces raw string data in the executable. Used for sprite names, sound names, level names, etc.

```
Text <from_length> <to_length>
<old_string><new_string>
```

- `from_length`: byte length of old string
- `to_length`: byte length of new string
- Strings are read as raw bytes immediately after the header line (no newline separator between old and new)
- Used to rename sprite prefixes (4 chars), sound lump names, music lump names, and other hardcoded strings

Example — rename sprite prefix BOSS to BBRN:
```
Text 4 4
BOSSBBRN
```

## BEX Extensions

### [STRINGS] Section

Named string replacements (replaces Text block for readability).

```
[STRINGS]
<mnemonic> = <value>
```

Common mnemonics:
- `HUSTR_1` through `HUSTR_32` — level names
- `C1TEXT` through `C6TEXT` — intermission text
- `CC_ZOMBIE`, `CC_SHOTGUN`, etc. — cast call names
- `PD_BLUEK`, `PD_REDK`, `PD_YELLOWK` — key messages
- `GOTARMOR`, `GOTMEGA`, etc. — pickup messages

Values can span multiple lines using `\n` for newlines within the value.

### [PARS] Section

Override par times.

```
[PARS]
par <episode> <map> <seconds>    # Doom 1
par <map> <seconds>              # Doom 2
```

### [CODEPTR] Section

Assign action functions to frames by name instead of by copying from other frames.

```
[CODEPTR]
Frame <number> = <action_name>
```

Action names: `A_Chase`, `A_Look`, `A_Fire`, `A_Pain`, `A_Die`, `A_Explode`, `A_BFGSpray`, `A_Scream`, `A_XScream`, `A_Fall`, `A_BrainSpit`, `A_SpawnFly`, `A_BrainDie`, `A_Tracer`, `A_SkelWhoosh`, `A_SkelFist`, `A_SkelMissile`, `A_FatRaise`, `A_FatAttack1-3`, `A_BossDeath`, `A_KeenDie`, `A_BrainPain`, `A_BrainScream`, `A_BrainExplode`, `A_Metal`, `A_BabyMetal`, `A_Hoof`, `A_CPosAttack`, `A_CPosRefire`, `A_TroopAttack`, `A_SargAttack`, `A_HeadAttack`, `A_BruisAttack`, `A_SkullAttack`, `A_SpidRefire`, `A_BspiAttack`, `A_CyberAttack`, `A_PainAttack`, `A_PainDie`, `A_VileChase`, `A_VileStart`, `A_VileTarget`, `A_VileAttack`, `A_StartFire`, `A_FireCrackle`, `A_PlayerScream`, and MBF additions.

### [SPRITES] Section (Eternity/DSDA)

Add new sprite names beyond the vanilla 138.

```
[SPRITES]
<number> = <4-char name>
```

## Parsing Notes

- Field values can be decimal or hex (0x prefix)
- Thing/Frame/Weapon numbers are 1-indexed for Things, 0-indexed for Frames
- Frame 0 is `S_NULL` (the null/removed state)
- Sound 0 is `sfx_None`
- Parenthesized text after block number is a comment: `Thing 24 (Former Captain)` or `Thing 24 (_Custom Name)`
- Names starting with `_` indicate custom/modified entries
- Width and Height are 16.16 fixed-point: 1048576 = 16.0 units
- Speed for monsters is map units per tic; for projectiles it's 16.16 fixed-point

## Limits

- Things: 1-137 (Doom 2), extendable to 145+ via MBF/DSDA
- Frames: 0-966 (vanilla), extendable
- Sprites: 0-137 (vanilla), extendable via [SPRITES]
- Sounds: 0-108 (vanilla)
- Weapons: 0-8
- Ammo: 0-3

DeHackEd can only MODIFY existing entries, not add new ones. MBF21/DSDA-doom extensions allow entries beyond vanilla limits.
