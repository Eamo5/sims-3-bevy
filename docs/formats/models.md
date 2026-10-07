# Sims 3 object models and materials: byte-level format notes

Status: research notes, compiled 2026-10-04. Covers the PC/Mac Sims 3 resource formats a renderer needs to go
from a catalog object (OBJD) to triangles and textures.

## 0. Conventions, confidence levels and sources

* All multi-byte values are **little-endian** (PC data). `u8/u16/u32/u64`, `i16/i32`, `f32` (IEEE754).
* "TGI" = resource key `(type u32, group u32, instance u64)`. **The byte order of the three fields varies by
  container** and is called out each time (`TGI`, `ITG`, `IGT`).
* `FNV32` below means **FNV-1 32-bit (multiply, then XOR) over the lower-cased ASCII name**:
  `h = 0x811C9DC5; for b in lower(name): h = (h * 0x01000193) mod 2^32; h ^= b`.
  I verified this against ~45 known name/hash pairs from s3pi (e.g. `fnv32("diffusemap") = 0x6CC0FD85`,
  `fnv32("model") = 0x2954E734`); it is not FNV-1a.
* Confidence tags used in this document:
  * **[code]** read logic transcribed from working open-source parsers (s3pi C#, s3py Python). Treat as reliable
    for the byte layout; s3pi round-trips EA files with its `Checking` flag on.
  * **[wiki]** from the Mod The Sims / SimsWiki "Sims 3:0x..." pages (community reverse engineering).
  * **[inferred]** my reasoning (e.g. D3D9 semantics), not confirmed by a source.
  * **[unknown]** genuinely undocumented. See section 11.

Primary sources (all fetched and read for this document):

| Source | What it gave |
|---|---|
| s3pi source, GitHub mirror <https://github.com/marcos4503/sims3-package-interface> (`S3PI-Library-DLLs-Source/s3pi Wrappers/...`). Original: <https://sourceforge.net/projects/s3pi/>, <https://s3pi.sourceforge.net/> | `GenericRCOLResource/GenericRCOLResource.cs` (RCOL + ChunkReference), `MeshChunks/{MODL,MLOD,VRTF,VBUF,IBUF,SKIN,VBSI,GEOM,GeomResource}.cs`, `s3piRCOLChunks/{MATD,MTNF,ShaderData,MTST,VPXY,FTPT,LITE}.cs`, `CatalogResource/{CatalogResource,ObjectCatalogResource}.cs`, `ObjKeyResource/ObjKeyResource.cs`, `TxtcResource/TxtcResource.cs`, `s3pi/Interfaces/{TGIBlock,DependentList,SimpleList,BoundingBox}.cs`, `s3pi Extras/DDSPanel/DdsFile.cs` |
| s3py (Blender "TS3 Tools" Python library), <https://github.com/garthand/s3py> | Independent second implementation: `s3py/io.py` (RCOL), `s3py/buybuild/geometry.py` (MLOD/MODL/VRTF/VBUF/IBUF/SKIN/VBSI), `s3py/model/material.py` (MATD/MTST), `s3py/helpers.py` (FNV32) |
| MTS / SimsWiki format pages, e.g. <http://simswiki.info/wiki.php?title=Sims_3:RCOL> and `...title=Sims_3:0x01D10F34` (same content as `modthesims.info/wiki.php?title=...`) | RCOL, MLOD, MODL, GEOM, VRTF, VBUF, IBUF, MATD, MTST, VPXY, FTPT, SKIN, OBJD, OBJK, LITE, TXTC, DDS pages; "Catalog Resource" and "Key Table" pages |
| "Expensive Toilet" resource-chain diagram by Inge Jones (2010) <https://simswiki.info/wiki.php?title=File:Sims_3-ToiletResourceChain.png> | Which resource references which, for a real EA object |
| Normal-map channel layout: <https://simswiki.info/wiki.php?title=Tutorials:Creating_Bumpmaps_(Normal_Maps)_for_TS3_-_Main> | DXT5 normal-map channel packing |

The wiki pages are short; most precise detail below comes from the s3pi code, cross-checked against s3py
where both implement a format.

---

## 1. RCOL container (resource types MODL 0x01661233, MLOD 0x01D10F34, MATD 0x01D0E75D, MTST 0x02019972, VPXY 0x736884F1, LITE 0x03B4C61D, FTPT 0xD382BF57, RSLT 0xD3044521, GEOM 0x015A1849, ...)

An RCOL ("Resource COLlection", the "Scene" container) holds one or more **chunks** (also called blocks). Each chunk
has its own 4CC tag and RCOL type id. The resource's DBPF type matches the type of its first chunk.

### 1.1 Header [code+wiki]

```
u32 version            // s3py writes 3; treat as informational
i32 publicChunks       // number of "public" chunks (they come first in the chunk list)
u32 index3             // s3pi "unused"; s3py: count of "External" (0x2) references, asserts 0
i32 externalCount      // number of external resource keys ("Delayed" references)
i32 internalCount      // number of chunks in this RCOL
TGI_ITG chunkKeys[internalCount]      // per chunk: u64 instance, u32 type, u32 group
TGI_ITG externalKeys[externalCount]   // referenced resources (DDS, TXTC, MLOD, ...): u64 I, u32 T, u32 G
struct { u32 position; u32 size; } chunkIndex[internalCount]   // position is absolute from resource start
// chunk data follows; s3pi pads each chunk start to a 4-byte boundary when writing
```

Header size is `0x14 + 16*internalCount + 16*externalCount + 8*internalCount`.

**Quirk [code]:** s3pi ignores the position/size table when `internalCount == 1` and instead uses
`position = 0x2C + externalCount*16; size = resourceLength - position` (0x2C = 0x14 header + 16-byte chunk key +
8-byte index entry). If the single chunk key has `type == 0`, s3pi reads the chunk's first 4 bytes as a tag and maps
it back to a type. GEOM resources are the main case: they declare a null chunk key `00000000:00000000:0000000000000000`
[wiki]. A reader should do the same: for a single-chunk RCOL, derive the chunk extent from the header length and
identify the chunk by its tag.

Chunk index `i < publicChunks` is "public"; `i >= publicChunks` is "private". In EA MODL resources the MODL chunk is
public and the MLOD/VRTF/VBUF/IBUF/MATD/MTST/SKIN chunks are private (s3py writes MODL's LOD references with the
PUBLIC flag and everything else PRIVATE).

### 1.2 ChunkReference (32-bit "RCOL reference") [code+wiki]

Used by every intra-RCOL pointer (MLOD mesh -> MATD/VRTF/VBUF/IBUF/SKIN, MODL -> MLOD, MTST -> MATD, MATD texture
-> external key, VBUF -> swizzle info).

```
raw == 0                      -> null reference
type  = raw >> 28             // top 4 bits
index = (raw & 0x0FFFFFFF) - 1  // 1-based on disk
type 0 Public  -> chunks[index]
type 1 Private -> chunks[index + publicChunks]
type 2 External-> unused (never seen; s3pi/s3py throw)
type 3 Delayed -> externalKeys[index]   // another resource, looked up by key in the package set
```

Examples: `0x10000003` = private chunk #2 (`chunks[publicChunks + 2]`); `0x30000001` = `externalKeys[0]`.
Private indices count from the first private chunk. s3py encodes and decodes this way, and so does s3pi's
`GetKey`/`GetBlock`. (s3pi's `CreateReferenceHelper` writer encodes the absolute list position instead, which looks
like a bug that only matters when writing. A reader can ignore it.) **Decode with the table above.**

### 1.3 Chunk type table [code: `RCOLResources.txt`; wiki]

| Type | Tag | Stand-alone resource? | Notes |
|---|---|---|---|
| 0x01661233 | MODL | yes | object model, LOD table |
| 0x01D10F34 | MLOD | yes (and embedded in MODL) | mesh groups |
| 0x01D0E723 | VRTF | no | vertex declaration |
| 0x01D0E6FB | VBUF | no | vertex buffer |
| 0x0229684B | VBUF | no | shadow-mesh vertex buffer, usually no VRTF |
| 0x01D0E70F | IBUF | no | index buffer |
| 0x0229684F | IBUF | no | shadow-mesh index buffer |
| 0x00000000 | (none) | no | VBUF swizzle info (s3pi calls it VBSI, s3pi type 0xFFFFFFFF internally) |
| 0x01D0E75D | MATD | yes | material |
| 0x02019972 | MTST | yes | material set (state-switchable materials) |
| 0x01D0E76B | SKIN | no | inverse bind poses for skinned MLOD meshes |
| 0x015A1849 | GEOM | yes | CAS/sim mesh (non-standard RCOL, see section 6) |
| 0x736884F1 | VPXY | yes | "visual proxy": links an object's model parts |
| 0xD382BF57 | FTPT | yes | footprint |
| 0x03B4C61D | LITE | yes | light sources/occluders |
| 0xD3044521 | RSLT | yes | slot definitions |
| 0x0355E0A6 | BOND | yes | slot adjusts |
| 0x021D7E8C | TREE | yes | SpeedTree data |
| 0x63A33EA7 | ANIM | yes | animated texture |

Every chunk except the swizzle-info block starts with its 4CC tag stored as a little-endian u32 (so the bytes read
`'M','L','O','D'`), followed by a `u32 version`.

---

## 2. MODL chunk (0x01661233) [code+wiki]

```
u32  tag = 'MODL'
u32  version              // 256 (0x100) or 258 (0x102) seen
i32  lodCount
f32  boundsMin[3], boundsMax[3]           // whole object, model space
if version >= 258:                         // seen on large community lots (City Hall, School)
    i32 extraBoundsCount
    { f32 min[3]; f32 max[3]; } extraBounds[extraBoundsCount]
    u32 fadeType
    f32 customFadeDistance
LODEntry lods[lodCount]:
    u32 modelLod          // ChunkReference -> MLOD chunk in this RCOL (private) or an external MLOD resource (delayed, 0x3...)
    u32 lodInfoFlags      // 0x1 Portal, 0x2 Door
    u32 lodId             // see below (s3py reads it as u16 id + u16 isShadow)
    f32 minZValue         // default -FLT_MAX ("min float32") -- LOD switch range [unknown units]
    f32 maxZValue         // default +FLT_MAX
```

LOD ids: `0x00000000` HighDetail, `0x00000001` MediumDetail, `0x00000002` LowDetail, `0x00010000`
HighDetailShadow, `0x00010001` MediumDetailShadow, `0x00010002` LowDetailShadow. For rendering, pick the
`HighDetail` entry and resolve its ChunkReference; shadow LODs carry shadow-only meshes.

Observed in the Expensive Toilet chain: the MODL resource (group 0x00000001) references three separate MLOD
resources with the same instance and groups 0x00000000, 0x00010000, 0x00010001. Do not rely on the group pattern;
follow the delayed references through the MODL's external key table.

Mesh-space coordinates are Y-up (the wiki GEOM example swaps y/z only to suit a Z-up tool). [inferred from tools]

---

## 3. MLOD chunk (0x01D10F34) [code+wiki]

```
u32 tag = 'MLOD'
u32 version               // 0x201, or 0x202 (adds parentName + mirrorPlane per mesh)
i32 meshCount
Mesh meshes[meshCount]:
    u32 meshSize          // bytes of this mesh record after this field (validate!)
    u32 nameHash          // FNV32 of the group name
    u32 material          // ChunkReference -> MATD or MTST (resolve MTST to its default MATD)
    u32 vertexFormat      // ChunkReference -> VRTF (0 = none: use the default shadow formats, section 4.3)
    u32 vertexBuffer      // ChunkReference -> VBUF (0x01D0E6FB or 0x0229684B)
    u32 indexBuffer       // ChunkReference -> IBUF (0x01D0E70F or 0x0229684F)
    u32 primAndFlags      // primitiveType = low 8 bits; meshFlags = value >> 8
    u32 streamOffset      // BYTE offset of this mesh's first vertex inside the VBUF data
    i32 startVertex       // always 0 in practice (D3D BaseVertexIndex)
    i32 startIndex        // first INDEX (not byte, not triangle) in the IBUF index array
    i32 minVertexIndex    // 0 in practice
    i32 vertexCount       // vertices belonging to this mesh, starting at streamOffset
    i32 primitiveCount    // triangles for TriangleList
    f32 boundsMin[3], boundsMax[3]
    u32 skinController    // ChunkReference -> SKIN (0 = unskinned)
    i32 jointCount
    u32 jointNameHashes[jointCount]   // bone names used by this mesh (blend indices index into this/the SKIN)
    u32 scaleOffset       // ChunkReference -> special MATD holding UVScale/UVOffset/PosScale/PosOffset (see 4.6)
    i32 geometryStateCount
    GeometryState states[geometryStateCount]:
        u32 nameHash      // FNV32 of state name, e.g. 0x4A9A1FD1 "guitarOnly"
        i32 startIndex    // index into IBUF (wiki: "triangle start * 3")
        i32 minVertexIndex// first vertex of the state, relative to the mesh's streamOffset
        i32 vertexCount
        i32 primitiveCount
    if MLOD.version > 0x201:
        u32 parentNameHash      // always 0 seen
        f32 mirrorPlane[4]      // normal xyz + offset (used by mirror objects)
```

Primitive types: 0 PointList, 1 LineList, 2 LineStrip, **3 TriangleList** (the only one both parsers support and
the only one seen in practice), 4 TriangleFan, 5 TriangleStrip, 6 RectList (s3pi) / QuadList (wiki), 7 QuadList (s3pi) /
DisplayList (wiki), 8 DisplayList (s3pi). The numbering above 5 differs between s3pi and the wiki; treat anything but 3
as unsupported. [unknown which is right]

Mesh flags (`primAndFlags >> 8`): 0x01 BasinInterior, 0x02 HDExteriorLit, 0x04 PortalSide, 0x08 DropShadow,
0x10 ShadowCaster, 0x20 Foundation, 0x40 Pickable.

### 3.1 Assembling a mesh's triangles [code + inferred D3D9 mapping]

The fields map one-to-one onto Direct3D 9 `SetStreamSource(offset=streamOffset)` +
`DrawIndexedPrimitive(type, BaseVertexIndex=startVertex, MinVertexIndex, NumVertices=vertexCount, StartIndex=startIndex, PrimitiveCount)`.
Both s3pi and s3py implement it as:

```
stride   = vrtf.stride
vertices = for v in 0..vertexCount: decode(vbuf.data[streamOffset + v*stride ..][..stride])
indices  = ibuf.indices[startIndex .. startIndex + primitiveCount*3]   // already un-delta'd (section 4.5)
triangle k = (indices[3k], indices[3k+1], indices[3k+2])              // indices are relative to streamOffset
```

i.e. the final vertex for index `i` is `streamOffset/stride + startVertex + i`. Several meshes usually share one
VBUF/IBUF with different offsets. Front-face winding is D3D (clockwise) [inferred].

Geometry states (for state-dependent visibility, e.g. sprinkler dome): vertices from
`streamOffset + state.minVertexIndex*stride`, `state.vertexCount` of them; indices
`ibuf[state.startIndex .. + state.primitiveCount*3]`, each minus `state.minVertexIndex` [code]. Game semantics
[wiki]: a state hides everything in that mesh group outside its ranges; a state name that matches no entry shows the
whole group; a "zero" state (all counts 0) hides the group.

The base range is every state's geometry together, so a renderer that ignores states draws a chess table with
pieces from all of its games on the board, or a dish full, half eaten and empty at once [observed]. State names are
FNV-32 hashes of names the game scripts set, found in the script assemblies' strings: `boardEmpty`/`boardSet`
(chess), `base`/`halfFull`/`full` (bookshelves), `tableClothOff`/`tableClothOn`, `basketOpen`/`basketClosed`,
`largePainting`/`smallPainting`, `guitarOnly`, `garbageEmpty`, `indicatorOff`, `unlocked`, `empty`/`full`, and
`thumbnail` for the catalogue picture; food states come from the recipe table (`foodServeSpaghetti#full`). Here a
model with alternative states is drawn in the one it rests in (`boardSet`, `tableClothOff`, `basketClosed`, `full`;
bookshelves without a `full` state whole), else its fullest, never `thumbnail` [ours]. 373 base-game catalogue models
have states.

### 3.2 SKIN chunk (0x01D0E76B) [code+wiki]

```
u32 tag='SKIN'; u32 version (1)
i32 boneCount
u32 boneNameHash[boneCount]
f32 inverseBindPose[boneCount][3][4]   // 3 rows x 4 columns, row-major; column 3 = translation
```

s3pi maps the 12 floats `m00 m01 m02 m03 m10 ... m23` to Right=(m00,m10,m20), Up=(m01,m11,m21),
Back=(m02,m12,m22), Translate=(m03,m13,m23). SKIN chunks occur only on skinned/animated objects (doors, cars, some furniture) [inferred].

### 3.3 Objects with moving parts in practice [verified, this project]

- An MLOD mesh's `jointReferences` (after the skin-controller reference) are FNV-32 hashes of
  rig bone names; VRTF usage 3 (BlendIndex, 4 bytes) indexes that list, usage 4 (BlendWeight,
  ColorUByte4 stored B,G,R,A like the normals, or UByte4N) weights them. The value fridge's
  door mesh is skinned to `_bind_0_door_W`, the cabinet to `transformBone`, rigidly (one bone
  per vertex at weight 255).
- The rig is the VPXY's 0x8EAF13DE entry (the same RIG format as a Sim's). Inverse bind poses
  can be rebuilt from the rig's own rest pose (bones composed from the root); the SKIN chunk
  isn't needed.
