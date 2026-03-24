# TODO

## Software 3D rendering

- [ ] Masked middle textures: do not repeat up or down

## Voxels

- [X] Don't replace sprites with voxels if the pwad contains replacement sprites

## BOOM compatibility

- [x] `SWITCHES` lump — extend switch list from binary lump
- [x] `ANIMATED` lump — extend animation list from binary lump
- [x] Translucent linedefs via ARGB alpha blend (linedef 260)
- [x] PassThru flag (bit 9) — multiple line activations per press
- [x] Generalized linedef types (0x2F80–0x7FFF)
- [x] Generalized sector types (bits 5-11)
- [x] Generalized locked door key check (with skull_is_card bit)
- [ ] Friction sectors (bit 9 of sector type)
- [ ] Push/pull sectors (bit 10 of sector type)
- [ ] Dehacked support

## UMAPINFO / MAPINFO

- [x] UMAPINFO parser in `wad/src/umapinfo/`
- [x] MAPINFO parser (ZDoom old syntax) in `wad/src/umapinfo/mapinfo.rs`
- [x] `skytexture`, `music`, `partime`, `next`/`nextsecret`
- [x] `endgame`/`endpic`/`endbunny`/`endcast`
- [x] `levelname`, `levelpic`, `exitpic`/`enterpic`
- [x] `intertext`/`intertextsecret`, `interbackdrop`, `intermusic`
- [x] `nointermission`, `episode`, `bossaction`
- [ ] `episode = clear` — total conversion episode menu override
- [ ] `label` / `author` — automap label and author display (needs automap)

## Map formats

- [x] Vanilla Doom nodes (OGDoom)
- [x] Extended nodes (XNOD) — uncompressed
- [x] Compressed extended nodes (ZNOD) — zlib decompression
- [ ] GL nodes (XGLN/XGL2) — skip BSP3D carving with pre-built convex subsector polygons
- [ ] Compressed GL nodes (ZGLN/ZGL2) — parser exists, not wired in level_data
- [ ] UDMF (general)
- [ ] UDMF (ZDoom extended)

## Menu

- [ ] Scrolling menu support for submenus exceeding screen height (key bindings, etc)

## Core features

- [ ] Automap
- [ ] Bunny scroller end screen
- [ ] Doom II cast call end screen
- [ ] Mlook options
- [ ] Display resolution selection
- [ ] Limit lost soul count from pain elementals
- [ ] Reset sector sound targets on player death
- [-] HUD (done except multiplayer chat)
- [-] Sound pitch shift

## Tools

- [x] `bsp-viewer` — BSP geometry inspector with egui GUI
- [x] `voxel-viewer` — standalone voxel render testing
- [x] `wad-tool` — WAD inspection and lump extraction CLI (`info`, `list`, `show`, `extract`)

## Test coverage

- [x] BOOM binary lump parsing (SWITCHES, ANIMATED from sunder/sigil2/eviternity)
- [x] UMAPINFO parsing (sigil, sigil2, SOS_Boom files)
- [x] MAPINFO parsing (sunder file)
- [x] XNOD header signature verification
- [x] Vanilla node signature detection
- [x] Cross-reference tests — E1M1 vertexes/linedefs/sidedefs/sectors/things vs omgifol JSON
- [ ] ZNOD test file — need a WAD with compressed zdoom nodes
- [ ] TRANMAP test file — need a BOOM WAD with translucency map lump
- [ ] UDMF test file — need a TEXTMAP lump for parser development

## Graphics

- [ ] OpenGL renderer
- [ ] Vulkan renderer
