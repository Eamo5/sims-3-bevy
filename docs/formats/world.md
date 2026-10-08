# The Sims 3 `.world` file — resource formats

Verified against `Sunset Valley.world` (base game, 80,283,799 bytes, 6766 entries, DBPF 2.0).
Python prototype parsers used for verification lived in a scratch dir; the logic is transcribed
below as pseudo-code.

Confidence tags used throughout:

- **[VERIFIED]** — parsed byte-exact across *all* instances in Sunset Valley (parser consumes the
  resource exactly to its end) or cross-checked against an independent value.
- **[WIKI]** — from the SimsWiki/MTS per-type research pages (community reverse engineering,
  2009-2011, by Karybdis, Tiger, granthes and others). Not re-verified unless also tagged VERIFIED.
- **[GUESS]** — our inference from the data; treat as a hypothesis.

Conventions: little-endian. `str16` = `u32 charCount` + `charCount` UTF-16LE code units (no NUL).
`TGI` = `u32 type, u32 group, u64 instance` (16 bytes). **FNV64** = 64-bit FNV-1
(`h = 0xCBF29CE484222325; for byte: h = h*0x100000001B3; h ^= byte`) over the **lower-cased** ASCII
string; **FNV32** likewise (`0x811C9DC5`, prime `0x01000193`).

## Contents