- The object's animation is the `<sim clip stem>_<object actor>` clip of the Sim's clip
  (`a2o_fridge_openDoor_x` → `a2o_fridge_openDoor_fridge`: one track on `_bind_0_door_W`,
  rotating it ~100° about Y). Played in step with the Sim's clip; tracks are local to the bone's
  parent, as for Sims. Shower doors, dresser drawers, the mailbox flag, trash can lids, swing
  seats, toy box lids and beds' covers have them.

---

## 4. Vertex and index data: VRTF, VBUF, swizzle info, IBUF

### 4.1 VRTF chunk (0x01D0E723) [code+wiki]

```
u32 tag = 'VRTF'
u32 version            // 2
i32 stride             // bytes per vertex
i32 elementCount
u32 isExtendedFormat   // always 0 seen
Element elements[elementCount]:
    if !isExtendedFormat:  u8 usage; u8 usageIndex; u8 format; u8 offset     // packed D3DVERTEXELEMENT-like
    else:                  u32 usage; u32 usageIndex; u32 format; u32 offset // s3py only; s3pi ignores the flag
```

`offset` is the byte offset of the element inside one vertex. `usageIndex` increments for repeated usages (UV0, UV1, ...).

**Usage enum** (u8): 0 Position, 1 Normal, 2 UV, 3 BlendIndex, 4 BlendWeight, 5 Tangent, 6 Colour.

**Format enum** (u8). EA's own enum: it looks like D3DDECLTYPE but values 4/5 and 13/14 are swapped relative to D3D9:

| Id | Name | Bytes | Typical usage | Decode (section 4.2) |
|---|---|---|---|---|
| 0x00 | Float1 | 4 | - | f32 |
| 0x01 | Float2 | 8 | UV | 2 x f32 |
| 0x02 | Float3 | 12 | Position | 3 x f32 |
| 0x03 | Float4 | 16 | UV | 4 x f32 |
| 0x04 | UByte4 | 4 | BlendIndex | 4 raw bytes |
| 0x05 | ColorUByte4 | 4 | Normal, Tangent, BlendWeight, Colour | D3DCOLOR-style packed bytes |
| 0x06 | Short2 | 4 | UV | 2 x i16 x uvScale |
| 0x07 | Short4 | 8 | Position (sun-shadow), UV (drop-shadow) | 3 x i16 / u16 scalar |
| 0x08 | UByte4N | 4 | - | unimplemented in both parsers |
| 0x09 | Short2N | 4 | - | unimplemented |
| 0x0A | Short4N | 8 | - | unimplemented |
| 0x0B | UShort2N | 4 | - | unimplemented |
| 0x0C | UShort4N | 8 | Position (drop-shadow) | 3 x **i16** / u16 scalar |
| 0x0D | Dec3N | 4 | - | unimplemented |
| 0x0E | UDec3N | 4 | - | unimplemented |
| 0x0F | Float16_2 | 4 | - | unimplemented |
| 0x10 | Float16_4 | 8 | - | unimplemented |

The formats marked "unimplemented" are not decoded by s3pi or s3py and have not been reported in shipped object
meshes. Implement them with standard D3D semantics if you meet them [inferred], and log their first occurrence.

### 4.2 Element decoding [code: s3pi `VBUF.ReadFloatData/ReadUVData`, s3py `read_element`]

Let `e` be the element's bytes at `vertexStart + offset`.

**Position**
* `Float3`: `(f32, f32, f32)`.
* `Short4`: `x,y,z = i16(e[0..6])`, `s = u16(e[6..8])`, `if s == 0 { s = 32767 }`, `pos = xyz / s`. (Wiki: "fourth
  word is an unsigned scalar, or 32767 if zero".)
* `UShort4N`: the same layout. **The three components are read as signed i16 despite the name.** `s = u16(e[6..8])`;
  if `s == 0`, the wiki and s3pi's *writer* use **512**, while s3pi's and s3py's *readers* use 511. Use 512. The
  writer comment says "changed to fixed 512 as LoveseatDanishModern had problems". The difference is a 0.2% scale
  error. Check it against the MLOD mesh bounds (section 4.6).

**Normal / Tangent: `ColorUByte4`** (byte order follows D3DCOLOR: memory bytes B,G,R,A map to x=e[2], y=e[1], z=e[0], w=e[3])
```
for i in 0..3: c = e[2 - i]; n[i] = (c == 0) ? -1.0 : ((c + 1) / 128.0 - 1.0)   // s3pi and s3py, identical
w (handedness / bitangent sign) = e[3]: 0 -> -1.0, 127 -> 0.0 ("no determinant"), 255 -> +1.0
```
This is about `(c - 127) / 128`. Plain D3D semantics would be `c/255*2 - 1` [inferred]. The two differ by under 1%,
so renormalise after decoding. The wiki's text description of this format is self-contradictory (see the
discussion on its talk page); follow the code above.

**BlendWeight: `ColorUByte4`**: `w[i] = e[map[i]] / 255` with `map = {0:2, 1:1, 2:0, 3:3}` (also D3DCOLOR swizzle).
**BlendIndex: `UByte4`**: 4 raw bytes `idx[i] = e[i]`, no swizzle. `idx[i]` pairs with `w[i]`. Indices select bones
from the mesh's `jointNameHashes` / SKIN bone list [inferred; neither parser says which, and they normally match].

**Colour: `ColorUByte4`**: s3pi/s3py return `e[i]/255` in memory order (no swizzle). Under D3DCOLOR semantics the
memory order is B,G,R,A, so RGBA = `(e[2], e[1], e[0], e[3]) / 255` [inferred, unverified].

**UV**
* `Float2` / `Float4`: floats.
* `Short2`: `uv = i16 * uvScale`. `uvScale` comes from the mesh material's MATD parameter **`UVScales` (0x420520E9,
  Float3)**, element `[n]` for the n-th UV channel. s3py indexes by `usageIndex`. s3pi indexes by the UV element's
  position among UV elements and falls back to element `[0]` when `uvScales[n] == 0`. If the MATD has no
  `UVScales`, use `1/32767`. For an MTST material use the default state's MATD. (Wiki: "multiply by the MATD
  ShaderData entry for UVScales, element 0. If no such value exists, divide by 32767.")
* `Short4` (only in drop-shadow meshes, which s3pi calls `Short4_DropShadow`): `u = s0/32767`, `v = s1/32767`,
  `s2` must be 0, third value `= s3/511` (meaning unknown).

Non-UV `Short2`: s3pi divides by 32767 and s3py by 65535. Not seen in practice.

V axis: Sims 3 UVs are D3D-convention (v = 0 at the top of the texture) [inferred from D3D9 origin]. Do not flip
when sampling DDS data uploaded top-row-first.

### 4.3 Missing VRTF (shadow meshes) [code+wiki]

If a mesh's `vertexFormat` reference is 0 (typical for VBUF type 0x0229684B):
* `ShadowCaster` flag (0x10) set: "sun shadow" format = `{Position, Short4, offset 0}`, stride 8.
* otherwise: "drop shadow" format = `{Position, UShort4N, offset 0}`, `{UV0, Short4 (drop-shadow variant), offset 8}`, stride 16.

The wiki's phrasing: "If VRTF is not present, then it is a 16 byte vertex size, or 8 byte if ...". A renderer can
skip shadow meshes: those with VBUF 0x0229684B, those whose material shader is DropShadow (0xC09C7582), and those
whose MODL LOD id has bit 0x10000 set.

### 4.4 VBUF chunk (0x01D0E6FB; shadow variant 0x0229684B) [code+wiki]

```
u32 tag = 'VBUF'
u32 version          // 0x101
u32 flags            // s3pi enum: 0x1 Dynamic, 0x2 DifferencedVertices, 0x4 Collapsed. Wiki: always 0; s3py asserts 0.
u32 swizzleInfo      // ChunkReference -> swizzle-info block (untagged, chunk type 0x00000000)
u8  data[chunkSize - 16]   // raw interleaved vertices, layout per VRTF; meshes index it via streamOffset
```

Neither parser handles `DifferencedVertices` or `Collapsed`. Fail loudly if `flags != 0`. [unknown encoding]

**Swizzle info block (no tag, no version)** [code+wiki]:
```
i32 segmentCount                 // one per mesh using the VBUF
Segment { u32 vertexSize; u32 vertexCount; u32 byteOffset; u32 command[vertexSize/4]; }
command: 0 None, 1 Swizzle32 (reverse the 4 bytes of this dword), 2 Swizzle16x2 (byte-swap each of two u16 halves)
```
There is one command per 4-byte word of the vertex: Float* gives one `Swizzle32` per float, `UByte4`/`ColorUByte4`
gives one `Swizzle32`, and `Short2` gives one `Swizzle16x2` (`Short4` gives two). This is endian-conversion
metadata, apparently for big-endian console builds [inferred]. **PC data is already little-endian. Do not apply the
commands.** Neither PC tool applies them.

### 4.5 IBUF chunk (0x01D0E70F; shadow variant 0x0229684F) [code+wiki]

```
u32 tag = 'IBUF'
u32 version            // 0x100
u32 flags              // 0x1 DifferencedIndices, 0x2 Uses32BitIndices, 0x4 IsDisplayList
u32 displayListUsage   // wiki: "always zero (pipeline bug); if not zero, safe to ignore"
data: if flags & 0x2: i32[ (chunkSize-16)/4 ] else i16[ (chunkSize-16)/2 ]
```

Decoding (s3pi `IBUF.Parse`, s3py identical):
```
last = 0
for each raw value r (i16 sign-extended to i32, or i32):
    cur = (flags & DifferencedIndices) ? r + last : r
    out.push(cur); last = cur
```
* **The delta chain runs over the whole buffer from index 0, not per mesh.** Decode the entire IBUF once, then slice
  with each mesh's `startIndex`. (Wiki: "if compressed, start at zero and add or subtract each signed value for the
  entire buffer before using the offset".)
* Deltas are **signed** 16-bit values. If a 16-bit buffer addresses more than 32767 vertices, a delta can exceed the
  i16 range. EA's encoder then presumably wraps, so accumulate modulo 2^16 for 16-bit buffers
  (`cur = (last + r) & 0xFFFF`) [inferred]. This matches s3pi whenever s3pi's result is valid.
* `IsDisplayList` (0x4) has never been documented as seen on PC [unknown].

### 4.6 Scale/offset MATD and validation [wiki + unknown]

Each MLOD mesh has a `scaleOffset` ChunkReference. The wiki describes its target as a "special MATD that only has
UVScale/UVOffset/PosScale/PosOffset and no ShaderName". The parameters are `UVScale` 0x159BA53E, `UVOffset`
0x57582869, `PosScale` 0x487648E5 and `PosOffset` 0x790EBF2C, all Float4. **Neither s3pi nor s3py applies these
when decoding vertices**, and nothing documents the formula. The likely meaning is `pos = raw*PosScale.xyz +
PosOffset.xyz` and `uv = raw*UVScale.xy + UVOffset.xy` for quantised formats [inferred]. Recommended practice:

1. Decode positions with the rules in 4.2 and compute their AABB.
2. Compare it with the mesh's own `bounds` in the MLOD record. The geometry-state ranges are included in the mesh
   vertices. A match within ~1e-3 confirms the decoding.
3. If they disagree and `scaleOffset != 0`, try applying PosScale/PosOffset and keep whichever matches. Log the
   object so the rule can be pinned down from real data.

The same check (MLOD bounds and MODL bounds) also settles the 511-versus-512 `UShort4N` scalar question.

---

## 5. Materials: MATD, MTNF/MTRL parameter blocks, MTST