0. [Key findings / quick map](#0-quick-map)
1. [Coordinate system](#1-coordinate-system)
2. [Resource inventory](#2-resource-inventory)
3. [Terrain: heightmap, texturing, tiles, sea level](#3-terrain)
4. [Layers (DETL/WDET) — how the world is organised](#4-layers)
5. [REFS — the per-resource TGI tables (`residx`)](#5-refs-0x05ed1226)
6. [OBJN — placed objects (world + lots)](#6-objn-0x4d1a5589--placed-objects)
7. [OBJS — serialized script state](#7-objs-0x06b981ed--serialized-script-state)
8. [Lots: LDES and the per-lot resources](#8-lots)
9. [Lot buildings: walls, floors, roofs, stairs](#9-lot-buildings)
10. [Lot imposters / pre-baked LOD](#10-pre-baked-lod-lot-imposters-and-terrain-tiles)
11. [Roads, routing, boundary](#11-roads-routing-world-boundary)
12. [Small world-description resources](#12-small-singletons)
13. [Unknowns / next steps](#13-unknowns--next-steps)
14. [Sources](#14-sources)

---

## 0. Quick map

| You want | Read |
|---|---|
| Terrain height | `0x2AD195F2` (2049² u16, ×1000/65536 m) |
| Terrain texture names | `0x9063660D` layer list → DDS `0x00B2D882` with instance FNV64(name) in game packages |
| Terrain blend weights | `0x3D8632D0` (block-compressed 2048² masks, one per layer) |
| Sea level | `0xB074ACE6` header f32 (28.07 m in SV) [GUESS, strong] |
| Lot position/size/rotation/type/name | `0xD063545B` LDES (one per lot) |
| Every placed object (trees, lamps, furniture, doors, windows...) with OBJD + transform | `0x4D1A5589` OBJN, one per lot **and** one per world layer; `residx` → `0x05ED1226` REFS group `0x00F0B54D` |
| World object grouping (tree layer, street objects, neighbourhoods) | `0x03D86EA4` DETL |
| Walls | `0x312E7545` wall graphs (5 channels) + `0xB1422971`/`0xF12E5E12` side materials |
| Floors | `0xB125533A` per-level grids (9 channels) |
| Roofs / stairs | `0x11E32896` / `0x04A09283` |
| Wall/floor/roof catalog refs | REFS group `0x00000000` (same lot instance) |
| Pre-baked distant building | per-lot `MODL 0x01661233` groups `0x00B0C507` / `0x001DA7E9` (+ DDS) |
| Pre-baked terrain chunks | `0xAE39399F` (2 LODs × 64 tiles), `0x033B2B66` (64), DDS group 1 (564) |

---

## 1. Coordinate system  [VERIFIED]

- World is 2048 m × 2048 m (Sunset Valley; all map headers say 0x800).
- Right-handed, **Y up**. Horizontal axes X and Z. Units: metres.
- Heightmap sample `[row][col]` is at world `(x = col, z = row)`, 1 m spacing, 2049 × 2049 samples.
  Verified: LDES lot origin `y` equals the terrain height at its `(x, z)` (7/7 lots, ±0.2 m), and
  OBJN street objects sit exactly on the terrain (174/174, |Δy| < 0.5 m).
- **All OBJN object positions are in world space**, including objects on lots (not lot-local).
- Matrices in lot resources are row-major, row-vector convention (`p' = p · M`, translation in row 3).
- Quaternions are stored `(x, y, z, w)`.
- World tiles: 8 × 8 tiles of 256 m (used by WTXT, the terrain chunk resources and per-tile DDS).

---

## 2. Resource inventory

Sunset Valley counts. "Lot id" instances look like `0x6C11001B1FD9D1B0`; every per-lot resource has
instance = lot id. Groups matter: several types use the **group as a channel id**.

| Type | s3pi tag | n | Meaning | Conf. |
|---|---|---|---|---|
| 0x2AD195F2 | – | 1 | Terrain heightmap | VERIFIED |
| 0x9063660D | WTXT | 1 | Terrain layer list + per-tile layer usage | VERIFIED (header/list) |
| 0x3D8632D0 | – | 1 | Terrain paint blend masks | VERIFIED + WIKI |
| 0x05DA8AF6 | WBND | 1 | World boundary mask (same codec) | WIKI + header VERIFIED |
| 0xB074ACE6 | – | 1 | Ocean/water: header holds sea level f32 | GUESS (strong) |
| 0xAE39399F | – | 128 | Pre-baked terrain mesh per 256 m tile, 2 LODs | GUESS (strong) |
| 0x033B2B66 | – | 64 | Per-tile terrain chunk wrapper/metadata | GUESS |
| 0x342779A7 | – | 1 | Unknown big record table (also in saves) | unknown |
| 0x05CD4BB3 | – | 1 | World routing graph | WIKI |
| 0x9063660E | – | 1 | Road & walkway graph (bezier curves) | WIKI |
| 0x03D86EA4 | DETL | 42 | World layers ("treeLayer", "StreetObjects", neighbourhoods) | VERIFIED |
| 0x04A4D951 | WDET | 34 | World detail: (guid, name) pairs | WIKI |
| 0x90620000 | – | 1 | List of 32 layer ids | VERIFIED |
| 0x4D1A5589 | OBJN | 134 | **Placed objects**: 92 lots + 42 layers | VERIFIED (133/134) |
| 0x05ED1226 | REFS | 227 | TGI tables: g=0x00F0B54D (134, for OBJN), g=0 (92, lot building), g=0x00BBB9D2 (1) | VERIFIED |
| 0x06B981ED | OBJS | 1 | Serialized C# script state (households, lots, sim descs) | WIKI + header VERIFIED |
| 0xD063545B | LDES | 92 | **Lot description** | VERIFIED |
| 0x046A7235 | – | 92 | Lot location + routing (portals/footprints) | WIKI + header VERIFIED |
| 0x312E7545 | – | 460 = 92×5 | Wall graphs (groups 0x002E7B1A/1C/1D/1E/1F) | VERIFIED (layout) |
| 0xB1422971 | – | 460 = 92×5 | Wall-side material assignments | VERIFIED (layout), meaning GUESS |
| 0xF12E5E12 | – | 368 = 92×4 | Companion of B1422971 | unknown |
| 0xB125533A | – | 828 = 92×9 | Per-level lot grids (floors etc.) | VERIFIED (header) |
| 0x0563919E | – | 92 | Wall/floor compositor | WIKI |
| 0x11E32896 | – | 92 | Roofs | VERIFIED |
| 0x04A09283 | – | 92 | Stairs | VERIFIED |
| 0x07CD07EC | – | 92 | Front door reference | VERIFIED (size) |
| 0x04F66BCC | – | 92 | Fireplace/chimney parts | WIKI |
| 0x05512255 | – | 92 | Room index | WIKI |
| 0x04EE6ABB | – | 92 | Lot terrain-paint layer list | WIKI + GUESS |
| 0x06DC847E | – | 341 | Lot terrain paint, one per layer (groups 0x00D4DAA4+i) | GUESS |
| 0x05FF6BA4 | 2ARY | 184 = 92×2 | Lot water table (g=0x00FF2067) / slope flags (g=0x00958344) | WIKI + dims VERIFIED |
| 0x0553EAD4, 0x05E4FAF7, 0x05FF3549, 0x060E1826, 0x06393F5D, 0x065B8B38, 0x913381F2, 0x048A166D | – | 92 each | per-lot, undocumented | unknown |
| 0x01661233 | MODL | 217 | 92×2 lot imposters (g=0x00B0C507, 0x001DA7E9) + 33 world (g=2) | GUESS |
| 0x00B2D882 | DDS | 945 | 564 terrain tile textures (g=1) + lot imposter textures | GUESS |
| 0xD84E7FC6 / 0xD84E7FC5 | ICON | 92 / 22 | Lot thumbnails PNG | WIKI |
| 0x0580A2CD/CE, 0x6B6D837D/7E | SNAP | | household/sim PNG snapshots | WIKI |
| 0x0166038C | NMAP (_KEY) | 1 | Name map | WIKI |
| singletons | | | see §12 | |

**Bug in our repo:** `crates/s3pkg/src/types.rs` maps `0x04A4D951` to `"CPRX"`. In s3pi it is
`WDET` (`.wrlddetail`); CPRX is `0x04AC5D93`.

---

## 3. Terrain

### 3.1 Heightmap — 0x2AD195F2  [VERIFIED]

```
u32 width          // 2049
u32 height         // 2049
f32 scale          // 0.0152587890625 = 1000/65536 m per unit
f32 ?              // 1.0
u32 ?              // 1
u16 h[height][width]   // row = world Z, column = world X
```
`height_m = h * scale`. Size is exactly `20 + 2049*2049*2 = 8,396,822`. Sunset Valley: 0 .. 414.2 m,
ocean floor = 0.0. Lots are already flattened into this heightmap.

### 3.2 Terrain layers — 0x9063660D WTXT  [VERIFIED header and list; tile tail partial]

```
u32 version        // 11
u16 ?              // 0
u32 mapWidth       // 2048   (offset 6)
u32 mapHeight      // 2048
u32 ?              // 1
u32 ?              // 1000   (= heightmap scale × 65536, i.e. max terrain height)  [GUESS]
u32 layerCount     // 13     (offset 22)
layerCount × {
    u32 type       // 0x00B2D882 (DDS)
    u32 group
    u64 instance   // = FNV64(name)
    str16 name     // e.g. "grass_medium_base"
}
u32 tileCount      // 64 (8×8 tiles of 256 m)
tileCount × {
    u32 size
    u32 x0, y0, w, h       // tile rect in metres
    f32 cx, cy             // tile centre
    u32 n
    u8  layerIdx[n]        // layers present in this tile
    ...                    // remainder not decoded
}
```
The terrain textures themselves are DDS (type 0x00B2D882) looked up by TGI in the game packages
(FullBuild/terrain packages), not inside the .world. Layer index `i` is the "texture" byte used by
the 0x3D8632D0 layers.

### 3.3 Terrain paint masks — 0x3D8632D0  [VERIFIED by coordinator + WIKI]

```
u16 version       // 0x0300
u16 magic         // 0xA5FF        (bytes 00 03 FF A5)
u32 mapWidth      // 2048
u32 mapHeight     // 2048
u32 blockWidth    // 16
u32 blockHeight   // 16
u32 layerCount
u32 textureCount  // (absent in WBND)
layerCount × {
    zero-pad to 4-byte alignment (from resource start)
    u32 size          // bytes of this layer after the next field
    u32 blockCount    // non-empty blocks
    u8  texture       // index into the 0x9063660D layer list
    blockCount × {
        u16 blockIndex    // BIG-endian, row-major in a (2048/16)×(2048/16) = 128×128 block grid
        u8  kind          // 1 = constant, 0 = raw
        kind==1: u8 value              // whole block
        kind==0: u8 value[16][16]      // row-major
    }
}
```
Missing blocks = 0 weight. Orientation: block row = world Z (same as the heightmap) [GUESS, consistent
with the rendered `cache/sv/blend.png`]. The wiki claims the game does not read this at runtime and
instead uses the pre-baked per-tile textures (§10.2), but it is the authoring data and is complete.

### 3.4 Lot-level terrain paint  [GUESS]

- `0x04EE6ABB` (per lot) [WIKI + VERIFIED shape]: `u32 version (0xC); u32 count; count × { u16 residx; u32 value }; ...`.
  `residx` points into the lot's REFS **group 0**, and lands on terrain DDS entries (e.g. Goth lot
  residx 0x93 → `00B2D882:00000000:B30EFEEFFCFAD27E`, which is one of the world terrain textures).
  `value` is probably a coverage count.
- `0x06DC847E` (341 = one per lot paint layer; group = `0x00D4DAA4 + layerIndex`, up to 12 layers):
  header `u32 1; u32 241; u32 241; u32 ?` then a packed payload (lot width×4+1 = 241 → 0.25 m
  resolution for a 60-tile lot). Payload looks RLE-ish (`00 80` repeats); not decoded.

### 3.5 Sea level — 0xB074ACE6  [GUESS, strong]

```
u16 version     // 3
f32 seaLevel    // 28.0730 m in Sunset Valley
u32 ?           // 2
u32 count       // 21612
count × u8[11]  // unknown 11-byte records (shoreline/ocean mesh data?)
```
Size check: `2+4+4+4 + 21612×11 = 237,746` = resource size exactly. Supporting evidence: ocean floor is
0.0 m; the lowest beach lot origins are at 28.8 m (RecurveStandBeach) and 32.3 m; CAW has a per-world
"Change Sea Level" tool, so the value must be stored somewhere per world, and this is the only
world-level float in that range. Confirm by rendering water at 28.07 m.

Lot-local water (ponds/pools): see 2ARY (§8.6) and pool data in the lot grids.

---

## 4. Layers

### 4.1 DETL — 0x03D86EA4  [VERIFIED]

The CAW "layers": every world object belongs to a layer; neighbourhood layers group lots.

```
u16 version          // 4
str16 name           // "treeLayer", "StreetObjects", "Parkview_nbhd_01", "FishSpawners", ...
u32 count
count × { u64 childId; u32 kind }   // kind 1 = lot id, kind 2 = child layer (DETL instance)
```
Sunset Valley layers (instance → name): `7D6F0013F6F4E4E0 treeLayer`, `0908001FCBB4FB60 StreetObjects`,
`7D6F001262B2E130 GlobalLayer`, `0908001B2123AA10 BeachObjects`, `0908001BD6A82E20 MountainObjects`,
`6C11001CBBD626E0 SeedSpawners`, `…ED4790380..3C0` Beetle/Butterfly/Gem/Metal/Meteor spawners,
neighbourhood layers (`downtown_nbhd0`, `OceanSidenbhd4`, `Bluff_nbhd3`, …) whose children are lots
plus a `*_streetdetail` child layer.

**Each DETL instance has a matching OBJN (0x4D1A5589) + REFS (g=0x00F0B54D) with the same instance**
holding that layer's objects. The 92 lots likewise each have OBJN + REFS. 92 + 42 = 134 OBJN.

### 4.2 WDET — 0x04A4D951  [WIKI]

```
u32 count
count × { u64 guid; str16 detailIdentity }
```

### 4.3 0x90620000  [VERIFIED]
`u32 version (2); u32 count (32); count × { u64 layerId; u32 2 }` — all ids are DETL instances.

---

## 5. REFS — 0x05ED1226  [VERIFIED]

A TGI table. Other resources refer to entries by a 16-bit **`residx`** that matches the per-entry
index field (indices are *not* the array position; they can be sparse/out of order).

```
u16 version          // 3
if version >= 3: u8 wide       // 0 in all SV files
u32 count
count × {
    TGI  key          // u32 type, u32 group, u64 instance
    wide ? u32 index : u16 index
}
u32 count2
u16 list2[count2]    // purpose unknown (lot group-0 REFS only)
```
Which REFS to use:
- OBJN (lot or layer instance X) → REFS `05ED1226:00F0B54D:X`.
- Lot building resources (walls, roofs, stairs, B1422971, 04EE6ABB, compositor …) of lot X →
  REFS `05ED1226:00000000:X` (contains CWAL 0x515CA4CD, CWST 0x9151E6BC, CFEN 0x0418FE2A,
  CRAL 0x04C58103, CSTR 0x049CA4CD, CRST 0x91EDBD3E, CRMT 0xF1EDBD86, TXTC 0x033A1435,
  TXTF 0x0341ACC9, COMP 0x044AE110, DDS, _XML).
- Verified examples: roof residx 426/427 → CRMT/CRST; stair residx 429/430 → CSTR.

---

## 6. OBJN 0x4D1A5589 — placed objects

This is **the** placement data for everything placed in the world and on lots (trees, rocks, street
lamps, mailboxes, furniture, doors, windows, plants, rabbit-hole buildings, effects, spawners…).
Positions are world space. Parser below consumes 133/134 Sunset Valley OBJN exactly to the end of the
object section (10,119 objects); the one failure is a GlobalLayer object with the `Sim` component
whose animation state uses a record type (0x19) we did not decode.

Worlds saved by later versions of the game (Appaloosa Plains, EP5; same header version 0x14 and
object version 10) differ in two places: the model component's visual-state block ends in 4 bytes
rather than 3 (`9 + 24 + 4`: its six floats are the object's bounds), and each object record has
4 more bytes after the audio component. Nothing in the headers tells them apart: try the 3-byte
tail and then the 4-byte one, and take the one after which the next object's header follows
within 8 bytes (then keep it for the rest of the resource). Before this, 219 of the Pinkertons'
223 objects failed to parse (their house stood bare: no doors, windows or furniture). [VERIFIED]

The animation state holds record types not decoded here (0x19 in Sunset Valley; 0x075E3ECB and
0x075E4C89 in later worlds). Everything that matters about an object (catalogue entry, position,
rotation, model, design) comes before it, and the script after it: on an unknown record, the
script component is found by its name (`u32, u32 length 8..256, "Sims3..."`) before the next
object's header, and the object read on from there. Failing that, an object is resynchronised on
the first header after its own (never skipping the next object). What follows the script (the
physics to audio components) isn't needed to place an object and is read as far as it goes: a
Sim standing in a later world (`Sims3.Gameplay.Actors.Sim`, whose SACS block is longer) is kept
but dropped at load, for the town's people come from its households. A script that won't read
where the animation state left off is found by its name too. With all this `s3tool objnstats
<world>` counts every placed object of every installed world; Oasis Landing's three short are
entries of a world-builder helper's 67 KB property blob (ending in an `NOBJ` table), not placed
objects. [VERIFIED]

### 6.1 Header  [VERIFIED]

```
u32 version              // 0x14
u32 ?                    // 1
u32 0x02DC343F           // OBJK type
u32 nObjk
nObjk × { u64 instance; u32 hash }     // distinct catalog instances used (dependency list)
u32 ?                    // 1
u32 0x736884F1           // VPXY type
u32 nVpxy
nVpxy × { u64 instance; u32 hash }     // distinct VPXY instances used
f32 lotX, lotY, lotZ, lotHeading       // lot origin + heading for lot OBJN; zeros for layers
u32 nobjOffset           // absolute offset of the "NOBJ" tag later in the resource
u32 0x1C
i32 -1
u32 objectSectionLength  // bytes of the object section that follows
-- object section (6.2) --
-- trailing property/component table + "NOBJ" block (not needed for placement) --
```

### 6.2 Object section  [VERIFIED except where noted]

```
u32 objectCount
objectCount × Object

Object:
  u64 guid                 // object id (referenced by containment, routing, front door, etc.)
  u32 ?                    // always 10
  u32 nComponents
  u32 component[nComponents]   // FNV32(lowercase component name), any order
  u16 residxCatalog        // -> REFS: OBJD 0x319E4F1D (8632), OBJK 0x02DC343F (1044),
                           //    _SPT 0x00B552EA speedtree (131), 0x2DA18F83 (312, unknown type)
  u16 residx2              // always the null TGI
  u16 residxVpxy           // -> VPXY 0x736884F1 (null for trees)
  then component data in this FIXED order, each only if the component is present:
```
Component hashes (FNV32 of the lower-case name) — the data order is fixed regardless of list order:

| # | name | FNV32 |
|---|---|---|
| 1 | location | 0x461922C8 |
| 2 | transform | 0x54CB7EBB |
| 3 | model | 0x2954E734 |
| 4 | animation | 0xEE17C6AD |
| 5 | script | 0x23177498 |
| 6 | physics | 0x1A8FEB14 |
| 7 | tree | 0xC602CD31 |
| 8 | effect | 0x80D91E9E |
| 9 | sim | 0x22706EFA |
| 10 | steering | 0x61BD317C |
| 11 | sacs | 0x3AE9A8E7 |
| 12 | slot | 0x2EF1E401 |
| 13 | lighting | 0xDA6C50FD |
| 14 | visualstate | 0x50B3D17C |
| 15 | footprint | 0xC807312A |
| 16 | audio | 0x3FC40859 |

```
location:    f32 x, y, z                         // WORLD space
transform:   f32 qx, qy, qz, qw                  // rotation quaternion
             u64 parentGuid                       // containing object (0 = none)
             if parentGuid != 0: u32 slotNameHash; u16 ?   // object sits in parent's slot
model:       u16 residxModel                      // -> MODL 0x01661233 (group 1)
             u16 residxCompositorProp1, u16 residxCompositorProp2
             u32 n4
             n4 × { u16 residx; u32 8; u32 lenMinus1; u8 rest[lenMinus1 + 1] }   // [WIKI shape; n4=0 in SV]
             u16 residxComplate                    // COMP 0x044AE110 preset (colour/pattern)
             if REFS[residxComplate] is not the null TGI:
                 u32 2; u32 2; u32 blockLen
                 if blockLen != 0:
                     u8 block[blockLen]            // the object's own design [VERIFIED]: u16 0x42,
                                                   // u32 TGI offset, u32 TGI size, then a compact
                                                   // complate (as in a CWAL) whose Tgi indices go
                                                   // through residx[] below (its TGI list is empty)
                     u8 n6; u16 residx[n6]         // -> REFS (the OBJN's own)
             if visualstate component present:
                 u8 ?[9]                           // [WIKI: 4 bytes, u32 MTST state hash, 1 byte]
                 f32 bbox[6]                       // minX,minY,minZ,maxX,maxY,maxZ (object space)
                 u8 ?[3]                           // first is 1
animation:   u16 residxRig                         // -> _RIG 0x8EAF13DE
             u8  active
             if active: AnimationState (6.3)
script:      u32 6
             u32 len; char className[len]          // ASCII, e.g. "Sims3.Gameplay.Objects.Lighting.LightOutdoorStreet"
             u8 ?; u32 ?; u32 scriptObjectId       // -1 for every object in the .world (see §7)
physics:     u8 enabled
tree:        u16 residxSpeedTree                   // -> _SPT 0x00B552EA
             u32 ?
             u32 n
             n × { f32 m[16]; f32 scale }          // m = 4x4 row-major: rows 0-2 rotation (+0 column),
                                                   // row 3 = (x, y, z, 1) world position
effect:      u16 ?; u16 ?; u8 ?; u8 len; char name[len]   // e.g. "simsflag"  [VERIFIED on 3 objects;
                                                   // wiki says u16,u8,u8 len — that is 2 bytes short]
sim:         u16 residxOutfit; u16 ?               // [WIKI]
steering:    u8; u32                               // [WIKI]
sacs:        u16; u8; u8 n; u32 driver[n]          // [WIKI]
slot:        u16 residxSlot                        // -> RSLT 0xD3044521
lighting:    u16 residxLite                        // -> LITE 0x03B4C61D
             f32 ?[4]                              // typically (1,-1,-1,-1)
visualstate: u8 ?[6]; i32 -1
footprint:   u8 ?; u16 residxFtpt                  // -> FTPT 0xD382BF57
             u32 n; u32 ?[n]
             u16 residxFtpt2; u32 n; u32 ?[n]
             u8 ?; f32 ?                           // 0.123
audio:       u32 ?; f32 ?, ?; u32 ?; f32 ?         // 20 bytes
```

To instantiate an object: `REFS[residxCatalog]` gives OBJD/OBJK `(type, group 0, instance)`; for EA
objects the same instance is used by the OBJD (0x319E4F1D) and OBJK (0x02DC343F) in
`FullBuild0.package` (e.g. buildingPlacement layer: OBJK instance 0x485; the Spa lot's REFS lists
OBJD instance 0x78A while its OBJN header lists the same 0x78A under the OBJK type). `REFS[residxModel]` /
`REFS[residxVpxy]` give the exact MODL/VPXY. `REFS[residxComplate]` is the colour/pattern preset.
Final transform = `T(location) · R(quaternion)`; for `tree`, use each instance matrix and scale.
Objects with `parentGuid != 0` (260 in SV) are slotted into another object (e.g. flag effects on a
pole, items on counters); their location is still given in world space.

**Designs.** An OBJD's materials are its catalogue designs: each `u8 type, (u32 if type != 1),
u32 len, u16 0x42, u32 TGI offset, u32 TGI size, complate, TGI list, u32`, the complate usually
`ObjectRgbMask` / `ObjectRgbaMask` (mask, overlay, multiplier, specular and up to four pattern
blocks). Rendered at the size of the object's composited (TXTC) diffuse, a design stands in for
that texture on every mesh using it; the shipped TXTC is one of them (not always the first: the
country couch ships its third, red plaid). Placed objects keep their own design inline in the
model component (above): in Sunset Valley 7,277 of 10,483 objects have one. Most name a legacy
pattern at the top (`Pattern A = OLD\defaultWood`), but that's only a name: each pattern's block
carries the pattern it's drawn with (its file, colours, tiling), and drawn they come out as their
builders chose (on the Landgraabs' lot 391 of 441 designs name one, and none draws flat: mission
oak desks, marble columns, blue-and-white patio chairs), so every design is drawn. Compared with
the catalogue's designs, a placed object's has an extra `daeFilePath` and fewer decimals.
[VERIFIED]

Fences: a lot's `0x913381F2` holds its fence posts (`u32 level, f32 x, f32 z, u16 REFS index` of
the fence's CFEN), at its ends and corners and every few metres along it (not at every tile).
The runs are edges of the wall graph `0x312E7545:0x002E7B1E` (the room-boundary graph
`0x002E7B1C` with every fence besides, a garden's too) that aren't walls (`0x002E7B1A`): an edge
is fenced where a post stands within three steps along its line on both sides, in the style of
the post at either end (else the nearest before it). [VERIFIED by eye on the Frio, Goth,
Landgraab and Bachelor lots: privacy fences right round, a garden's edging, brick walls with
iron railings]

Sunset Valley totals: 10,119 objects; 406 tree objects carrying 5,596 tree instances; script classes
include windows/doors/stairs/street lights/mailboxes/parking spaces/rabbit holes.

### 6.3 AnimationState (when `active != 0`)  [WIKI + VERIFIED on 25 resources]

```
u32 1
u32 count
count × {
    u32 priority                         // e.g. 10000
    loop {
        u32 more;  if more == 0: break
        u32 type;  if type == 1: type = u32
        if type == 0: u32[4]; continue
        u32 1; u32 id; u8 flag           // common prefix
        switch type:
          0x060CAEEF: u32, u32, u32, u32 flags, f32, f32
          0x0664ED68: u32 c1; u32 1; u32 c2; f32[c1*c2]; u32; u32 flags; f32; f32
          0x0664FB68: u32, u32, u32 flags, f32, f32
          0x067C5CAB: u32, u32, f32, f32
          0x0681C688, 0x073B9D8C, 0x07A11BA8, 0x34F49F94: u32
          0xB489FC2C:                      // active clip
             u32 2; u32 flag
             TGI clip (0x6B20C4F3 CLIP); TGI trackMask (0x033260E3 TKMK)
             u32 c1; u32 c2; i32 -1
             if c2 > 0: repeat { u64 guid; u32 values until 0; u32 actor; u32 slot } until actor==0 && slot==0
             u64 guid
             if flag: u32 n; n × { u32 actorNameHash; u32; u32 flags; u32; u64 actorGuid; u32 0 }
             f32[3]; u32[3]; u8[3]
             u32; u32 flags; f32; f32
    }
}
```
(The wiki's extra conditional "FLOAT FLOAT" rule after 0xB489FC2C was not needed for any SV object.)
Sim objects (component `sim`) use another record type (0x19) — undecoded; affects 1 resource.

---

## 7. OBJS 0x06B981ED — serialized script state  [WIKI + header VERIFIED]

Not placement data. It is the persisted C# object graph (ScriptCore persistence): households,
SimDescriptions, `Sims3.Gameplay.Core.Lot` + `Lot+SavedData`, `LotManager`, etc. (380 type names).
Every OBJN script component in the .world has `scriptObjectId = -1`, i.e. world objects get fresh
script instances from their class name; OBJS matters for gameplay (pre-made families, lot ownership),
not for rendering.

```
u32 version        // 0x500
char magic[4]      // "OBJS"
u32 typeCount      // 470
u32 instanceCount  // 52054
u32 typeDefsOffset       // (version >= 0x200)  0x1A3EC3
u32 instanceTableOffset  // (version >= 0x200)  0x17116B
u32 tgiTableOffset       // (version >= 0x500)  0x1B6C26
-- instances from offset 28: each = u8 PersistedTypeCode + data --
-- instance table: u32 offset[instanceCount] (1-based ids; 0 = null) --
-- type definitions --
-- TGI table: u32 n (252); n × TGI (mostly SIMO 0x025ED6F4 outfits) --
```
Type definition grammar, the 0x00–0x1A PersistedTypeCode table (0x01 reference, 0x09 int,
0x0C single, 0x10 object, 0x11 array, 0x17 enum, 0x19 ResKey → TGI index, …) are fully listed on the
[wiki page](http://simswiki.info/wiki.php?title=Sims_3:0x06B981ED); first type names in SV:
`ScriptCore.ScriptObjectGroup`, `Sims3.Gameplay.Core.Null` with fields `mValueModifier:int`,
`mPurchasedPrice:int`, `mOwnerLot:ref`, `mFlags:enum` … Treat as a later-phase task.

**Premade Sims' outfits** [VERIFIED on Sunset Valley]. A `SimDescription` holds
`mDefaultOutfitKey` (a SIMO key) and `mOutfits`, an `Sims3.Gameplay.CAS.OutfitCategoryMap`
that serializes without a field list: `u8 0x10, u32 class, u32 n`, then `n` ×
`{u32 OutfitCategories, u64 instance, u32 group, u32 type}` (one SIMO per category: 1 everyday,
2 formal, 4 sleepwear, 8 swimwear, 0x10 athletic, 0x20, 0x40, 0x100 ...). Those SIMO resources
are **not in the world file nor in any installed package** (searched every package index for
the instances; the only hits are the OBJS key table and the REFS manifests `05ED1226:00BBB9D2`
and `05ED1226:00F0B54D:*`, which list them as dependencies), so a premade Sim's real clothes
and hair can't be recovered from an install; the household-bin families in
`GameData/Shared/NonPackaged/Library/*.package` do ship their SIMO (with TXTC and `0x0341ACC9`).
The SimDescription's `mGeneticHairstyleKey` (a `0x00000013` key) and hair colours are there.

---

## 8. Lots

### 8.1 LDES 0xD063545B — lot description  [VERIFIED, version 32]

```
u16 version              // 32 (wiki: 31, 32, 43, 45, 69 in later EPs)
u64 nameHash             // FNV64(lower(nameKey))   (verified)
u64 descHash             // FNV64(lower(descKey));  0xCBF29CE484222325 = FNV64("") when empty
u64 thirdHash            // FNV64(lower(thirdKey))
str16 internalName       // designer name: "Graveyard", "10SHillHghtsLgraab", "24SimLPviewSekemot"
if version >= 32: f32 ?, f32 ?          // 0, 0 in SV
f32 x, y, z              // lot origin (lot-local (0,0,0)) in world metres; y = terrain height there
f32 heading              // radians about +Y
u32 width                // tiles (1 tile = 1 m) along lot-local +X
u32 depth                // tiles along lot-local +Z
i32 lowestLevel          // 0, or -1 when there is a basement/pool level        [GUESS]
u32 levels               // building storeys, 0 = empty lot                      [GUESS]
f32 minHeight            // lowest building Y relative to lot (≈ -3 with basement) [GUESS]
f32 maxHeight            // highest building Y relative to lot (≈ 3 m per storey) [GUESS]
u32 levels2              // levels incl. roof? (≥ levels)                        [GUESS]
u32 ?                    // 1
str16 nameKey            // "World/SV/HouseName:Goth", "Gameplay/Core/Lot/CommunityLotName:CityHall",
                         // "World/Pleasant_Valley/LotName:650770269035414016", "CatalogObjects/Name:Hospital"
str16 descKey            // "World/SV/HouseDesc:Goth" or empty
str16 thirdKey           // "World/SV/LotAddress:Graveyard" or "World/Pleasant_Valley/LotName:<n>"
u32 lotType              // 0 = residential, 1 = community (wiki: 2 = tutorial)
if version >= 43: u32 subtype
f32 vistaBuff            // beautiful-vista moodlet strength: 0/10/15/20/25/30
f32 lotValueModifier     // 0/1500/3000/5000/7500 (paired with vistaBuff)
u32 furnishedValue       // 0 in SV
if version >= 43: u32; u8[5]; if version == 69: u8[11]      [WIKI]
```
Placement (verified against the 0x046A7235 matrices):
`world = (x, y, z) + Ry(heading)·local`, `Ry: x' = x·cosθ + z·sinθ, z' = −x·sinθ + z·cosθ`,
with `local.x ∈ [0, width]`, `local.z ∈ [0, depth]`. The OBJN header of a lot repeats `(x, y, z,
heading)`. Names are STBL (0x220557DA) keys; look them up by FNV64 in the game string tables.

Examples: CityHall (1032.0, 42.9, 1032.0) θ=0 48×64 community; Goth house (1432.0, 79.8, 1056.0) θ=π
60×60 residential (levels 5, basement); empty lot "15SHillHgtsEmpty" (904, 89.1, 692) θ=π 60×60,
levels 0, vista 30/7500.

### 8.2 0x046A7235 — lot location + routing  [WIKI; first part VERIFIED]

Big (15 MB total). Mostly route-planning data (portals, object/wall/floor footprints by object guid).
**No object transforms** beyond the lot matrix. Start of the resource:

```
u32 5; u32 2; u32 7
u32 section1Size
u32 section2Size
u64 lotId
u16 ?; u16 ?
u32 totalSubcount2b; u32 id18Count
u32 ?[4]; u8 ?[3]
f32 worldToLot[16]      // translation row = −Rᵀ·pos   (the wiki labels these two the other way round)
f32 lotToWorld[16]      // translation row = LDES (x, y, z)
u32 3; u64 lotId; u64 lotId
f32 (−0.5 | 1.0); f32 0.0; f32 0.123
u32 flags               // 0x04000000 / 0x84000000
f32 minX, minZ, maxX, maxZ   // −0.5, −0.5, width+0.5, depth+0.5 (lot-local)
... (rest: see wiki page — room graph, portals, footprint polygons)
```

### 8.3 Other per-lot resources

| Type | Layout | Conf. |
|---|---|---|
| 0x07CD07EC front door | `u32 5; u32 2; u64 lastDoorUsed?; u64 frontDoorObjectGuid` (24 bytes) | WIKI + size VERIFIED |
| 0x04F66BCC fireplaces | `u32 n; n × { u32 ?; u8 m; u64 objectGuid[m] }` | WIKI |
| 0x05512255 room index | `u16 9; u16 1; u32 0; u32 n; u16 idx[n]` | WIKI |
| 0x0498DA7E (singleton) | `u64 globalLotId` = `0x7D6F001262B2E130`, which is also the DETL "GlobalLayer" instance (its OBJN holds world-global objects, incl. Sims) | WIKI + VERIFIED |
| 0xD84E7FC6 / 0xD84E7FC5 | PNG thumbnails | WIKI |

### 8.4 2ARY 0x05FF6BA4  [WIKI + dims VERIFIED]

```
u32 version (1); u32 cols; u32 rows
group 0x00FF2067: f32[rows][cols]   // (width+1)×(depth+1) vertex grid: water table height, relative
                                    // to the lot (Goth: −1.55 .. 0.62)
group 0x00958344: u8[rows][cols]    // cols = 4×width: per tile 4 bytes (one per direction);
                                    // bit0 = steep enough for rock texture
```

**Ground dips** [VERIFIED by rendering]: the ground grid is lower than the flattened world in
places other than ponds too (Central Park's plaza fountain sits 0.75 m down in a pit the size
of its basin, with the plaza paving sloping down to it). Ground-level paving follows the ground.

**Ponds** [VERIFIED by rendering, 2026-10-05]. The water table is x-major (`[x][z]`, like the
level-height grid of §9.3) and relative to the same base as the lot's ground level. Wherever the
water table is above the lot's ground there is water: Summer Hill Springs' pond, its island, and
the three fishing spawners at 88.765 m all line up with water at base 89.04 − 0.27. Dry ground
holds −1.0 or other values below the ground. Each pond has a single level (Crystal Springs sits
8.63 m below its lot's base, between rocky hills). The pond basins are **not** in the world
heightmap (0x2AD195F2), which is flat under lots: they come from the lot's own ground grid
(§9.3, 0x0093D6D4). Pools on residential lots (the Goths') show up the same way.

The world's water resource 0xB074ACE6 is the **sea only**: header `u16 3, f32 seaLevel, u32 2,
u32 n`, then n × 11-byte vertices of a triangle list (`i16 x, i16 z` in quarter-metres, the sea
reaching beyond the world edge; `i16` clamped shore depth or ±32700; 5 bytes of flags and packed
normal).

---

## 9. Lot buildings

TS3 builds walls/floors/roofs procedurally at load from graph/grid data + catalog patterns.
There are **no pre-baked building meshes at full detail** in the .world (only the low-LOD imposters
in §10.1). All residx values here resolve through the lot's REFS **group 0**.

### 9.1 Wall graphs 0x312E7545 (groups 0x002E7B1A, 1C, 1D, 1E, 1F)  [VERIFIED layout]

```
u32 1
i32 -1                    // wiki: 0
u32 gridW                 // lot width + 1
u32 gridH                 // lot depth + 1
u32 levelCount            // (1..7) number of levels the lot spans
u32 maxVertexIndex
u32 maxEdgeIndex
u32 nVertices
nVertices × { u32 vertexId; f32 x; f32 z; i32 level }   // lot-local grid coords (integers), level -1 = basement
u32 nRooms
u32 roomId[nRooms]
u32 nEdges
nEdges × { u32 edgeId; u32 v0; u32 roomA; u32 v1; u32 roomB }   // roomA/B = room on either side
```
Parses exactly for all 460 resources. Channel meaning [GUESS]: `1A` has data only on lots with real
walls (empty for rabbit-hole lots Spa/Graveyard) → **walls**; `1C` contains the lot perimeter on the
graveyard (fence) and carries room ids → walls+fences room-partition graph; `1D` small closed loops
(foundations/platforms?); `1E` ≈ union of 1C with a single room (outline graph); `1F` always empty
(pools?). Determine definitively by rendering each channel over a lot.

### 9.2 Wall-side materials 0xB1422971 (groups 0x002E7E1D, 0x002FDACF, 0x004BFAAB, 0x0082079A, 0x00DD33E4)  [layout VERIFIED, meaning GUESS]

```
u32 version (4)
u32 n
n × { u32 edgeId; u16 residxWall; u16 residxSideA; u16 residxSideB }   // 0xFFFF = none
```
`residxWall` resolves to CWST 0x9151E6BC (catalog wall). The three channels 004BFAAB / 0082079A /
00DD33E4 hold, per wall side, references that land on CWAL (pattern) / TXTC (texture compositor) /
COMP (preset) respectively → one channel each for pattern, compositor and colour preset [GUESS].
`0xF12E5E12` (4 groups, `u32 2; u8[16] 0; u32 n; …`) is the companion — undecoded.

### 9.3 Lot grids 0xB125533A (9 groups)  [header VERIFIED]

```
u32 1
u32 w            // lot width (tiles) or width+1 (vertices)
u32 h
u32 levels
payload = w × h × levels × bytesPerCell     (sizes verified for the Goth lot, 60×60, 7 levels)
cells are x-major: index = (level × w + x) × h + z   [VERIFIED: Old Pier Beach's 61×38 paving,
                                                      Summer Hill Springs' pond vs its fishing spots]
```

**Grid levels** [VERIFIED on the Bachelor, Goth, Bunch, Andrews and pool-center lots]. The grid
levels are the lot's levels from the lowest: on some lots (15 of 92 in Sunset Valley: the Goths',
the pool center, ...) level 0 is 3 m down, for pools and basements, and the ground is level 1;
elsewhere the ground is level 0. Find the ground `g` as the level whose heights (0x0093D6D4,
relative to the lot) sit nearest 0. A house stands on a foundation when level `g+1` is 0.75 m up
under it (3 m elsewhere). Storey `n` (1 = ground floor) is grid level `g + n + F - 1` (F = 1 on a
foundation); its floor tiles are exactly the cells with a tile there (not every area the rooms'
boundaries enclose: fenced yards aren't floored). With a foundation, level `g` holds the ground
paving around it; without, the ground floor and the paving share level `g`. Pool floors are
the tiles on level `g - 1`, and walls on graph level 0 of such lots are the pool's sides. The
wall graphs number storeys the same way on every lot (graph level 1 = ground floor).

**Foundations are per column, not per lot** [VERIFIED on the Koffis', Wainwrights', Goths' and
Landgraabs' lots]. A house can have a room on a foundation beside rooms without one: the
foundation room's floor is on grid level `g+1` (0.75 m up), while the rooms beside it have their
ground floor on level `g` and their *second* storey on level `g+1` (3 m up). So one grid level
mixes storeys (Koffi level 2: 24 tiles at 0.75, 211 at 3.0; Wainwright level 1: 18 at 0.75,
240 at 3.0). Take each tile's storey as its grid level above `g`, plus one, less one where its
column is on a foundation: level `g+1` there is 0.3–2.4 m above level `g` (the Goths' manor is
1.64 m up on its slope), not a storey's 3 m; or, where the ground falls away more than that
under a floor on `g+1`, level `g+1` is 0.3–2.4 m over the lot's own level (Twinbrook's stilted
houses over the swamp: the Bulls' floor is 0.75 m up on brick piers over ground 2–3 m lower;
without this its floors counted a storey up and the ground floor showed none). Read the heights at the tile's own (lowest-index)
vertex: vertices on the border between such rooms can only hold one height. Floors above the
walls' storeys (roof terraces, the Koffis' open-air top floor) are storeys too.
`s3tool splitlevels <world>` lists the lots whose grid levels mix storeys.
```
```
| group | grid | bytes/cell | Goth lot sample | guess |
|---|---|---|---|---|
| 0x002E7B0E, 0x002E7CF0, 0x002E7CF1 | tiles | 8 | zeros/ids | floor pattern per quarter-tile (4×u16), 3 material channels like walls |
| 0x0093D6A5 | tiles | 1 | 0xFF | flags |
| 0x00A6D544 | tiles | 1 | 0 | flags / room id |
| 0x0093D6D4 | vertices (61×61) | 4 (f32) | −2.80 | per-level vertex heights, x-major `[level][x][z]`, lowest level first (basements); the ground level's grid is the lot's sculpted terrain relative to its base (pond basins included). Ground = the level whose edge, on the base that fits the surrounding world terrain, sits nearest the lot's height; upper levels are ground + 3 m per storey |
| 0x0093DEB7 | vertices | 8 | | |
| 0x0093D6D6 | tiles, levels−1 | 16 | 0xFF.. | |
| 0x00C0A9F3 | – | 0 | header only | |

### 9.4 Roofs 0x11E32896  [VERIFIED]

```
u32 version (4)
u16 residxRoofMaterial     // -> CRMT 0xF1EDBD86 roof pattern
u16 residx2                // -> CRST 0x91EDBD3E roof style
f32 roofHeight
u32 ?                      // highest index used
u8  autoRoof?
u32 n
n × { u32 id; f32 x1, z1; u32 level1; f32 x2, z2; u32 level2; u16 residxRoofType; f32 height }
```
(34 bytes/entry; the rectangle (x1,z1)-(x2,z2) on `level` is one roof piece.)

### 9.5 Stairs 0x04A09283  [VERIFIED]

```
u32 version (3)
u32 n
n × {
    u32 ?
    u16 residxStairs          // -> CSTR 0x049CA4CD
    u64 objectGuid
    u32 direction             // 0, 2, 4, 6 (×45°)
    f32 x1, z1; u32 levelTop
    f32 x2, z2; u32 levelBottom
    u32 nRailings
    nRailings × { u32 ?; u16 residxRailing; f32 x; f32 z }   // 14 bytes
}
```

[Verified, this project] (x1,z1) and (x2,z2) are opposite corners of the tiles the staircase
covers; `direction` is the climb: 0 −X, 2 +Z, 4 +X, 6 −Z (×45°), so the side along it is the
run (4 tiles a storey, 1 for foundation steps) and the other its width (often 2). The levels are
the house's storeys (0 = the ground). A CSTR style (0x049CA4CD) holds, at the offset in its
second word + 8, a counted TGI list: three VPXYs (a flight: one tile's steps along +X rising
0.75 m, ~1.08 m wide; the first step; a thin side panel a tile long), a CRAL railing, CWAL
patterns and a CFEN. The game lays a flight per tile per lane, each a tile's rise higher.

### 9.6 Wall/floor compositor 0x0563919E  [WIKI]

```
u32 version (5)
u32 offsetToSection3Marker
-- s1: u32 n; n × { u16 compositorResidx; u32 8; u32 entrySize; u32 0; u8[6]; u32 m; u8 1; u32 rule[m]; u8 0 }
-- s2: u32 n; n × { u16 complateResidx; u32 2; u32 2; u32 size; [u16 66; u32 tgiOff; u32 tgiSize; material block; empty TGI list]; u8 k; u16 tgi[k] }
u32 2
-- s3: u32 size; u32 entries; u32 count; count × { u32 rule; repeat { u32 propertyId; u8 0; u8 dataType; data } until propertyId == 0 }
```
(Property ids / data types as in TXTC 0x033A1435.) This is how custom wall/floor colours are built.

---

## 10. Pre-baked LOD: lot imposters and terrain tiles

### 10.1 Lot imposters  [GUESS, strong]

(Verified since: the imposter atlas's alpha cuts out railings, fences, garden beds and window
frames, so imposters are drawn alpha-tested; the colour looks premultiplied by alpha, so
half-transparent edges come out dark.)
Per lot (instance = lot id):
- `MODL 0x01661233` group **0x00B0C507** (Goth: 454 KB) — low-LOD building model; standard RCOL MODL.
- `MODL` group **0x001DA7E9** (Goth: 1.5 KB) — tiny model (lowest LOD / footprint plate).
- `DDS` groups 0x00B0C507 (512² class), 0x007E7555 (256²), 0x00BC1E4C (256²), 0x00BC1E54 (128²),
  0x001DA7E9 (13 lots only).
These are what the game shows for lots outside the active-lot radius. Parse with the normal
MODL/RCOL reader.

### 10.2 Terrain tiles  [GUESS]
Instance encoding: `instance = (kind:u16 << 48) | (sub:u16 << 32) | (tz << 24) | (tx << 8)` with
tx,tz ∈ 0..7 (256 m tiles); which of the two is X vs Z is not yet confirmed.
- `0xAE39399F` g=1: kind 0 = LOD0, kind 1 = LOD1. Header `u16 0x200; u16 0; u32 vertexCount;
  u32 indexCount; ...` followed by quantized/delta-coded geometry. Vertex counts track terrain
  complexity (flat ocean tile (0,0): 9 verts / 36 indices; hilly tile (6,6): 17,126 verts LOD0,
  1,570 LOD1) → **simplified terrain mesh per tile**.
- `0x033B2B66` g=1 (64): wrapper with tile coords, self type id, quantization scales
  `f32 0.015625 (1/64), 0.01526, 0.015625` and the same vertex count as the AE39399F LOD0.
- `DDS` g=1 (564 = 64×~9): per tile kinds 0x0010 sub 0/1(/2) = 256² uncompressed RGBA (packed
  blend weights, 4 layers per texture?), kinds 2–5 DXT1 256², kind 6 DXT1 128², kind 7 DXT5 256²,
  kind 0 sub 1 DXT5 256² with 9 mips (pre-baked colour?).
- `MODL` g=2 (33, instance = tile index 0..63): per-tile world model (distant objects?).
For a reimplementation, the heightmap + blend masks are sufficient; these are optional optimisations.

---

## 11. Roads, routing, world boundary

### 11.1 Roads & walkways 0x9063660E  [WIKI + VERIFIED on Sunset Valley]
```
u32 version (7)
u16 roadIntersections, walkwayIntersections, roadCurves, walkwayCurves
intersection × { f32 x, z, angle; u8[10] 0; u8 n; n × { u8 dir; u16 index; u8 ? } }
curve        × { f32 bezier[8] /* 4 control points (x,z) */; u8[8] 0; u8 n; n × { u8 dir; u16 index; u8 ? } }
u16[6] (all 1)
6 × { TGI diffuse; TGI bump; f32 width?; f32 height? }
```
Sunset Valley: 54 road intersections, 142 road curves, 37 walkways; the control points are world
x/z in metres and consecutive curves share end points, while curves meeting at an intersection
stop short of its centre (up to ~20 m). Used for traffic (`traffic.rs`).
Indices are 1-based across [road ints, walkway ints, road curves, walkway curves]. The wiki notes
edits to this resource had no visible effect: the road surface is baked into the terrain textures, so
use this for the road graph (traffic, regenerating road meshes).

### 11.2 World routing 0x05CD4BB3  [WIKI]
`u32 1; u32 cols; u32 rows; u32 blockOffset[cols*rows]; blocks{ u16 nVerts; u16 vertOffset[n];
per vertex { u8 nPaths; u8 usage; u8 x; u8 y; nPaths × { u8 destVertex; u8 destBlockRel }; u8 pathType[nPaths] } }`.
Usage 65 lot centre, 66 lot border, 71 footpath, 77 road, 78 footprint; dest block bits
`NNN EEE xx` (3 = no change).

### 11.3 WBND 0x05DA8AF6  [WIKI + header VERIFIED]
Same codec as §3.3 without `textureCount`, 1 layer. `0xFF` outside, `0x80` camera only, `0x00` sims +
camera; missing block = not allowed.

---

## 12. Small singletons

| Type | Bytes (SV) | Meaning | Conf. |
|---|---|---|---|
| 0xD9BD0909 WDNM | str16 `World/Pleasant_Valley/WorldDescription:Name` | world name STBL key | VERIFIED |
| 0x0668F628 WDDT | str16 `…WorldDescription:Description` | description key | VERIFIED |
| 0x022B756C WRDH | u64 | name hash | WIKI |
| 0x35A33E29 WDSH | u64 | description hash | WIKI |
| 0x0668F630 WDNL | u32 92 | number of lots | VERIFIED |
| 0x0668F635 TWNI | PNG | world map image | WIKI |
| 0x0668F639 TWNP | str16 `X:\InGame\Environments\Worlds\PV_UI_256X256.png` | image source path | VERIFIED |
| 0x06CE4804 META | u32[9] = 1, 7, 96, 92, 63, 29, 38, 2048, 2048 | world meta; 92 = lots, 2048 = size; rest unknown | VERIFIED bytes |
| 0xF609FD60 WPID | 16 bytes | world package GUID | WIKI |
| 0x0498DA7E | u64 | global lot id | WIKI |
| 0x044735DD | u32 2; u32 0; u32 64; 64 × (u32 0, u32 i) | per-tile table | unknown |
| 0x90624C1B | 39 bytes: u32 13, …, values 2048, 2048, 30000, 10000, 35000 | camera/LOD limits? | GUESS |
| 0xF0633989 | `01 41 3A 63 B0` | u8 1 + (BE f32 11.65?) | unknown |

---

## 13. Unknowns / next steps

- 0x342779A7 (9 MB, also in saves): `u32 7; u32 1407760; u32 24879; u32 0` then records
  `{u32 index; u32 8; u32 len; u8 data[len+1]}` of bit-packed values. Unknown — possibly routing /
  object "memory" data. Not needed for rendering.
- Exact semantics of the wall-graph channels (9.1), B1422971/F12E5E12 (9.2) and B125533A grids (9.3).
  Suggested experiment: render each channel for the Goth lot and compare with a screenshot.
- Lot terrain paint 0x06DC847E payload codec.
- Per-lot 0x0553EAD4, 0x05E4FAF7, 0x05FF3549, 0x060E1826, 0x06393F5D, 0x065B8B38, 0x913381F2,
  0x048A166D (all small; probably pools, foundations, ceilings, lot-level misc).
- OBJN: `Sim` animation record 0x19; Model `n4` entries; meaning of the 9 + 3 bytes around the bbox.
- AE39399F mesh codec (only needed if the pre-baked terrain is used instead of the heightmap).
- Sea level: confirm `0xB074ACE6` offset 2 by rendering water; the 11-byte records are undecoded.
- Later EPs: LDES versions 43/45/69 add subtype and trailing bytes (see wiki).

## 14. Sources

- SimsWiki per-type pages (raw wikitext via `&action=raw`):
  [0xD063545B](http://simswiki.info/wiki.php?title=Sims_3:0xD063545B),
  [0x046A7235](http://simswiki.info/wiki.php?title=Sims_3:0x046A7235),
  [0x4D1A5589](http://simswiki.info/wiki.php?title=Sims_3:0x4D1A5589),
  [0x06B981ED](http://simswiki.info/wiki.php?title=Sims_3:0x06B981ED),
  [0x05ED1226](http://simswiki.info/wiki.php?title=Sims_3:0x05ED1226),
  [0x03D86EA4](http://simswiki.info/wiki.php?title=Sims_3:0x03D86EA4),
  [0x04A4D951](http://simswiki.info/wiki.php?title=Sims_3:0x04A4D951),
  [0x3D8632D0](http://simswiki.info/wiki.php?title=Sims_3:0x3D8632D0),
  [0x05DA8AF6](http://simswiki.info/wiki.php?title=Sims_3:0x05DA8AF6),
  [0x9063660E](http://simswiki.info/wiki.php?title=Sims_3:0x9063660E),
  [0x05CD4BB3](http://simswiki.info/wiki.php?title=Sims_3:0x05CD4BB3),
  [0x05FF6BA4](http://simswiki.info/wiki.php?title=Sims_3:0x05FF6BA4),
  [0x312E7545](http://simswiki.info/wiki.php?title=Sims_3:0x312E7545),
  [0x0563919E](http://simswiki.info/wiki.php?title=Sims_3:0x0563919E),
  [0x11E32896](http://simswiki.info/wiki.php?title=Sims_3:0x11E32896),
  [0x04A09283](http://simswiki.info/wiki.php?title=Sims_3:0x04A09283),
  [0x04EE6ABB](http://simswiki.info/wiki.php?title=Sims_3:0x04EE6ABB),
  [0x04F66BCC](http://simswiki.info/wiki.php?title=Sims_3:0x04F66BCC),
  [0x05512255](http://simswiki.info/wiki.php?title=Sims_3:0x05512255),
  [0x07CD07EC](http://simswiki.info/wiki.php?title=Sims_3:0x07CD07EC),
  [0x0498DA7E](http://simswiki.info/wiki.php?title=Sims_3:0x0498DA7E).
- Type list: [MTS Sims_3:PackedFileTypes](https://modthesims.info/wiki.php?title=Sims_3:PackedFileTypes),
  [simswiki mirror](http://simswiki.info/wiki.php?title=Sims_3:PackedFileTypes).
- s3pi tags/extensions: `s3pi Extras/Extensions/Extensions.txt` in
  [marcos4503/sims3-package-interface](https://github.com/marcos4503/sims3-package-interface)
  (mirror of Peter Jones' [Sims3Tools](https://sourceforge.net/projects/sims3tools/)). s3pi and
  [ChaosMageX/s3pi-wrappers](https://github.com/ChaosMageX/s3pi-wrappers) have **no** wrappers for
  any world/lot resource; no other public parser was found.
- CAW sea level is per world: [CAW walkthrough](https://www.thesims3.com/content/global/downloads/caw/CaW_WalkThrough.pdf),
  [MTS: water levels Twinbrook/Bridgeport](https://modthesims.info/t/425205).
- LDES background: [pepoluan "puzzling over LDES"](https://simoluan.tumblr.com/post/110264369020/puzzling-over-ldes).

## Lot wall and floor coverings **[verified: rendered and compared in game]**

Implemented in `s3formats::{lotdesign, catalog, complate}` and `s3bake::building`.

### Where each wall side and floor tile gets its look

Per lot (instance = lot id) in the world file, with REFS = `0x05ED1226:0:lot`:

* **Wall sides.** `0xB1422971` channels hold `u32 version (4), u32 count`, then per edge of
  graph `0x312E7545:0x2E7B1E` (the wall graph cut into per-tile edges; its edges carry no room
  ids): `u32 edge id, u16 wall style (REFS → CWST), u16 side A, u16 side B` (palette ids,
  `0xFFFF` = bare). Side A is the edge's **left** — taking the rooms from the `0x2E7B1A` wall
  that contains the edge, in the edge's direction (checked: outdoor sides always land on
  siding).
  * channel `0x002FDACF` + palette `0x002E7DF7` → catalogue patterns (CWAL `0x515CA4CD`);
  * channel `0x00DD33E4` + palette `0x00DD3460` → the lot's own designs (COMP `0x044AE110`);
  * channels `0x004BFAAB` / `0x0082079A` (palettes `0x004BE299` / `0x008207AA`) → TXTC entries
    that are **not shipped** — the game rebuilds them from the designs.
* **Palettes** `0xF12E5E12`: `u32 version (2), 16 zero bytes, u32 count`, then
  `u16 REFS index, u32 palette id, u32 painted area`.
* **Floors.** Grid `0xB125533A:0x0093DEB7`: `u32 1, u32 w, u32 d, u32 levels` then
  `levels × w × d` cells (x-major: `[level][x][z]`, like every 0xB125533A grid; read z-major,
  non-square lots come out as stripes) of four `u16` palette ids (one per tile triangle); `w × d` is the lot
  plus one row and column. Grid level 0 is paving on the ground (driveways often use the
  `Floor_Misc_TarRoof` pattern). Without a foundation, storey `n` is grid level `n`; on a
  foundation, grid level 1 is the foundation's own top (often the unpainted `Floor_Foundation`
  concrete, or a porch covering) and storey `n`'s floors are on grid level `n + 1`, at the same
  height (the Landgraab mansion: furniture at the foundation top, 0.75 m, rooms painted on grid
  level 2, the upstairs atrium's hole on level 3). The level above the top storey holds flat
  roofs. The value counts equal the palette's areas exactly; a palette entry whose references
  are all null (or the `0000DEAD` key) is "no covering". The floor palette is
  `0x0553EAD4:0x0093D9D1`: `u32 4, 16 zero bytes, u32 count`, then
  `u16 CWAL, u16 TXTC, u16 TXTC, u16 COMP (REFS indices), u32 id, u32 area`.
  (The `0x002E7B0E` / `0x002E7CF0` / `0x002E7CF1` grids are something else.)
* **Fences** `0x913381F2`: `u32 version, u32 count`, then `u32 level, f32 x, f32 z, u16 REFS
  index (CFEN 0x0418FE2A)` — one record per **post** (whole-metre points; level 0 = the ground,
  railings on the storeys' levels). The runs between posts are the room graph's (`0x2E7B1C`)
  edges between them. A CFEN names its pieces as VPXYs in its TGI list: a straight run
  (model spanning 0..1 along +X), a diagonal run (0..1.414) and, for some, a post. [VERIFIED by
  rendering the Bachelor lot: yard fence, porch railings, garden edging]
* **CFEN** `0x0418FE2A` (version 10): `u32 version, u32 TGI offset (from byte 8), u32 TGI
  size, u32 material count (0)`, then the catalogue's common block (`u32 version, u64 name key,
  u64 description key, 7-bit-length UTF-16BE name and description key strings
  (`CatalogObjects/Name:FenceRailwayTiesSuite`), f32 price, f32 niceness, f32 crap score, u8
  status (bit 0: in the catalogue), ...`), then the TGI list (`u32 count`, TGI order) of its
  VPXY pieces. The base game shows 19 fences (colour variants are separate CFENs with the
  same name); the thumbnails package has no pictures of them. [VERIFIED]

### The lot's designs (`0x0563919E`)

`u32 version (5), u32 offset of the last section, u32 count`, then three sections:

1. `count` compact records (`u16 id, u32 type (8), u32 len, …`) — object designs, not needed.
2. `u32 n` COMP records, **keyed by the REFS index of the COMP entry** they stand for:
   `u16 id, u32 version (2), u32 flags (2 or 0x40), u32 len` — `len 0` = empty record —
   then a material exactly as in a CWAL (below, minus the leading type byte), then
   `u8 n, u16 REFS index × n`: the resource list the material's `Tgi` indices go through
   (its own TGI list is empty). Found reliably as the section whose records parse cleanly
   up to the third section's offset.
3. Hashed property records (`u32 2, u32 size, u32 count`, …) — not needed.

### Catalogue patterns: CWAL `0x515CA4CD`

`u32 version (0x0D), u32 TGI-list offset (from offset 8), u32 TGI-list size, u32 materials`;
per material `u8 type, u32 len (from after the field), u16 0x0042, u32 TGI offset (relative,
after the field), u32 TGI size, complate…, TGI list (u32 count, then type/group/instance)`,
then 16 trailing bytes. After the materials the catalogue common block (`u32 0x0C, u64 name
GUID, u64 desc GUID`, name and description as 7-bit-length big-endian UTF-16, price, …), then
`u32 pattern type` (1 floor, 2 wall), VPXY index, sort flags, ….

**Compact complate encoding** (s3pi `MaterialBlock`): `u8 xml (TGI index of the recipe),
string name, string pattern slot ("Pattern A"…), u32 n, n × (string param, u8 type, value),
u32 sub-blocks, sub-blocks…`. Strings: `0x80 | len` inline bytes; `0x40 k` = table entry
`0x40 + k`; other bytes = table entry (s3pi's 112-entry `ComplateString.stringTable`, in
`catalog.rs`). Value types: 1 string, 2 ARGB u32, 3 TGI index u8, 4 f32, 5 2×f32, 6 3×f32,
7 bool.

### Roof patterns: CRMT `0xF1EDBD86`  [VERIFIED on the 27 base-game patterns]

`u32 version (3), u32 TGI-list offset (from offset 8), u32 TGI-list size, u32 0x0C, u64 name
GUID, u64 desc GUID`, then the name key as 7-bit-length big-endian UTF-16
(`CatalogObjects/Name:Roof_…`) and a second string, `f32 1.0`, a `0x21`-tagged u64 (the
pattern's own instance), a few flags, and the TGI list: two or three VPXYs `0x736884F1` (group
1) then the three catalogue icons `0x2E75C764/5/6`. The first VPXY lists a LITE, a MATD
`0x01D0E75D` (standalone RCOL), a MODL/MLOD pair (a 24-vertex preview box) and a footprint.
That MATD uses the Roof shader `0x7BD05F63`; its DiffuseMap is a 512² DDS whose instance is
usually the CRMT's own: an **atlas** — the field of tiles fills it and the ridge, hip and
gutter pieces are stamped over it at fixed places (shared layout across patterns), so the
field itself repeats along one axis or both (shingle rows every 102 px; corrugations ~34 px).
`0x2CE11842` is a grey scalar (≈ specular: metals 1.0, thatch 0.04), not a tint. Some roofs
(e.g. "Burnt Red Tile") have greyscale atlases, so their colour comes from elsewhere
(unresolved). CRST `0x91EDBD3E` (16 in the base game) are the roof styles (gable, hip…), with
the same catalogue block.

### Complates `_XML` 0x0333406C

Plain XML: `<complate name>` with `<param type name default>` variables and `<destination>`
lists of `<step type=…>` — the same steps as a compiled TXTC (ColorFill, SetTarget,
ChannelSelect, DrawFabric, DrawImage, HSVShift), with `($Param)` references. Recipe instance
= `fnv64(lowercase name)`; textures named by path resolve to DDS `fnv64(lowercase file
stem)`. Wall recipes (`ObjectRgbaMask`, `ObjectRgbMask`) select patterns A–D through the mask
channels, then multiply by `Multiplier` and lay `Overlay` and stencils on top; each pattern is
itself a complate (a fabric: background image shifted in HSV, masked by `rgbmask`). Natural
sizes: wall masks 256×512 (one tile wide, one storey high), floors 256×256 per tile.