### 5.1 MATD chunk (0x01D0E75D) [code+wiki]

MATD appears as a private chunk inside MODL/MLOD RCOLs, and also as a stand-alone RCOL resource.

```
u32 tag = 'MATD'
u32 version              // 0x103 in practice; < 0x103 uses an MTRL block instead of MTNF
u32 materialNameHash     // FNV32 of material name, or 0
u32 shaderNameHash       // FNV32 of shader name (table 5.4), or 0 (the scale/offset MATD has 0)
u32 blockLength          // bytes of the MTRL/MTNF block, measured from its tag to its end
if version < 0x103:
    MTRL block
else:
    u32 isVideoSurface      // bool (TV screens)
    u32 isPaintingSurface   // bool (easel/painting canvas)
    MTNF block
```

### 5.2 MTNF / MTRL parameter block [code+wiki]

```
blockStart = stream position of the tag          // ALL param offsets are relative to this
MTNF:  u32 tag='MTNF'; u32 unknown (0); u32 dataSize; i32 paramCount
MTRL:  u32 tag='MTRL'; u32 unknown (0); u16 unk2; u16 unk3; i32 paramCount    // no dataSize field
Param params[paramCount]:
    u32 nameHash          // FNV32 of the parameter name (table 5.3)
    u32 dataType          // 1 = Float, 2 = Int, 4 = Texture
    i32 sizeDwords        // size of the value in 32-bit words (see below)
    u32 offset            // value location = blockStart + offset
// data area: values packed back to back, total = sum(sizeDwords)*4 = dataSize (MTNF)
```

Value decoding by `(dataType, sizeDwords)`. s3pi accepts only these combinations and throws on anything else:

| dataType | sizeDwords | Value |
|---|---|---|
| 1 Float | 1, 2, 3, 4 | `f32[size]` (scalar, vec2, vec3, vec4) |
| 2 Int | 1 | `i32` (wiki also lists `Int[]` for size > 1) |
| 4 Texture | 4 | "TextureRef": `u32 ref` then 12 zero bytes. In MATD, `ref` is a **ChunkReference** (section 1.2), in practice Delayed (`0x3xxxxxxx`) into the containing RCOL's **external key list**. In GEOM it is a plain `i32` index into the GEOM's own TGI list (section 6). |
| 4 Texture | 5 | "TextureKey": inline TGI in **ITG** order (`u64 instance, u32 type, u32 group`) then 4 zero bytes. Seen only on impostor parameters. |

Use the `offset` field rather than assuming the values are sequential (s3pi seeks to it; s3py reads sequentially
and works only because EA writes values in header order). The wiki's description of the MATD `blockLength` field,
"16+(16*count)+(4*parmcount)", is consistent with this layout.

**What a texture reference can point to** [wiki]: DDS `0x00B2D882`, TXTC `0x033A1435` (texture compositor, section
8.2) or ANIM `0x63A33EA7` (animated texture). In the Expensive Toilet chain, the MODL and MLOD resources reference
both TXTC resources and plain DDS resources. Pattern-recoloured surfaces (diffuse, and usually specular) go through a
TXTC; the other maps point straight at a DDS.

Resolving a material texture end to end:
```
ref  = param.value as u32                       // e.g. 0x30000004
key  = rcol.externalKeys[(ref & 0x0FFFFFFF) - 1] // when (ref >> 28) == 3
tex  = package_index.lookup(key)                // DDS -> decode; TXTC -> run compositor; ANIM -> first frame
```

### 5.3 Parameter names (FNV32) [code: s3pi `ShaderData.cs` `FieldType` enum; types from its comments]

I recomputed every hash in the s3pi enum as FNV32 of the lower-cased name, and all 145 match. The name strings are
therefore the real EA parameter names, matched case-insensitively.

Most important for a first renderer:

| Name | Hash | Type | Meaning / use |
|---|---|---|---|
| DiffuseMap | 0x6CC0FD85 | Texture | base colour (often a TXTC) |
| NormalMap | 0x6E56548A | Texture | tangent-space normal map (DXT5, see 8.1) |
| SpecularMap | 0xAD528A60 | Texture | specular colour in RGB, shininess/intensity in alpha [wiki DDS page] |
| AlphaMap | 0xC3FAAC4F | Texture | separate alpha/cutout |
| MultiplyMap | 0xCD869A45 | Texture | multiplier (greyscale shading) |
| EmissionMap | 0xF303D152 | Texture | self-illumination |
| SelfIlluminationMap | 0x6E067554 | Texture | |
| AmbientOcclusionMap | 0xB01CBA60 | Texture | |
| DetailMap | 0x9205DAA8 | Texture | |
| DirtOverlay | 0x48372E62 | Texture | dirty-state overlay |
| RoomLightMap | 0xE7CA9166 | Texture | |
| DropShadowAtlas | 0x22AD8507 | Texture | used by DropShadow shader meshes |
| UVScales | 0x420520E9 | Float3 | **dequantisation factor for Short2 UVs** (section 4.2) |
| UVScale / UVOffset | 0x159BA53E / 0x57582869 | Float4 | in the per-mesh scale/offset MATD (4.6) |
| PosScale / PosOffset | 0x487648E5 / 0x790EBF2C | Float4 | in the per-mesh scale/offset MATD (4.6) |
| DiffuseUVScale / NormalUVScale / SpecularUVScale / DetailUVScale | 0x2D4E507E / 0xBA2D1AB9 / 0xF12E27C3 / 0xCD985A0B | Float2 | per-map UV tiling |
| DiffuseMapUVChannel / SpecularMapUVChannel | 0xC45A5F41 / 0xCB053686 | Float | which UV set to sample |
| DiffuseUVSelector / SpecularUVSelector / NormalMapUVSelector | 0x91EEBAFF / 0xB63546AC / 0x415368B4 | Float3 | UV-set selectors [unknown encoding] |
| Diffuse | 0x637DAA05 | Float3 | diffuse colour tint |
| Specular | 0x2CE11842 | Float3 | specular colour |
| Shininess | 0xF755F7FF | Float | specular power |
| Transparency | 0x05D22FD3 | Float | |
| AlphaMaskThreshold | 0xE77A2B60 | Float | alpha-test cutoff |
| NormalMapScale | 0x3C45E334 | Float | |
| EmissiveLightMultiplier | 0x8EF71C85 | Float | |

Full list from s3pi, grouped by value type (`(dep)` = marked deprecated in s3pi):

* **Texture** (20): SparkleCube 0x1D90C086, DropShadowAtlas 0x22AD8507, DirtOverlay 0x48372E62, OverlayTexture 0x4DC0C8BC, JetTexture 0x52CE211B, ColorRamp 0x581835D6, DiffuseMap 0x6CC0FD85, SelfIlluminationMap 0x6E067554, NormalMap 0x6E56548A, HaloRamp 0x84F6E0FB, DetailMap 0x9205DAA8, SpecularMap 0xAD528A60, AmbientOcclusionMap 0xB01CBA60, AlphaMap 0xC3FAAC4F, MultiplyMap 0xCD869A45, SpecCompositeTexture 0xD652FADE, NoiseMap 0xE19FD579, RoomLightMap 0xE7CA9166, EmissionMap 0xF303D152, RevealMap 0xF3F22AC4
* **TextureKey** (4): ImposterTextureAOandSI 0x15C9D298, ImpostorDetailTexture 0x56E1C6B2, ImposterTexture 0xBDCF71C5, ImposterTextureWater 0xBF3FB9FA
* **Float** (72): AlignAcrossDirection 0x01885886, DimmingCenterHeight 0x01ADACE0, Transparency 0x05D22FD3, BlendSourceMode 0x0995E96C, SharpSpecControl 0x11483F01, RotateSpeedRadsSec 0x16BF7A44, AlignToDirection 0x17B78AF6, DropShadowStrength 0x1B1AB4D5, ContourSmoothing 0x1E27DCCD, reflectivity(dep) 0x29BCDD1F, BlendOperation 0x2D13B939, RotationSpeed 0x32003AD4, DimmingRadius 0x32DFA298, IsGenericBox 0x347C9E07, IsSolidObject 0x3BBF99CF, NormalMapScale 0x3C45E334, NoAutomaticDaylightDimming 0x3CB5FA70, FramesPerSecond 0x406ADE00, BloomFactor 0x4168508B, EmissiveBloomMultiplier 0x490E6EB4, IsObject 0x4C12ECE8, IsPartition 0x5250023D, RippleSpeed 0x52DEC070, UseLampColor 0x56B220CD, TextureSpeedScale 0x583DF357, NoiseMapScale 0x5E86DEA1, AutoRainbow 0x5F7800EA, DebouncePower 0x656025DF, SpeedStretchFactor 0x66479028, WindSpeed 0x66E9B6BC, DaytimeOnly 0x6BB389BC, FramesRandomStartFactor 0x7211F24F, DeflectionThreshold 0x7D621D61, LifetimeSeconds 0x84212733, NormalBumpScale 0x88C64AE2, DeformerOffset 0x8BDF4746, EdgeDarkening 0x8C27D8C9, OverrideFactor 0x8E35CCC0, EmissiveLightMultiplier 0x8EF71C85, SharpSpecThreshold 0x903BE4D3, RugSort 0x906997A9, Layer2Shift 0x92692CB2, SpecStyle 0x9554D40F, FadeDistance 0x957210EA, BlendDestMode 0x9BDECB37, LightingEnabled 0xA15E4594, OverrideSpeed 0xA3D6342E, VisibleOnlyAtNight 0xAC5D0A82, UseDiffuseForAlphaTest 0xB597FA7F, SparkleSpeed 0xBA13921E, WindStrength 0xBC4A2544, HaloBlur 0xC3AD4F50, RefractionDistortionScale 0xC3C472A1, DiffuseMapUVChannel 0xC45A5F41, SpecularMapUVChannel 0xCB053686, ParticleCount 0xCC31B828, RippleDistanceScale 0xCCB35B98, DivetScale 0xCE8C8311, ForceAmount 0xD4D51D02, AnimSpeed 0xD600CB63, BackFaceDiffuseContribution 0xD641A1B1, BounceAmountMeters 0xD8542D8B, IsFloor 0xD9C05335, index_of_refraction(dep) 0xDAA9532D, BloomScale 0xE29BA4AC, AlphaMaskThreshold 0xE77A2B60, LightingDirectScale 0xEF270EE4, AlwaysOn 0xF019641D, Shininess 0xF755F7FF, FresnelOffset 0xFB66A8CB, BouncePower 0xFBA6B898, ShadowAlphaTest 0xFEB1F9CB
* **Float2** (13): DiffuseUVScale 0x2D4E507E, RippleHeights 0x6A07D7E1, CutoutValidHeights 0x6D43D7B7, UVTiling 0x773CAB85, SizeScaleEnd 0x891A3133, StretchRect 0x8D38D12E, SizeScaleStart 0x9A6C2EC8, WaterScrollSpeedLayer2 0xAFA11435, WaterScrollSpeedLayer1 0xAFA11436, NormalUVScale 0xBA2D1AB9, DetailUVScale 0xCD985A0B, SpecularUVScale 0xF12E27C3, UVScrollSpeed 0xF2EEA6EC
* **Float3** (25): Ambient(dep) 0x04A5DAA3, OverrideDirection 0x0C12DED8, OverrideVelocity 0x14677578, CounterMatrixRow1 0x1EF8655D, CounterMatrixRow2 0x1EF8655E, ForceDirection 0x29881F55, Specular 0x2CE11842, HaloLowColor 0x2EB8E8D4, Emission(dep) 0x3BD441A0, NormalMapUVSelector 0x415368B4, UVScales 0x420520E9, LightMapScale 0x4F7DCB9B, Diffuse 0x637DAA05, Reflective(dep) 0x73C9923E, AmbientUVSelector 0x797F8E81, HighlightColor 0x90F8DCF0, DiffuseUVSelector 0x91EEBAFF, Transparent(dep) 0x988403F9, VertexColorScale 0xA2FD73CA, SpecularUVSelector 0xB63546AC, EmissionMapUVSelector 0xBC823DDC, HaloHighColor 0xD4043258, RootColor 0xE90599F6, ForceVector 0xEBA4727B, PositionTweak 0xEF36D180
* **Float4** (8): TimelineLength 0x0081AE98, UVScale 0x159BA53E, FrameData 0x1E5B2324, AnimDir 0x3F89C2EF, PosScale 0x487648E5, Births 0x568E0367, UVOffset 0x57582869, PosOffset 0x790EBF2C
* **Int** (3): AverageColor 0x449A3A67, MaskWidth 0x707F712F, MaskHeight 0x849CDADC

Parameters with other hashes will occur. Keep unknown ones as raw `(hash, type, words)` rather than failing.

### 5.4 Shader names (MATD `shaderNameHash`, GEOM "EmbeddedID") [code: s3pi `ShaderType`]

All of these are FNV32 of the name, except **PhongAlpha 0xFC5FC212**. Its real name string is unknown: "phongalpha"
hashes to 0xD9429C93 and "phong_alpha" to 0x032707F4. The id itself is what s3pi shipped, so the value is
presumably right and only the label is uncertain.

Subtractive 0x0B272CC5, Instanced 0x0CB82EB8, FullBright 0x14FA335E, PreviewWallsAndFloors 0x213D6300, ShadowMap 0x21FE207D, GlassForRabbitHoles 0x265FFAA1, ImpostorWater 0x277CF8EB, Rug 0x2A72B9A1, Trampoline 0x3939E094, Foliage 0x4549E22E, ParticleAnim 0x460E93F4, SolidPhong 0x47C6638C, GlassForObjects 0x492ECA7C, Stairs 0x4CE2F497, OutdoorProp 0x4D26BEC0, GlassForFences 0x52986C62, SimSkin 0x548394B9, Additive 0x5AF16731, SimGlass 0x5EDA9CDE, Fence 0x67107FE8, LotImposter 0x68601DE3, Blueprint 0x6864A45E, BasinWater 0x6AAD2AD5, StandingWater 0x70FDE012, BuildingWindow 0x7B036C01, Roof 0x7BD05F63, GlassForPortals 0x81DD204D, GlassForObjectsTranslucent 0x849CF021, SimHair 0x84FD7152, Landmark 0x8A60B969, RabbitHoleHighDetail 0x8D346BBC, CASRoom 0x94B9A835, SimEyelashes 0x9D9DA161, Gemstones 0xA063C1D0, Counters 0xA4172F62, FlatMirror 0xA68D9E29, Painting 0xAA495821, RabbitHoleMediumDetail 0xAEDE7105, Phong 0xB9105A6D, Floors 0xBC84D000, DropShadow 0xC09C7582, SimEyes 0xCF8A70B4, Plumbob 0xDEF16564, SculptureIce 0xE5D98507, PhongAlpha 0xFC5FC212, ParticleJet 0xFF5E6908

The shader programs are compiled into the game executable and are not documented. A reimplementation has to write
its own shaders per family. A workable first pass is Phong/SolidPhong/OutdoorProp/Counters/Stairs/Roof/Fence/Floors/
Rug/Painting = opaque lit; PhongAlpha/Glass* = alpha-blended; Foliage = alpha-tested; DropShadow/ShadowMap = skip
[inferred].

### 5.5 MTST chunk (0x02019972): material sets [code+wiki]

```
u32 tag = 'MTST'
u32 version                // 0x200
u32 defaultStateHash       // FNV32 of the default state name, normally 0x2EA8FB98 "default"
u32 defaultMaterial        // ChunkReference -> MATD (or nested MTST)
i32 count
{ u32 material (ChunkReference); u32 stateNameHash; } entries[count]   // NOTE: reference first, then name
```

Known states: `default` 0x2EA8FB98, `dirty` 0xEEAB4327, `verydirty` 0x2E5DF9BB, `burnt` 0xC3867C32, `clogged`
0x257FB026, `carLightsOff` 0xE4AF52C1. The game swaps sets with
`ScriptCore.World.ObjectSetMaterial(objId, materialSetName, ...)`, where the name is FNV32-hashed [wiki]. An MLOD
mesh's `material` may point to an MTST. Use `defaultMaterial` (s3py follows `default_material` recursively until it
reaches a MATD). Inside a MODL RCOL these references are private (`0x1...`). The wiki says a stand-alone MTST
resource uses `0x3...` (external) references.

---

## 6. GEOM (0x015A1849): CAS / sim body mesh [code+wiki]

GEOM is wrapped in an RCOL container with **exactly one chunk**, usually with a null chunk key (see the 1.1 quirk).
It has its own TGI list ("key table") instead of using the RCOL external list, and all GEOM indices refer to that
list. s3pi's `GeometryResource` checks `ChunkEntries.Count == 1` and parses the chunk as GEOM.

```
u32 tag = 'GEOM'
u32 version                // 5 (s3pi rejects anything else when Checking). Later EPs: see note below.
u32 tgiOffset              // relative: TGI list starts at (position after this field) + tgiOffset
u32 tgiSize                // bytes of the TGI list = 4 + 16*count
u32 embeddedShaderHash     // 0 = no material; else shader FNV32: SimSkin 0x548394B9 (morphskin/morphskincloth), SimEyes 0xCF8A70B4, SimHair, SimGlass, SimEyelashes...
if embeddedShaderHash != 0:
    u32 mtnfSize           // bytes of the following MTNF block
    MTNF block             // as section 5.2, but texture params (type 4, size 4) hold an i32 index into THIS GEOM's TGI list
u32 mergeGroup
u32 sortOrder
i32 vertexCount
i32 formatCount
VertexFormat formats[formatCount]:
    u32 usage              // 1 Position, 2 Normal, 3 UV, 4 BoneAssignment, 5 Weights, 6 TangentNormal, 7 Color/TagVal, 10 VertexID
    u32 dataType           // 1 float, 2 byte, 3 ARGB colour, 4 u32
    u8  bytesPerElement
Vertex vertices[vertexCount]: for each format in order (no padding, interleaved, offset = running sum):
    Position        f32[3]
    Normal          f32[3]
    UV              f32[2]           (Pets added animal meshes with several UV elements)
    BoneAssignment  u8[4]            // each byte indexes boneHashes[] below
    Weights         f32[4]           // pairs with BoneAssignment[i]
    TangentNormal   f32[3]
    Color (TagVal)  u32 / u8[4]      // "colour channel data", ARGB
    VertexID        u32
i32 itemCount              // always 1 in practice (s3pi asserts 1)
u8  bytesPerFacePoint[itemCount]  // always 2 (s3pi asserts 2)
i32 facePointCount         // number of u16 indices (= 3 * triangles)
u16 indices[facePointCount]       // triangle list
i32 skinControllerIndex    // index into TGI list -> skin controller resource (type 0x00AE6C67)
i32 boneCount
u32 boneHashes[boneCount]  // FNV32 of bone names used; BoneAssignment bytes index this
TGI list (at tgiOffset):   i32 count; { u32 type; u32 group; u64 instance; } [count]   // "TGI" order (key table)
```

s3pi validates the element sizes: Position 12, Normal 12, UV 8, BoneAssignment 4, Weights 16, Tangent 12,
Color 4, VertexID 4. The matching `dataType` values are 1,1,1,2,1,1,3,4.

**GEOM versions [unknown]:** s3pi and s3py implement only version 5, and the wiki documents only version 5. Sims 4
changed the format (for example GEOM v0x0C has byte weights), so do not borrow Sims 4 code. If Sims 3 data
contains any other version, log it and skip it.

**Skin controller (0x00AE6C67)** [code: s3py `BodySkinController`]: `u32 version; u32 n; n x 7-bit-length-prefixed
UTF-16BE bone-name strings; u32 n; n x 12 f32 bind-pose matrices`. s3py reads each matrix as [4][3] but writes it as
[3][4], so the matrix orientation is not reliably documented. Treat it as unknown and verify against the rig (`_RIG`
0x8EAF13DE).

GEOM is used for Sims and CAS parts (reached through CASP 0x034AEECB resources, outside this document). Build/buy
objects use MODL/MLOD.

---

## 7. From catalog object to model: OBJD -> OBJK -> VPXY -> MODL/MLOD (+ FTPT, LITE)

### 7.1 The reference chain [wiki: OBJK page + Expensive Toilet diagram; code]

```
OBJD 0x319E4F1D (catalog entry; instance = object id, e.g. 0x40B)
 ├─ TGI list[objkIndex] ──────────────► OBJK 0x02DC343F (often the same instance as the OBJD)
 │                                        └─ componentData "modelKey" (index into OBJK TGI list)
 │                                             ─► VPXY 0x736884F1 (group 0x00000001 in the example)
 │                                                 ├─ MODL 0x01661233 (same instance as the VPXY, group 1)
 │                                                 │    ├─ internal chunks: MODL, MLOD(s), VRTF, VBUF, IBUF, MATD/MTST, SKIN
 │                                                 │    └─ external keys: MLOD 0x01D10F34 (other LODs), DDS 0x00B2D882, TXTC 0x033A1435
 │                                                 ├─ FTPT 0xD382BF57   (footprint)
 │                                                 ├─ LITE 0x03B4C61D   (lights)
 │                                                 ├─ RSLT 0xD3044521   (slots)
 │                                                 └─ _RIG 0x8EAF13DE   (skeleton, for animated objects)
 │                                        └─ componentData "footprintKey" -> FTPT (only used when modelKey is not a VPXY)
 ├─ TGI list: preset/complate XML 0x0333406C, pattern DDS 0x00B2D882 (recolour presets, section 7.3)
 └─ TGI list: OBJD (diagonal/fallback/proxy), wall-cutout DDS, etc.
```

Recommended resolution algorithm for "render object X with its default look":

1. Find the OBJD by key `(0x319E4F1D, group, instance)`. Parse it as far as `objkIndex` (7.2), then read its TGI list
   at `tgiOffset`.
2. `objk = load(OBJD.tgi[objkIndex])`. Parse it (7.4) and find the component-data entry with key `"modelKey"`
   (control code 0x01 or 0x02). Its `i32` is an index into the OBJK TGI list, which gives the VPXY key.
3. Parse the VPXY (7.5). Take every TGI in its key list with type 0x01661233 (MODL). For an MLOD-only object
   (rare), take 0x01D10F34 instead. Take FTPT, LITE and RSLT keys from the same list. This type scan is more robust
   than interpreting the VPXY entry list, whose LOD semantics are only partly documented.
4. Parse the MODL resource as an RCOL (section 1). Find the public MODL chunk and pick its `HighDetail` LOD entry.
   Resolve its ChunkReference: a private reference is an MLOD chunk in the same RCOL; a delayed reference is an
   external MLOD resource, which is its own RCOL whose chunk references resolve against **its own** chunk and
   external lists.
5. For each MLOD mesh (skipping shadow meshes, 4.3): decode vertices and indices (section 3.1/4), resolve
   `material` (MTST -> default -> MATD), and read the MTNF parameters (5.2). Resolve texture params through the
   **external key list of the RCOL that contains that MATD**, then load DDS or run the TXTC compositor (8.2).

### 7.2 OBJD 0x319E4F1D layout [code: s3pi `ObjectCatalogResource.Parse`; wiki OBJD + Catalog Resource pages]

```
u32 version                    // 0x16..0x22+ seen; fields below are version-gated
u32 tgiOffset                  // key table: relative to the position after this field
u32 tgiSize
MaterialList materials         // i32 count; Material[count] (7.3) -- recolour presets
if version >= 0x16: STR7 instanceName
Common common                  // shared catalog block, below
u32 objkIndex                  // -> TGI list entry of the OBJK
u32 objectTypeFlags
if version >= 0x1A: u32 objectTypeFlags2
u32 wallPlacementFlags
u32 movementFlags
u32 wallCutoutTilesPerLevel
u32 levels
u8  wallCutoutCount; { f32 leftX, leftZ, rightX, rightZ; u32 levelOffset; u32 wallMaskDDSIndex; } [wallCutoutCount]
u8  isScriptEnabled
u32 diagonalOBJDIndex
u32 ambienceTypeHash
u32 roomCategoryFlags
u32 functionCategoryFlags
u64 functionSubCategoryFlags
if version >= 0x1C: u64 functionSubCategoryFlags2
u64 roomSubCategoryFlags
u32 buildCategoryFlags
u32 surfaceCutoutDDSIndex
if version >= 0x17:
    u32 floorCutoutDDSIndex; u32 floorCutoutLevelOffset; f32 floorCutoutBoundsLength
    if version >= 0x20: f32 floorCutoutBoundsWidth
        if version >= 0x21: f32 floorCutoutOffsetX; f32 floorCutoutOffsetZ
    if version >= 0x18: i32 n; u32 buildableShellDisplayStateHashes[n]
        if version >= 0x19: u32 levelBelowOBJDIndex
            if version >= 0x1B: u32 proxyOBJDIndex
                if version >= 0x1D: u32 blueprintXMLIndex
                    if version >= 0x1E: u32 blueprintIconIndex
                        if version >= 0x1F: f32 blueprintIconOffsetMinX, MinZ, MaxX, MaxZ
u32 slotPlacementFlags
STR7 surfaceType
STR7 sourceMaterial
u32 moodletGiven
i32 moodletScore
u32 topicCount                 // always 5 (s3pi "unknown21")
{ u32 topic; i32 rating; } topicRatings[5]   // s3pi reads exactly 5
u32 fallbackOBJDIndex
if version >= 0x22: u32 modularArchEndEastIndex, modularArchEndWestIndex, modularArchConnectIndex, modularArchSingleIndex  // VPXY/model indices
// (now at tgiOffset) key table:
i32 tgiCount; { u32 type; u32 group; u64 instance; } [tgiCount]   // "TGI" order
```

`Common` block (all catalog types) [code+wiki]:
```
u32 version; u64 nameGUID; u64 descGUID           // STBL keys for the localised name and description
STR7 name; STR7 desc                              // internal names
f32 price; f32 nicenessMultiplier; f32 crapScore
u8  buildBuyProductStatusFlags                    // 0x1 ShowInCatalog, 0x2 ProductForTesting, 0x4 InDevelopment, 0x8 Shipping, 0x10 Debug, 0x20 NewEntryScheme
u64 pngInstance                                   // thumbnail instance if non-zero (types 0x2E75C764..66)
u8  unknown7
f32 environmentScore
u32 fireType                                      // 0 DoesNotBurn, 1 Chars, 2 AshPile
u8  isStealable; u8 isReposessable
u32 uiSortPriority
if version >= 0x0D: u8 isPlaceableOnRoof
    if version >= 0x0E: u8 isVisibleInWorldBuilder
        if version >= 0x0F: u32 productNameHash
```

`STR7` = .NET `BinaryReader.ReadString` with UTF-16**BE**: a 7-bit-encoded **byte** length (LEB128-style, low 7
bits first, bit 7 = continue), then that many bytes of UTF-16BE. [code: `SevenBitString.cs`]

**Shortcut:** the TGI list sits at a known offset, but `objkIndex` comes after the variable-length material list,
instance name and Common block, so you have to parse (or skip) those.

### 7.3 OBJD material list (recolour presets / complate variables) [code+wiki]

```
i32 materialCount
Material:
    u8  materialType
    if materialType != 1: u32 unknown1
    u32 endOffset          // relative to the position after this field; points at 'unknown3' below
    u16 unknown2
    u32 tgiOffset; u32 tgiSize        // this material's own key table (relative, as usual)
    MaterialBlock block
    i32 n; TGI(TGI-order) keys[n]     // the key table
    u32 unknown3                      // wiki: "ID"
MaterialBlock:                        // recursive
    u8   complateXMLIndex             // index into the Material's key list -> complate/preset XML 0x0333406C
    CSTR name
    CSTR pattern
    i32  varCount; { CSTR varName; u8 typeCode; value } [varCount]
         // typeCode 1: CSTR; 2: u32 ARGB; 3: u8 key-list index (e.g. a DDS); 4: f32; 5: f32[2]; 6: f32[3]; 7: u8 bool
    i32  subCount; MaterialBlock sub[subCount]
```

**Skipping a Material without parsing it:** read `materialType`, read `unknown1` if `type != 1`, read `endOffset`
at position `p`, seek to `p + 4 + endOffset`, then read `u32 unknown3`.

`CSTR` ("complate string") [code: s3pi `ComplateString`]:
```
a = u8
a == 0            -> ""
a & 0x80          -> literal ASCII; len = (a & 0x40) ? u8 : (a & 0x3F); bytes[len]
a == 0x40         -> table[0x40 + u8]          // s3pi reader AND writer agree on this
otherwise         -> table[a]
```
The wiki gives `(a & 0x3F) + u8` for the index case, which disagrees with s3pi for `a == 0x40`. s3pi's version
round-trips, so use it. String table (s3pi; index=string): 00='' 01=filename 02=X: 03=-1 04=assetRoot
05=daeFileName 06=daeFilePath 07=Color 08=ObjectRgbMask 09=rgbmask 0a=specmap 0b=Background Image 0c=HSVShift Bg
0d=H Bg 0e=V Bg 0f=S Bg 10=Base H Bg 11=Base V Bg 12=Base S Bg 13=Mask 14=Multiplier 15=Dirt Layer 16=1X Multiplier
17=Specular 18=Overlay 19=Face 1a=partType 1b=gender 1c=bodyType 1d=age 1e=A 1f=M 20..23=Stencil A..D
24..27=Stencil A..D Enabled 28..2b=Stencil A..D Tiling 2c..2f=Stencil A..D Rotation 30..32=Pattern A..C
33..35=Pattern A..C Enabled 36..38=Pattern A..C Linked 39..3b=Pattern A..C Rotation 3c..3e=Pattern A..C Tiling
3f=(unknown) 40='' 41=MaskWidth 42=MaskHeight 43=ObjectRgbaMask 44=RndColors 45=Flat Color 46=Alpha 47..4b=Color 0..4
4c..4e=Channel 1..3 4f=Pattern D 50=Pattern D Tiling 51=Pattern D Enabled 52=Pattern D Linked 53=Pattern D Rotation
54..56=HSVShift 1..3 57..59=Channel 1..3 Enabled 5a=Base H 1 5b=Base V 1 5c=Base S 1 5d=Base H 2 5e=Base V 2
5f=Base S 2 60=Base H 3 61=Base V 3 62=Base S 3 63=H 1 64=S 1 65=V 1 66=H 2 67=S 2 68=V 2 69=H 3 6a=V 3 6b=S 3
6c=true 6d=1,0,0,0 6e=defaultFlatColor 6f=solidColor_1.

These variables are the inputs to the object's TXTC compositor: the preset patterns (DDS), colours, HSV shifts,
the RGB(A) mask and so on. The first preset is the default appearance. In the Expensive Toilet diagram the OBJD key
list points at pattern XMLs (0x0333406C) and their DDS files (`*_s`, `*_paint`, ...).

### 7.4 OBJK 0x02DC343F [code+wiki]

```
u32 version                      // 7
u32 tgiOffset; u32 tgiSize       // key table, relative to after the offset field
u8  componentCount; u32 componentIds[componentCount]       // FNV32 of component name
u8  dataCount
ComponentData data[dataCount]:
    i32 keyLen; char key[keyLen]  // ASCII, e.g. "modelKey"
    u8  controlCode
    0x00 String:            i32 len; char[len]
    0x01 ResourceKey:       i32 index into OBJK key table
    0x02 AssetResourceName: i32 index into OBJK key table
    0x03 SteeringInstance:  i32 len; char[len]
    0x04 UInt32:            u32
u8  visibilityFlag               // 0 VisibleAlways, 1 VisibleInToolsOnly, 2 VisibleAsDistantTerrain
key table: i32 count; { u32 T; u32 G; u64 I; } [count]
```

Components (FNV32 of name; verified): Animation 0xEE17C6AD, Effect 0x80D91E9E, Footprint 0xC807312A, Lighting
0xDA6C50FD, Location 0x461922C8, LotObject 0x6693C8B3, **Model 0x2954E734**, Physics 0x1A8FEB14, Sacs 0x3AE9A8E7,
Script 0x23177498, Sim 0x22706EFA, Slot 0x2EF1E401, Steering 0x61BD317C, Transform 0x54CB7EBB, Tree 0xC602CD31,
VisualState 0x50B3D17C.

Component data keys [wiki]: `modelKey` (Model: **references a VPXY**; Tree: references SpeedTree data),
`footprintKey` (FTPT, "only used if modelKey is not a VPXY"), `scriptClass` (string, e.g.
`Sims3.Gameplay.Objects.Plumbing.Mimics.ToiletExpensive`), `simOutfitKey`, `steeringInstance`, `allowObjectHiding`.

### 7.5 VPXY 0x736884F1 [code+wiki]

```
u32 tag = 'VPXY'
u32 version                 // 4
u32 tgiOffset; u32 tgiSize  // key table (relative); s3pi ignores tgiSize here
u8  entryCount
Entry entries[entryCount]:
    u8 entryType
    0x00: u8 entryId; u8 n; i32 tgiIndex[n]   // "linked parts". entryId: incremental, "for GEOMs this is the LOD number"
    0x01: i32 tgiIndex                        // a single separate part
u8  tc02                    // always 0x02 (type code of the following bounding box)
f32 boundsMin[3], boundsMax[3]
u8  unused[4]
u8  modular                 // 0 or 1
if modular != 0: i32 ftptIndex   // key-table index of the FTPT for modular pieces
key table: i32 count; { u32 T; u32 G; u64 I; } [count]   // GEOM, MODL, MLOD, LITE, RSLT, FTPT (+ _RIG) keys
```

**[unknown]** Exactly which parts go in type-0 versus type-1 entries for build/buy objects is not documented. Scan
the key table by type (step 3 in 7.1).

### 7.6 FTPT 0xD382BF57: footprint [code+wiki]

```
u32 tag = 'FTPT'
u32 version                 // 7 current (elevationOffset present when >= 7)
u8  footprintAreaCount; Area footprintAreas[...]   // placement and routing footprint
u8  slotAreaCount;      Area slotAreas[...]        // slot-related areas
Area:
    u32 nameHash
    u8  priority
    u32 areaTypeFlags       // 0x1 ForPlacement, 0x2 ForPathing, 0x4 IsEnabled, 0x8 IsDiscouraged, 0x10 ForShell
    u8  pointCount; { f32 x; f32 z; } polygon[pointCount]   // closed polygon in object space, XZ plane
    u32 allowIntersectionFlags  // 0x2 Walls, 0x4 Objects, 0x8 Sims, 0x10 Roofs, 0x20 Fences, 0x40 ModularStairs, 0x80 ObjectsOfSameType
    u32 surfaceTypeFlags        // 0x1 Terrain, 0x2 Floor, 0x4 Pool, 0x8 Pond, 0x10 Fence, 0x20 AnySurface, 0x40 Air, 0x80 Roof
    u32 surfaceAttributeFlags   // 0x1 Inside, 0x2 Outside, 0x4 Slope
    u8  levelOffset
    if version >= 7: f32 elevationOffset
    f32 boundsMinX, boundsMinZ, boundsMaxX, boundsMaxZ   // AABB of the polygon
```

The wiki labels the polygon coordinates "X, Y". s3pi names them X and Z, which is the horizontal plane of a Y-up
world. Units are metres in object space: one Sims 3 tile is 1 m [inferred]. Polygon winding is not documented.

### 7.7 LITE 0x03B4C61D: object light sources (brief) [code+wiki]

```
u32 tag='LITE'; u32 version (4); u32 unknown1 (0x84); u8 lightCount; u8 occluderCount; u16 unknown2
Light[lightCount]:    u32 type; f32 transform[3] (position); f32 color[3]; f32 intensity; f32 data[24]
Occluder[occluderCount]: u32 type (0 Disc, 1 Rectangle); f32 origin[3]; f32 normal[3]; f32 xAxis[3]; f32 yAxis[3]; f32 pairOffset
```
Light types: 1 Ambient, 2 Directional, 3 Point, 4 Spot, 5 LampShade, 6 TubeLight, 7 SquareWindow, 8 CircularWindow,
9 SquareAreaLight, 10 DiscAreaLight, 11 WorldLight. Each light has 24 type-specific floats. Spot: `at[3],
falloffAngle, blurScale`, then unused. LampShade: `at[3], falloffAngle, shadeLightRigMultiplier, bottomAngle,
shadeColor[3]`. TubeLight: `at[3], tubeLength, blurScale`. SquareWindow: `at[3], right[3], width, height,
falloffAngle, windowTopBottomAngle`. CircularWindow: `at[3], right[3], radius`. The rest of each 24 is padding.

---

## 8. Textures

### 8.1 DDS resources (0x00B2D882) [code: s3pi `DdsFile.cs`; wiki DDS page]

* **Confirmed:** the resource body (after any DBPF-level compression is removed) is a complete, standard Microsoft
  DDS file: `u32 magic = 0x20534444 ("DDS ")`, the 124-byte `DDS_HEADER` with a 32-byte `DDS_PIXELFORMAT`, then
  mip level 0 down to the smallest. There is no EA wrapper (unlike Sims 4's DST/RLE2 variants).
* **Formats:** s3pi's viewer supports exactly the FourCCs DXT1, DXT3 and DXT5, plus uncompressed 32-bpp
  A8R8G8B8/X8R8G8B8 (via RGB masks) and A8L8 luminance (`L mask 0xFF`, `A mask 0xFF00`). It rejects everything else
  (palette, YUV, alpha-only, nVidia normal). The wiki says EA textures are saved as DXT1 (no alpha) or DXT5
  (alpha). Expect mostly DXT1/DXT5 and support the uncompressed formats as a fallback.
* **Header quirks to tolerate** (s3pi has explicit fix-ups; at least some come from files written by older modding
  tools):
  * FourCC holds DXT1/3/5 but the `DDPF_FOURCC` flag is not set. Trust the FourCC.
  * Both "alpha-only" and RGB flags set. Treat as RGB with alpha.
  * `DDSD_PITCH`/`DDSD_LINEARSIZE` missing or wrong. Compute sizes from width, height and format rather than
    trusting `pitchOrLinearSize`.
  * Uncompressed with no format flag at all. Default to RGB.
  * Also defensive [inferred]: treat `mipMapCount == 0` as 1, clamp the mip chain to the bytes actually present,
    and use the DXT block size (8 bytes DXT1, 16 bytes DXT3/5) with `max(1, (w+3)/4) * max(1, (h+3)/4)` blocks.
* **Channel conventions** [wiki]:
  * **Normal maps** (MATD `NormalMap`): DXT5, "DXT5nm"-style. **X is in alpha, Y in green** (also copied into R and
    B). Reconstruct with `x = a*2-1, y = g*2-1, z = sqrt(max(0, 1-x*x-y*y))`. Whether Y needs flipping is not
    documented [unknown]; test on a known model.
  * **Specular**: DXT5. RGB = specular colour, alpha = shininess/intensity ("the whiter the pixel, the shinier").
  * **Multiplier**: greyscale shading in RGB; alpha = cutout/transparency where the shader supports it.
  * **Mask**: R/G/B (+A) = which pattern channel applies (TXTC input).
  * **Overlay / Stencil**: RGBA decals composited on top (TXTC inputs).
* **Where they live:** in the toilet example the model RCOLs are in `FullBuild0.package` and the DDS files in
  `FullBuild2.package`. A key referenced from an RCOL's external list can be in any installed package (base game,
  EPs/SPs, DeltaBuild patches), so the resource index must span every package and honour patch override order.

### 8.2 TXTC (0x033A1435): texture compositor (summary only) [wiki + code: s3pi `TxtcResource.cs`]

A MATD texture parameter (typically `DiffuseMap`, often `SpecularMap`) can point at a TXTC instead of a DDS. A TXTC
is a small program of render steps that the game executes at load time to bake the final texture from the
multiplier, mask, pattern DDS files, colours and HSV shifts selected by the OBJD preset (7.3). A faithful
reimplementation needs a TXTC interpreter. A useful first approximation [inferred] is to draw the `Draw Image` step
whose source is the multiplier DDS, tinted by the preset's flat colour.

```
u32 version
u32 tgiOffset                 // relative to after this field; points at the u8 TGI count at the end
if version >= 7:
    u8 superBlockCount
    { u8 tgiIndex; u32 size; TXTC nested /*same format, recursive*/; u8 unk[3]; } [superBlockCount]  // embedded "fabric" compositors
u32 patternSize               // 0 Default, 1 Large
u32 partType                  // s3pi "dataType" flags
u8  unknown
i32 stepCount
if version >= 8: u8 unknown
Step steps[stepCount]:        // each step = property list terminated by a u32 0
    repeat: u32 propertyId (FNV32); if 0 -> end of step
            u8 unknown (0); u8 dataType; value
    dataType: 0 bool(u8) 1 i8 2 i16 3 i32 4 i64 5 u8 6 u16 7 u32 8 u64 9 f32 0x0A RectF f32[4] 0x0B Vector4 f32[4]
              0x0C u8 index into the TXTC TGI list 0x0D u16 len + chars
u8  tgiCount; { u64 instance; u32 group; u32 type; } [tgiCount]      // NOTE: "IGT" order
```

Step-type ids (property `ID` 0x687720A6): Draw Image 0xA15200B1, Color Fill 0x9CD1269D, Channel Select 0x1E363B9B,
Remapped Channel Select 0x890805DB, Draw Fabric 0x034210A5, Set Target 0xD6BD8695, HSV to RGB 0xDC0984B9, Skin Tone
0x43B554E3, Hair Tone 0x5D7C85D4, CAS Pick Data 0xC6B6AC1F. Other important properties: Image Key 0xF6CC8471,
Mask Key 0x49DE3B16, Channel Select 0xD0E69002, Colour 0xB01748DA, HSV Shift 0xB67C2EF8, Source/Destination
Rectangle 0xA3AAFC98/0xE1D6D01F, Source/Destination Blend 0xE055EE36/0x048F7567, Enable Blending 0xFBF310C7, Colour
Write 0xB07B3B93, Render Target 0xA2C91332, Width/Height 0x182E64EB/0x4C47D5C0, Default Fabric 0xDCFF6D7B. The full
property and blend-factor tables are on the wiki page (`Sims_3:0x033A1435`). TXTC deserves its own document.

---

## 9. Implementation checklist (Rust)

* RCOL reader: header, chunk table (with the single-chunk quirk), `ChunkRef { kind, index }` decoding, lazy chunk
  parsing by tag.
* `Mlod::meshes[*]`: validate `meshSize` against bytes consumed (both tools do), so version drift shows up at once.
* VRTF-driven vertex decoder: per element `(usage, usageIndex, format, offset)` produce f32 attributes with the 4.2
  rules. Pre-decode the whole IBUF once (delta, signed, whole buffer).
* Material: parse MTNF by offsets into `HashMap<u32 /*fnv*/, ParamValue>`. Resolve MTST -> default MATD. Resolve
  texture refs through the right RCOL's external list.
* Object resolution: OBJD (parse through the material list, instance name and Common block to `objkIndex`) ->
  OBJK `modelKey` -> VPXY key scan -> MODL HighDetail LOD.
* Validation harness: decoded-vertex AABB against MLOD mesh bounds and MODL bounds; index range `< vertexCount`;
  log any unknown VRTF format, VBUF flags != 0, IBUF display-list flag, non-zero `scaleOffset`, or primitive type != 3.
* Coordinate system: data is D3D-style (Y-up, left-handed, clockwise front faces) [inferred]. Bevy is right-handed
  Y-up, so mirror one axis (for example negate Z on positions and normals, flip tangent handedness) and reverse
  triangle winding.

## 10. Example: decoding one MLOD mesh (pseudo-code)

```
rcol  = Rcol::parse(bytes)
mlod  = rcol.chunk_by_tag("MLOD")                  // or follow MODL.lods[HighDetail].modelLod
ibufs = {}                                         // cache decoded IBUF per chunk index
for mesh in mlod.meshes:
    if mesh.vertexFormat == 0: continue            // shadow/drop-shadow geometry (no VRTF, section 4.3)
    if rcol.chunk_type(mesh.vertexBuffer) == 0x0229684B: continue        // shadow VBUF
    // (also skip if the resolved MATD shader is DropShadow 0xC09C7582)
    vrtf = rcol.resolve(mesh.vertexFormat) as VRTF
    vbuf = rcol.resolve(mesh.vertexBuffer) as VBUF
    ibuf = ibufs.get_or_insert(mesh.indexBuffer, decode_ibuf(rcol.resolve(mesh.indexBuffer)))
    mat  = rcol.resolve(mesh.material); while mat is MTST: mat = rcol.resolve(mat.defaultMaterial)
    uvs  = mat.params.get(0x420520E9).unwrap_or([1/32767; 3])           // UVScales
    for v in 0..mesh.vertexCount:
        base = mesh.streamOffset + v * vrtf.stride
        for el in vrtf.elements: decode(vbuf.data[base + el.offset ..], el, uvs[el.usageIndex])
    tris = ibuf[mesh.startIndex .. mesh.startIndex + 3*mesh.primitiveCount]   // relative to streamOffset
    diffuse = mat.params.get(0x6CC0FD85)  -> ChunkRef(kind=3) -> rcol.external[idx] -> DDS | TXTC
```

---

## 11. Documented vs. guessed: open questions

Solid (two independent implementations and/or wiki agree): the RCOL header and ChunkReference; the MODL, MLOD
(0x201/0x202), VRTF, VBUF header, IBUF and SKIN layouts; the MATD/MTNF/MTRL layout and texture-reference encoding;
the MTST, GEOM v5, VPXY, FTPT v7, OBJK and OBJD (to v0x22) layouts; `UVScales` scaling of Short2 UVs;
ColorUByte4 normal and weight decoding; IBUF delta decoding; FNV32 naming.

Unknown or uncertain:

1. **Scale/offset MATD** (`MLOD.mesh.scaleOffset` -> PosScale/PosOffset/UVScale/UVOffset): when it is non-zero and
   how it should be applied. No tool applies it. Validate against bounds (4.6).
2. **`UShort4N` zero-scalar default**: 512 (wiki and s3pi writer) or 511 (both readers).
3. **VBUF flags** `DifferencedVertices` (0x2) and `Collapsed` (0x4): their encoding is unknown, and whether shipped
   data uses them is unknown.
4. **VRTF formats 0x08-0x10** (UByte4N, Short2N, Short4N, UShort2N, Dec3N, UDec3N, Float16_2/4): defined but never
   decoded by any tool. Their presence in shipped objects is unconfirmed.
5. **Colour (vertex colour) channel order** for ColorUByte4: BGRA (D3DCOLOR) is assumed; the tools read raw order.
6. **Blend index target**: whether indices address the MLOD mesh's `jointNameHashes` or the SKIN bone list (they
   usually coincide).
7. **Primitive-type numbering above 5** differs between s3pi and the wiki. Only TriangleList (3) has been observed.
8. **IBUF `IsDisplayList`** and `displayListUsage`, and whether 16-bit delta streams wrap modulo 65536.
9. **MODL LOD selection**: the meaning and units of `minZValue`/`maxZValue`, and the FadeType values.
10. **VPXY entry semantics** for build/buy objects (type-0 "linked" vs type-1 "separate" parts; the meaning of
    `entryId`). The recommended workaround is to scan the key list by type.
11. **GEOM versions other than 5**, and the bind-pose matrix layout of the skin controller (0x00AE6C67) (s3py's
    reader and writer disagree).
12. **Shader semantics**: shader programs are compiled into the game, so how each shader uses its parameters is
    reverse-engineering territory. The name for 0xFC5FC212 ("PhongAlpha") is not an FNV32 match.
13. **Normal-map Y orientation** (DXT5nm, X in alpha and Y in green is documented; the sign convention is not).
14. **TXTC execution semantics** (blend equations, channel-select details, how OBJD preset variables bind to TXTC
    inputs): only the container format is documented here.
15. **OBJD `Common.unknown7`, Material `unknown1/unknown2/unknown3`, MTNF/MTRL unknown words**: preserved by the
    tools, meaning unknown and irrelevant for rendering.
16. **Coordinate handedness and winding** (D3D left-handed, clockwise) are inferred from the engine's D3D9
    heritage, not stated by any source. Confirm visually on a known asymmetric object.
