# Sims (CAS) rendering & animation — format notes

Research notes for rendering and animating Sims from the user's installed TS3 data.
Status: complete first pass (2026-10-04). Each claim is tagged:

- **[src]** — transcribed from reader source code that is known to work (s3pi, s3py, nwn2mdk, LSLib, ...)
- **[wiki]** — from the Mod The Sims (MTS) wiki / SimsWiki; usually right, sometimes stale
- **[guess]** — my inference; verify against real data before relying on it
- **[verified]** — I checked it myself against real game-derived files (rig files shipped with blender-sims3-geom) or by recomputing hashes against ids quoted online

All integers are little-endian unless stated. "TGI" = (type u32, group u32, instance u64).

Primary sources (cloned/read for this document):

| Source | What it gives us |
|---|---|
| [s3pi-wrappers (mattparizeau mirror)](https://github.com/mattparizeau/s3pi-wrappers) `s3piwrappers.AnimationResources/ClipResource.cs`, `s3piwrappers.Helpers/Cryptography/FNV*.cs` | CLIP outer container, S3CLIP inner parser (older interpretation), FNV + CLIP instance hashing |
| [s3py (garthand fork)](https://github.com/garthand/s3py) `s3py/animation/__init__.py`, `animation/rig.py`, `animation/blender.py`, `cas/catalog.py`, `cas/geometry.py` | The code behind the *Sims 3 CLIP Tool for Blender* (round-trips clips into the game). Best CLIP decoder; new-format RIG; BOND; CASP; FACE/BBLN; TONE; GEOM |
| [Sims4Tools (s4pi)](https://github.com/s4ptacle/Sims4Tools) `s4pi Wrappers/AnimationResources/S3CLIP.cs` | EA's real enum names for the S3CLIP channel types (TS4 shares the `_pilC3S_` format) |
| [nwn2mdk](https://github.com/Arbos/nwn2mdk) `nwn2mdk-lib/gr2_decompress.cpp`, `gr2_file.cpp`, `gr2.h` | **Portable Oodle1 decompressor** + GR2 **v6** reader (same file version as TS3's rigs). Apache-2.0 / Boost licence on the decompressor |
| [opengr2](https://github.com/arves100/opengr2) `libopengrn/oodle1.c`, `gr2_read.c` | C port of the same Oodle1 code (MPL-2.0), GR2 low-level reader, [format wiki](https://github.com/arves100/opengr2/wiki/File-Format-documentation) |
| [LSLib (Norbyte)](https://github.com/Norbyte/lslib) `LSLib/Granny/GR2/Format.cs`, `Reader.cs` | Authoritative GR2 type-tree (member type ids, element sizes, read order) |
| [MTS wiki: Sims_3:0x8EAF13DE (RIG)](https://modthesims.info/wiki.php?title=Sims_3:0x8EAF13DE) | Granny wrapper + **new non-Granny RIG format** |
| [MTS wiki: Sims_3:0x6B20C4F3 (CLIP)](https://modthesims.info/wiki.php?title=Sims_3:0x6B20C4F3) | CLIP layout (wiki version) |

---

## 1. Skeletons — `_RIG` 0x8EAF13DE

### 1.1 Two (really three) formats exist — check DeltaBuild first

The MTS wiki page ([Sims_3:0x8EAF13DE](https://modthesims.info/wiki.php?title=Sims_3:0x8EAF13DE)) says **[wiki]**:

> "There was once the proprietary GR2 format, but it has since been replaced with EA's own format
> that is much easier to deal with. ... It has been deprecated by patches and expansions, and new
> versions of the base game rig files can be found in **DeltaBuild0.package**. Pets in their own EP
> were made using the new format as well." — "Starting with the Create A Pet Demo, there is a new
> third format of RIG files that is unencrypted."

So for any RIG instance there may be:

1. **Raw Granny2** (`B8 67 B0 CA ...` at offset 0) — what `FullBuild*.package` contains (confirmed by the
   coordinator on the user's data: GR2 v6, 1 section, compression 2 = Oodle1).
2. **Wrapped Granny2** — first DWORD = `0x8EAF13DE` (see 1.3).
3. **New EA format** — first DWORD = 3 or 4 (major version). Same layout later used by TS4.

**Action item:** look up the same RIG TGI in `GameData/Shared/DeltaPackages/p0/DeltaBuild0.package`
(and in EP `DeltaBuild*.package`s). If the patched copy is the new format, prefer it — it is a trivial
reader (1.2) and avoids Oodle1/GR2 entirely. DBPF priority: delta packages override full-build packages
with the same TGI. **[guess: not verified on user data]** Keep the GR2 path (1.4–1.7) as the fallback.

Rig instance ids are FNV-64 of the rig name **[guess, consistent with every other named TS3 resource]**.
Names: s3py `CASPart.get_rig()` builds `"%s%sRig" % (age_char, species_char)` with age `a/b/c/e/t/p`
and species `u` human, `c` cat, `d` dog/little dog, `h` horse/deer, `r` raccoon. The skeleton-name
field of real rig files confirms `auRig`, `cuRig`, `puRig` (+ pet rigs) **[verified, §3]**; blender-sims3-geom
offers exactly au (adult) / cu (child) / pu (toddler) for humans, so teen..elder presumably share
`auRig`; `buRig` (baby) is unconfirmed:

| name | FNV-64 (lower-cased) | FNV-32 |
|---|---|---|
| `auRig` | `D057FCC534C1BCBB` | `43AC09FB` |
| `cuRig` | `2183C4B327C38FC9` | `9E5E5F09` |
| `puRig` | `192F5C47F5D28A72` | `585CC6B2` |
| `buRig` | `193F4CBE84707158` | `C6868098` |

(Check these against the RIG instance list in FullBuild; if they don't match, list RIG instances and
look for a name table — `0x0166038C` NMAP — in the same package.)

**Object rigs vs sim rigs:** both use type 0x8EAF13DE and the same three container variants, so one
reader serves both. Sim rigs are the named skeletons `auRig`/`cuRig`/`puRig` (+ pets `acRig`, `adRig`,
`alRig`, `ahRig`, `ccRig`, `cdRig`, `chRig`) with 257 bones for humans (§3). Object rigs are small
per-object skeletons (`b__ROOT__`, a few joints, `*_slot` attachment points, IK helpers) referenced
from the object's model/RCOL chain **[guess on how they are referenced]**. Patch 1.26 changed object
rig handling enough that MTS has a "Fixing Object RIGs for Patch 1.26" tutorial and RIG maker/fix
tools ([tutorial](https://modthesims.info/wiki.php?title=Tutorial:Fixing_Object_RIGs_for_Patch_1.26)),
i.e. the game expects object rigs in the new format too post-patch.

### 1.2 New (non-Granny) RIG format **[wiki]+[src: s3py `SkeletonRig.read`]**

```
u32 major            // 3 or 4  (s3py writes 4)
u32 minor            // 1 or 2  (s3py writes 2)
i32 bone_count
repeat bone_count:
    f32 position[3]        // LOCAL, parent-relative translation (metres, Y-up)
    f32 orientation[4]     // LOCAL quaternion, order x, y, z, w
    f32 scale[3]
    i32 name_len ; char name[name_len]    // ASCII, NOT NUL-terminated (s3py p32)
    i32 opposite_bone      // mirror partner index (== own index if unpaired; -1 possible)
    i32 parent_index       // -1 for root
    u32 name_hash          // FNV-32(lower-case name); s3py warns if mismatched
    u32 flags              // wiki: usually 0x23, 0x3F on mirrored bones; auRig has 0x20..0x5A -- ignore
i32 skel_name_len ; char skel_name[...]   // wiki + s3pi: always present (v3 => length 0);
                                          // (s3py reads it only if major >= 4)
if major >= 4:                            // wiki + s3pi; s3py reads chains unconditionally
    i32 chain_count
    repeat chain_count:
        i32 n ; i32 bones[n]               // bone indices
        i32 info_node[11]
        i32 pole, slot_info, slot_offset, root  // bone indices, -1 = none
```

Positions/orientations are parent-relative: s3py builds the Blender armature by creating every bone at
identity, setting `pose.location = position`, `pose.rotation = orientation` under the parent, then
"apply pose as rest". It then rotates the whole armature +90° about X to go from TS3's **Y-up** to
Blender's Z-up — so TS3 data is Y-up like Bevy; no axis swap needed **[src]**.

Bind pose world matrix: `W[i] = W[parent] * T(pos) * R(quat) * S(scale)`; inverse bind = `W[i]^-1`.

### 1.3 Wrapped Granny format **[wiki]**

```
u32 0x8EAF13DE
u32 0
u32 offset1              // to "Int2" block, from start of Granny data
u32 offset2              // to start of Granny data (from start of this DWORD)
u32 ik_chains_offset     // all following offsets: from start of their own DWORD
u32 ik_target_slots_offset
u32 ik_info_node_offset
u32 compress_node_offset
u32 full_bone_list_offset
u32 unused[4]
<Granny2 file>  (padded with 00 to DWORD)
... IK chain table, target slots, info nodes, "compress" nodes (b__R_Calf__Compress__ etc.),
    full bone list -- all as indices into the Granny skeleton's bone array.
```
For rendering you only need the embedded Granny file: skip to `offset2`.

### 1.4 Granny2 (GR2) file container, version 6, 32-bit LE **[src: nwn2mdk gr2_file.h, LSLib Format.cs]**

```
0x00  u8  magic[16]   = B8 67 B0 CA F8 6D B1 0F 84 72 8C 7E 5E 19 00 1E   (v6 LE32)
0x10  u32 headers_size     // size of all headers = offset where section data starts (LSLib)
0x14  u32 header_format    // 0
0x18  u32 reserved[2]
---- "Info" header (0x38 bytes in v6) starts at 0x20 ----
0x20  u32 version          // 6
0x24  u32 file_size
0x28  u32 crc32            // standard CRC-32 of bytes [0x20+0x38 .. file_size)  (LSLib: Crc32 of body after header)
0x2C  u32 sections_offset  // relative to 0x20 (so 0x38 -> section table at 0x58)
0x30  u32 section_count    // TS3 rigs: 1
0x34  u32 root_type_section ; 0x38 u32 root_type_offset   // -> MemberDef[] describing the root struct
0x3C  u32 root_node_section ; 0x40 u32 root_node_offset   // -> root object (granny_file_info)
0x44  u32 tag              // Granny type-version tag; wiki shows 0x8000001C for TS3
0x48  u32 extra_tags[4]    // wiki calls these "StringDb CRC, unused[3]"
0x58  SectionHeader[section_count]
```

`SectionHeader` (44 bytes):

```
u32 compression        // 0 none, 1 Oodle0, 2 Oodle1 (TS3), 3/4 BitKnit (v7 only)
u32 data_offset        // absolute file offset of (compressed) section data
u32 data_size          // compressed size
u32 decompressed_size
u32 alignment          // usually 4
u32 first16bit         // Oodle1 "stop 0" -- end of 1st sub-stream (bytes of output)
u32 first8bit          // Oodle1 "stop 1" -- end of 2nd sub-stream
u32 relocations_offset // absolute file offset; NOT compressed in v6
u32 relocation_count
u32 marshalling_offset // endian-swap info; ignore for LE files
u32 marshalling_count
```

`Relocation` (12 bytes): `u32 offset_in_section; u32 target_section; u32 target_offset`.
Meaning: "the 32-bit slot at `section[offset_in_section]` is a pointer to
`section[target_section] + target_offset`". Pointer slots without a relocation are NULL (they hold 0).

`Marshalling` (16 bytes): `u32 count; u32 offset_in_section; u32 target_section; u32 target_offset`
— only needed when the file endianness differs from the CPU. Skip.

Loading recipe (what nwn2mdk does):

```
read header; check magic; (optionally) check CRC
base[i] = sum(decompressed_size[0..i])           // all sections into ONE buffer
buf = zeroed(sum(decompressed_size))
for each section i:
    raw = file[data_offset .. data_offset+data_size] + 4..8 zero bytes of padding
    match compression:
        0 => copy
        2 => oodle1_decompress(raw, data_size, first16bit, first8bit, decompressed_size, &mut buf[base[i]..])
for each section i, each relocation r:
    slot = base[i] + r.offset_in_section
    target = base[r.target_section] + r.target_offset
    ptr_map.insert(slot, target)                 // Rust: keep a map instead of writing raw pointers;
                                                 // distinguishes NULL (no entry) from "points at offset 0"
root     = base[root_node_section] + root_node_offset
root_def = base[root_type_section] + root_type_offset
```

### 1.5 Oodle1 decompression (exact algorithm) **[src: nwn2mdk gr2_decompress.cpp ≡ opengr2 oodle1.c]**

This is a range-coder + adaptive-frequency LZ77 ("weighwindow") scheme. The code below is a faithful
transcription of nwn2mdk's `gr2_decompress.cpp` (itself derived from
[berenm/xoreos-tools granny-decoder](https://github.com/berenm/xoreos-tools/blob/wip/granny-decoder/src/decompress.cpp)).
nwn2mdk optionally cross-checks it byte-for-byte against `granny2.dll`'s `GrannyDecompressData`
(`TEST_GR2_DECOMPRESSION`), so it is known-correct for v6 files. **All arithmetic is u32 unless noted;
the `u16` fields really are 16-bit and wrap/truncate like C.**

#### Header — 3 parameter blocks (36 bytes) at the start of the compressed section

Each block is 12 bytes, read as two LE u32 bitfields + 4 bytes (MSVC/GCC LSB-first bitfield packing):

```
w0 = u32 @+0 : decoded_value_max = w0 & 0x1FF          (9 bits)
               backref_value_max = w0 >> 9             (23 bits)
w1 = u32 @+4 : decoded_count     = w1 & 0x1FF          (9 bits)
               (padding          = (w1 >> 9) & 0x3FF)  (10 bits)
               highbit_count     = w1 >> 19            (13 bits)
u8 sizes_count[4] @+8
```
Block `k` (k = 0,1,2) parameterises the k-th sub-stream: output `[0, first16bit)`,
`[first16bit, first8bit)`, `[first8bit, decompressed_size)`. The range-coded bitstream starts at byte 36
and is **shared/continuous across the three sub-streams**; only the dictionary/model is rebuilt.

#### Range decoder

```rust
struct Decoder { numer: u32, denom: u32, next_denom: u32, pos: usize /* into bytes */ }
fn init(bytes, start) -> Decoder { numer: bytes[start] as u32 >> 1, denom: 0x80, next_denom: 0, pos: start }

fn decode(&mut self, max: u16) -> u16 {
    while self.denom <= 0x80_0000 {
        self.numer <<= 8;
        self.numer |= ((bytes[self.pos] as u32) << 7) & 0x80;
        self.numer |= ((bytes[self.pos + 1] as u32) >> 1) & 0x7F;   // reads one byte ahead -> pad input!
        self.pos += 1;
        self.denom <<= 8;
    }
    self.next_denom = self.denom / max as u32;
    min(self.numer / self.next_denom, max as u32 - 1) as u16
}
fn commit(&mut self, max: u16, val: u16, err: u16) -> u16 {
    self.numer -= self.next_denom * val as u32;
    if (val as u32 + err as u32) < max as u32 { self.denom = self.next_denom * err as u32; }
    else                                      { self.denom -= self.next_denom * val as u32; }
    val
}
fn decode_and_commit(&mut self, max: u16) -> u16 { let v = self.decode(max); self.commit(max, v, 1) }
```

(nwn2mdk also zero-pads the compressed buffer up to a multiple of 4 and allocates +4 bytes.)

#### Adaptive symbol model ("weighwindow")

```
struct WeighWindow {
    count_cap: u16,
    ranges:  Vec<u16>,   // cumulative frequencies, last entry is the 0x4000 sentinel
    values:  Vec<u16>,   // symbol for each slot; slot 0 is the "escape / new symbol" slot
    weights: Vec<u16>,
    weight_total: u16,
    thresh_increase: u16, thresh_increase_cap: u16,
    thresh_range_rebuild: u16, thresh_weight_rebuild: u16,
}
new(max_value: u32, count_cap: u16):
    weight_total = 4; count_cap = count_cap + 1
    ranges = [0, 0x4000]; weights = [4]; values = [0]
    thresh_increase = 4; thresh_range_rebuild = 8
    thresh_weight_rebuild = max(256, min(32 * max_value, 15160))
    thresh_increase_cap = if max_value > 64 { min(2 * max_value, thresh_weight_rebuild / 2 - 32) } else { 128 }

rebuild_ranges():
    ranges.resize(weights.len())
    range_weight: i32 = 8 * 0x4000 / weight_total          // integer division
    start: i32 = 0
    for i in 0..weights.len() { ranges[i] = start as u16; start += (weights[i] as i32 * range_weight) / 8 }
    ranges.push(0x4000)
    if thresh_increase > thresh_increase_cap / 2 { thresh_range_rebuild = weight_total + thresh_increase_cap }
    else { thresh_increase *= 2; thresh_range_rebuild = weight_total + thresh_increase }

rebuild_weights():
    for w in weights { w /= 2 }
    weight_total = sum(weights)
    for i in 1..weights.len():                              // note: re-checks len each iteration
        while i < weights.len() && weights[i] == 0:
            swap(weights[i], weights.last); swap(values[i], values.last)
            weights.pop(); values.pop()
    if let Some(i) = argmax(weights[1..]) + 1:              // first max element (std::max_element)
        swap(weights[i], weights.last); swap(values[i], values.last)
    if weights.len() < count_cap && weights[0] == 0 { weights[0] = 1; weight_total += 1 }

try_decode(dec) -> Result<Known(u16) | New(&mut last_value)>:
    if weight_total >= thresh_range_rebuild:
        if thresh_range_rebuild >= thresh_weight_rebuild { rebuild_weights() }
        rebuild_ranges()
    value = dec.decode(0x4000)
    i = (upper_bound(ranges, value)) - 1                    // largest i with ranges[i] <= value
    dec.commit(0x4000, ranges[i], ranges[i+1] - ranges[i])
    weights[i] += 1; weight_total += 1
    if i > 0 { return Known(values[i]) }
    // escape slot hit
    if weights.len() >= ranges.len() && dec.decode_and_commit(2) == 1:
        // a symbol added since the last range rebuild (has no range yet)
        j = ranges.len() + dec.decode_and_commit((weights.len() - ranges.len() + 1) as u16) as usize - 1
        weights[j] += 2; weight_total += 2
        return Known(values[j])
    values.push(0); weights.push(2); weight_total += 2
    if weights.len() == count_cap { weight_total -= weights[0]; weights[0] = 0 }
    return New(values.last_mut())   // caller decodes a raw symbol and stores it here
```

Helper used everywhere: `sym(window, dec, raw_max) = match window.try_decode(dec) { Known(v) => v,
New(slot) => { *slot = dec.decode_and_commit(raw_max); *slot } }`.

#### Dictionary (LZ model) — one per sub-stream

```
new(p: Params):
    decoded_size = 0; backref_size = 0                       // backref_size persists between blocks
    decoded_value_max = p.decoded_value_max; backref_value_max = p.backref_value_max
    lowbit_value_max  = min(backref_value_max + 1, 4)
    midbit_value_max  = min(backref_value_max / 4 + 1, 256)
    highbit_value_max = backref_value_max / 1024 + 1
    lowbit_window   = WW::new(lowbit_value_max - 1, lowbit_value_max)
    highbit_window  = WW::new(highbit_value_max - 1, p.highbit_count + 1)
    midbit_windows  = [WW::new(midbit_value_max - 1, midbit_value_max); highbit_value_max]
    decoded_windows = [WW::new(decoded_value_max - 1, p.decoded_count); 4]
    size_windows    = for i in 0..4 { for j in 0..16 { WW::new(64, p.sizes_count[3 - i]) } }   // 64 windows
                      + [WW::new(64, p.sizes_count[0])]                                         // 65th

decompress_block(dec, out, pos) -> usize:
    backref_size = sym(size_windows[backref_size], dec, 65)
    if backref_size > 0:
        len = if backref_size < 61 { backref_size + 1 } else { [128, 192, 256, 512][backref_size - 61] }
        range = min(backref_value_max, decoded_size)
        lo  = sym(lowbit_window,       dec, lowbit_value_max)
        hi  = sym(highbit_window,      dec, range / 1024 + 1)
        mid = sym(midbit_windows[hi],  dec, min(range / 4 + 1, 256))
        dist = (hi << 10) + (mid << 2) + lo + 1
        decoded_size += len
        for k in 0..len { out[pos + k] = out[pos + k - dist] }   // forward, overlapping copy (LZ77)
        return len
    else:
        ctx = pos % 4      // nwn2mdk uses (uintptr_t)out_ptr % 4 on a 4-aligned section buffer, i.e.
                           // the output offset within the section mod 4 [verify if a section base is unaligned]
        out[pos] = sym(decoded_windows[ctx], dec, decoded_value_max) as u8
        decoded_size += 1
        return 1
```

#### Driver

```
fn oodle1_decompress(comp: &[u8] /* zero-padded */, stop0: u32, stop1: u32, out: &mut [u8]) {
    if comp.is_empty() { return }
    let params = [Params::read(&comp[0..12]), Params::read(&comp[12..24]), Params::read(&comp[24..36])];
    let mut dec = Decoder::init(comp, 36);
    let stops = [stop0, stop1, out.len() as u32];
    let mut pos = 0;
    for k in 0..3 {
        let mut dic = Dictionary::new(params[k]);
        while pos < stops[k] as usize { pos += dic.decompress_block(&mut dec, out, pos); }
    }
}
```

Pitfalls: `decode()` reads `bytes[pos+1]` (pad ≥ 4 zero bytes); `backref_size` lookups index
`size_windows` with the *previous* block's size symbol (0..=64); window `count_cap` arguments are
`u16`; the escape path's `decode_and_commit` max is `weights.len() - ranges.len() + 1`.

Alternative if this misbehaves: TS3 ships `granny2.dll` in `Game/Bin`; nwn2mdk's
`decompress_section_data_dll` shows the call: `GrannyDecompressData(format=compression, FileIsByteReversed=0,
CompressedBytesSize, CompressedBytes, Stop0=first16bit, Stop1=first8bit, DecompressedBytesSize,
DecompressedBytes)`. Useful only as an offline oracle (32-bit Windows DLL).

### 1.6 GR2 type tree — reading structs generically **[src: LSLib Reader.cs / Format.cs]**

GR2 files are self-describing: every struct pointer has an accompanying array of member definitions.
**Field layouts differ between Granny SDK versions** (e.g. `granny_bone` gained `LODError` and lost
`LightInfo`/`CameraInfo` across versions), so walk the type tree instead of hard-coding offsets.

`MemberDef` (32 bytes in a 32-bit file; an array of them is terminated by `type == 0`):

```
u32 type          // MemberType below
u32 name          // ptr -> NUL-terminated ASCII member name
u32 definition    // ptr -> MemberDef[] (sub-struct) for Inline/Reference/ReferenceToArray/ArrayOfReferences
u32 array_size    // 0 = scalar; N = fixed inline array of N elements
u32 extra[3]
u32 unknown       // 0
```

| id | MemberType | bytes in a 32-bit file | payload layout |
|---|---|---|---|
| 0 | None (terminator) | – | |
| 1 | Inline | size of sub-struct | sub-struct embedded |
| 2 | Reference | 4 | ptr → one sub-struct |
| 3 | ReferenceToArray | 8 | `i32 count; ptr → count × sub-struct (contiguous)` |
| 4 | ArrayOfReferences | 8 | `i32 count; ptr → count × ptr → sub-struct` |
| 5 | VariantReference | 8 | `ptr → MemberDef[] (type); ptr → data` |
| 7 | ReferenceToVariantArray | 12 | `ptr → MemberDef[]; i32 count; ptr → data` |
| 8 | String | 4 | ptr → NUL-terminated string |
| 9 | Transform | 68 | see below |
| 10 | Real32 | 4 | |
| 11/12 | Int8 / UInt8 | 1 | |
| 13/14 | BinormalInt8 / NormalUInt8 | 1 | |
| 15/16 | Int16 / UInt16 | 2 | |
| 17/18 | BinormalInt16 / NormalUInt16 | 2 | |
| 19/20 | Int32 / UInt32 | 4 | |
| 21 | Real16 | 2 | |
| 22 | EmptyReference | (4) [guess] | |

Member offset = running sum of `(array_size.max(1)) * element_size` — LSLib adds **no alignment
padding** (structs that need it carry explicit padding members, e.g. curve headers). Note the count
comes **before** the pointer for both array kinds.

`granny_transform` (Transform, 68 bytes):

```
u32 flags            // 1 = has position, 2 = has orientation, 4 = has scale/shear
f32 position[3]
f32 orientation[4]   // quaternion x, y, z, w
f32 scale_shear[9]   // 3x3 matrix
```
Local matrix = `T(position) * R(orientation) * ScaleShear` [Granny convention; ScaleShear is identity
for TS3 rigs in practice — guess].

### 1.7 Extracting the skeleton

Root struct = `granny_file_info`. Expected members (names are what to look up; layout from nwn2mdk `gr2.h`):

```
ArtToolInfo(Reference) ExporterInfo(Reference) FromFileName(String)
Textures(ArrayOfReferences) Materials(AoR) Skeletons(AoR) VertexDatas(AoR) TriTopologies(AoR)
Meshes(AoR) Models(AoR) TrackGroups(AoR) Animations(AoR) ExtendedData(VariantReference)
```

`Skeletons[k]` → `granny_skeleton { Name: String; Bones: ReferenceToArray<granny_bone>; [LODType: Int32] }`

`granny_bone` (members, look up by name):

```
Name                  String
ParentIndex           Int32        // -1 = root; parents precede children
Transform             Transform    // LOCAL (parent-relative) bind transform  (some SDKs: "LocalTransform")
InverseWorldTransform Real32[16]   // inverse bind matrix (some SDKs: "InverseWorld4x4")
LODError              Real32       // newer SDKs only
LightInfo, CameraInfo Reference    // older SDKs only (nwn2mdk's layout)
ExtendedData          VariantReference
```
With nwn2mdk's (older-SDK) layout a bone is 4+4+68+64+4+4+8 = 156 bytes; with the newer layout
4+4+68+64+4+8 = 152. Use the type tree to know which.

Granny matrices are `float[4][4]` row-major with the translation in elements 12..14 (row-vector
convention, D3D-style). That memory layout equals a **column-major, column-vector** matrix, i.e.
`glam::Mat4::from_cols_array(&m)` should give the right inverse-bind matrix directly **[guess — verify:
`inverse_world[i] * world_from_local_chain[i] ≈ I`]**. You can also ignore `InverseWorldTransform`
and compute it from the local transforms (safer).

Check `ArtToolInfo { FromArtToolName, ArtToolMajorRevision, ArtToolMinorRevision, UnitsPerMeter,
Origin[3], RightVector[3], UpVector[3], BackVector[3] }` once to confirm units = metres and up = +Y.

GEOM meshes refer to bones by **FNV-32 of the bone name** (GEOM section, later), and CLIP tracks are
keyed by the same FNV-32 — so build `HashMap<u32 /*fnv32(name)*/, bone_index>`.

Minimal Rust plan for a GR2 skeleton reader: header → section decompress (Oodle1) → relocation map →
`read_struct(def_ptr, data_ptr)` returning a tree of `Value::{Struct(Vec<(name, Value)>), Array, Ref,
String, F32, I32, Transform, ...}` → pick `Skeletons[0].Bones`.

---

## 2. Animation clips — `CLIP` 0x6B20C4F3

### 2.1 Outer CLIP resource **[src: s3pi ClipResource.Parse, s3py ClipResource.read; matches wiki]**

```
0x00 u32 type_id = 0x6B20C4F3
0x04 u32 linked_clip_offset   // relative*; 0 = none (neither reader supports linked clips)
0x08 u32 clip_size            // byte length of the embedded S3CLIP blob
0x0C u32 clip_offset          // relative*  -> S3CLIP ("_pilC3S_")
0x10 u32 ik_offset            // relative*, 0 = no IK slot-target table
0x14 u32 actor_offset         // relative*  -> NUL-terminated actor name (e.g. "x", "y")
0x18 u32 event_offset         // relative*  -> event table "=CE="
0x1C u32 unknown1
0x20 u32 unknown2
0x24 u32 end_offset           // relative*  -> 4 floats (s3pi "ClipEndSection" X,Y,Z,W; meaning unknown,
                              //   possibly end-of-clip root offset/orientation)  [guess]
0x28 u8  zero[16]
0x38 ... S3CLIP blob, padded to 4 with 0x7E ('~')
     ... optional IK table, actor name, event table, end vector (each padded to 4 with 0x7E)

* relative = value + (file offset of the field itself); a value of 0 means "absent".
```

IK table (only if `ik_offset != 0`): `i32 chain_count; u32 chain_off[chain_count]` (relative to just
after the count), each chain = `u32 0x7E7E7E7E; i32 n; u32 target_off[n]; n × { i32 index; char
namespace[512]; char bone[512] }` (strings NUL-terminated, padded with `#`). Ignore for basic playback.

Event table: `char[4] "=CE="; u32 version (0x103); i32 count; u32 byte_len; u32 start (4 if count>0 else
0)`, then events `u16 type; u16 0xC1E4; u32 id; f32 time; f32 ?; f32 ?; u32 ?; u32 name_len; char
name[] NUL; pad4` + type-specific payload (1 parent/attach: 3 hashes + u32 + 4x4 matrix; 2 unparent;
3 sound: char[128]; 5 effect start; 6 visibility f32; 9 destroy prop; 10 effect stop). Only needed for
props/sounds/effects.

### 2.2 Inner S3CLIP blob (`"_pilC3S_"`) **[src: s3py Clip.read, s3pi Clip.Parse, s4pi S3CLIP.Parse]**

All offsets inside the blob are **absolute from the start of the blob** (the `_` of `_pilC3S_`).

```
0x00 char[8] magic = "_pilC3S_"        (the u64 '_S3Clip_' stored little-endian, i.e. byte-reversed)
0x08 u32  version                      // s3py writes 2
0x0C u32  flags / unknown1             // s3py writes 0
0x10 f32  frame_duration               // seconds per tick; 1/30 in practice (s3py default)
0x14 u16  max_frame_count              // number of ticks (clip length = this * frame_duration)
0x16 u16  unknown2 / padding           // s3py writes 0x6C73
0x18 u32  curve_count                  // "channel count"
0x1C u32  indexed_float_count          // size of the shared float palette
0x20 u32  curve_info_offset            // -> CurveInfo[curve_count]   (normally 0x30)
0x24 u32  float_palette_offset         // -> f32[indexed_float_count]; frame data follows it
0x28 u32  anim_name_offset             // -> NUL-terminated clip name
0x2C u32  src_name_offset              // -> NUL-terminated source file name (Maya scene)
0x30 CurveInfo[curve_count]            // 20 bytes each
     anim_name\0  src_name\0
     f32 float_palette[indexed_float_count]
     frame data for each curve, in curve order
```

`CurveInfo` (20 bytes):

```
u32 frame_data_offset   // absolute in blob
u32 track_key           // bone: FNV-32(lower-case bone name), e.g. fnv32("b__ROOT_bind__") = 0x57884BB9
                        // morph/driver tracks: (FNV-64(name) & 0xFFFFFFFF)   [src: s3py blender.py]
f32 offset              // dequantisation offset
f32 scale               // dequantisation scale (s3py writes (min-max)/2, i.e. may be NEGATIVE)
u16 frame_count         // number of keyframes stored
u8  channel_type        // a.k.a. "flags" -- see table
u8  sub_target          // 1 = translation, 2 = orientation, 3 = scale, 7(s3py) = morph weight,
                        // 14+ = IK target weights/offsets (TS4 enum)
```

`channel_type` byte. EA's real enum (from s4pi `S3CLIP.ChannelType`) vs the bit-field reading used by
the TS3 tools (low 3 bits = data type, bit 3 = static, high nibble = "format"):

| value | s4pi ChannelType | s3py reading | payload per keyframe (after the 4-byte key header) |
|---|---|---|---|
| 0x01 | F1 | – | 1 × u16 index into float palette (+2 pad) [s4pi width/count] |
| 0x02 | F2 | VECTOR3_PACKED type, format 0 | 2 × u16 index |
| 0x03 | F3 | **VECTOR3_INDEXED** | 3 × u16 index into float palette (+2 pad per s4pi; s3py reads no pad — see note) |
| 0x04 | F4 | VECTOR4 type, format 0 | 4 × u16 index |
| 0x05 | F1_Normalized | **SCALAR** (morph) | s4pi: 1 byte + 3 pad; s3py: 1 × u16 (16-bit normalised) — conflict |
| 0x09–0x0C | F1..F4_Zero | static | none (constant 0) |
| 0x0D–0x10 | F1..F4_One | static | none (constant 1) |
| 0x11 (17) | F4_QuaternionIdentity | static | none (identity quaternion) |
| **0x12 (18)** | **F3_HighPrecisionNormalized** | **VECTOR3_PACKED (type 2, format 1)** — s3py's default for translations | **1 × u32: three 10-bit fields** (bits 0–9 = x, 10–19 = y, 20–29 = z) |
| 0x13 (19) | F4_HighPrecisionNormalized_Quaternion | – | 1 × u32, 10-bit fields (TS4; not seen in TS3 tools) |
| **0x14 (20)** | **F4_SuperHighPrecision_Quaternion** | **VECTOR4_PACKED (type 4, format 1)** — s3py's default for rotations | **4 × u16, low 12 bits used** (x, y, z, w) |
| 0x15 (21) | F3_HighPrecisionNormalized_Quaternion | – | TS4 only ("does not support offset or scale") |

The two readings agree on everything the TS3 tools actually write and read (0x12, 0x14, 0x03). Treat
0x12 / 0x14 / 0x03 as the main cases, handle 0x09–0x11 as constants, and log anything else.

The MTS CLIP wiki reads the last two bytes of `CurveInfo` as one **u16 "frame data type" =
`sub_target << 8 | channel_type`** and lists the values actually observed in TS3 data **[wiki]** — they
line up exactly with the s4pi enum:

| u16 | meaning (wiki) | s4pi decoding |
|---|---|---|
| 0x0103 | translation, 3 × WORD indexed-float references | sub 1 Translation, F3 |
| 0x010B | "null translation", always frame count 0 | sub 1, F3_Zero |
| 0x0112 | translation, 1 DWORD = 3 × 10-bit | sub 1, F3_HighPrecisionNormalized |
| 0x020C | "null rotation", count 0 | sub 2, F4_Zero |
| 0x0211 | rotation, always count 0 | sub 2, F4_QuaternionIdentity |
| 0x0214 | rotation quaternion, 4 × 12 bits in 8 bytes, **order XYZW** | sub 2, F4_SuperHighPrecision_Quaternion |
| 0x0705 | (Pets+) scalar morph, WORD / 0xFFFF then scale/offset (≈0..1) | sub 7, F1_Normalized |
| 0x0709 | (Pets+) unknown, count 0 | sub 7, F1_Zero |

Wiki notes: `frame_duration` "always seen as 1/30"; `max_frame_count` "not necessarily the number of
frames in this clip"; the key-flags word "has some data beyond sign bits"; outer header unknown1 is
"0 or 2", unknown2 "all have been 1"; the 16-byte end block is always `0,0,0,1` (identity quaternion).
Morph track keys are `FNV64(name) & 0xFFFFFFFF` of a 0x0A037DDA (pet face morph blend) resource.
"Original core decoded by Karybdis." The 0x0705 entry settles the scalar conflict above in favour of
s3py (one u16, /65535) for TS3.

Constant channel types (0x09–0x11) come with `frame_count == 0`, so there are no key records at all.
Whether "null translation" means *local translation = (0,0,0)* or *track not animated* is not
documented; start by treating them as "keep the bind-pose value" and compare visually **[guess]**.

Keyframe record (repeated `frame_count` times, starting at `frame_data_offset`):

```
u16 frame_index      // tick number; time = frame_index * frame_duration
u16 key_flags        // bits 0..3: SIGN bits, one per component (bit c set => component c negative)
                     // bits 4..15: unknown (s3pi keeps flags >> 4; s3py preserves them)
payload              // per table above
```

Dequantisation (identical in s3pi and s3py) **[src]**:

```
fn decode_component(raw: u32, bits: u32, sign_bit_set: bool, offset: f32, scale: f32) -> f32 {
    let max = ((1u32 << bits) - 1) as f32;          // 1023 for 10-bit, 4095 for 12-bit, 65535 for 16-bit
    let mut v = raw as f32 / max;                   // 0..1
    if sign_bit_set { v = -v; }                     // -1..1
    v * scale + offset
}

// 0x12 translation:  p = read_u32(); x = p & 0x3FF; y = (p >> 10) & 0x3FF; z = (p >> 20) & 0x3FF   (bits = 10)
// 0x14 orientation:  for c in 0..4 { raw = read_u16() & 0x0FFF }                                  (bits = 12)
// 0x03 indexed vec3: for c in 0..3 { v = palette[read_u16()]; if sign(c) { v = -v }; out = v * scale + offset }
//                    (the palette value is used directly in place of raw/max)
```

Then for orientations, renormalise the quaternion (quantisation error) and treat it as **(x, y, z, w)**
(s3py converts with `quat_wxyz(data)` before handing to Blender, i.e. stored order is xyzw) **[src]**.

Note on the 0x03 payload size: s3py reads exactly 3 × u16 = 6 bytes after the key header (10-byte keys);
s4pi pads odd counts to 4 bytes (12-byte keys). Disambiguate from the data: the next curve's
`frame_data_offset` tells you the real stride (`(next_off - this_off) / frame_count`).

### 2.3 Semantics — how to apply a clip to the skeleton **[src: s3py animation/blender.py]**

- Each track (= all curves sharing a `track_key`) targets the bone whose `fnv32(name) == track_key`.
- Values are the bone's **full local (parent-relative) transform**, not a delta from bind pose: s3py
  exports `inverse(parent_world) * bone_world` → `(translation, rotation)` per key, and on import sets
  `bone_world = parent_world * T(position) * R(orientation)`.
- If a track lacks a translation (or rotation) curve, keep the bind-pose value for that component.
- Keys are sparse: s3py's exporter writes a key only when the value changed by > 0.0001 from the previous
  key, so **interpolate** between keys (lerp translations, slerp/nlerp rotations) and hold the last key.
- s3py forces identity rotation on the top-level `b__ROOT__` on import (root orientation is not taken
  from the clip). `b__ROOT_bind__` carries the body offset; root motion (locomotion) lives on
  `b__ROOT__`'s translation track **[guess from naming — verify on a walk clip]**.
- Morph ("driver") tracks are children of `b__DRIVERS__` in the Blender rig; they use `sub_target` 7 and
  a scalar channel. Ignore initially.
- Clip duration = `max_frame_count * frame_duration`; for looping clips (walk cycles) wrap time.

### 2.4 Decoder pseudo-code

```rust
struct Key<T> { tick: u16, value: T }
struct Track { bone_hash: u32, translation: Vec<Key<Vec3>>, rotation: Vec<Key<Quat>>, scale: Vec<Key<Vec3>> }

fn parse_clip(res: &[u8]) -> Clip {
    assert_eq!(u32(res, 0), 0x6B20C4F3);
    let rel = |field: usize| { let v = u32(res, field); if v == 0 { None } else { Some(field + v as usize) } };
    let clip_size = u32(res, 8) as usize;
    let blob = &res[rel(0x0C).unwrap()..][..clip_size];
    assert_eq!(&blob[0..8], b"_pilC3S_");
    let frame_dt   = f32(blob, 0x10);
    let ticks      = u16(blob, 0x14);
    let n_curves   = u32(blob, 0x18) as usize;
    let n_pal      = u32(blob, 0x1C) as usize;
    let info_off   = u32(blob, 0x20) as usize;
    let pal_off    = u32(blob, 0x24) as usize;
    let name       = cstr(blob, u32(blob, 0x28) as usize);
    let palette: Vec<f32> = (0..n_pal).map(|i| f32(blob, pal_off + 4 * i)).collect();
    let mut tracks: IndexMap<u32, Track> = ...;
    for c in 0..n_curves {
        let ci = info_off + 20 * c;
        let (data_off, key, offset, scale) = (u32(blob, ci) as usize, u32(blob, ci + 4), f32(blob, ci + 8), f32(blob, ci + 12));
        let (n, chan, sub) = (u16(blob, ci + 16) as usize, blob[ci + 18], blob[ci + 19]);
        let mut p = data_off;
        for _ in 0..n {
            let tick = u16(blob, p); let kf = u16(blob, p + 2); p += 4;
            let sgn = |c: usize| (kf >> c) & 1 == 1;
            let deq = |raw: u32, bits: u32, c: usize| { let mut v = raw as f32 / ((1u32 << bits) - 1) as f32; if sgn(c) { v = -v }; v * scale + offset };
            let vals: Vec<f32> = match chan {
                0x12 => { let w = u32(blob, p); p += 4; (0..3).map(|c| deq((w >> (10 * c)) & 0x3FF, 10, c)).collect() }
                0x14 => { let v = (0..4).map(|c| deq(u16(blob, p + 2 * c) as u32 & 0xFFF, 12, c)).collect(); p += 8; v }
                0x03 => { let v = (0..3).map(|c| { let mut x = palette[u16(blob, p + 2 * c) as usize]; if sgn(c) { x = -x }; x * scale + offset }).collect(); p += 6 /* or 8, see note */; v }
                0x05 => { let v = vec![deq(u16(blob, p) as u32, 16, 0)]; p += 2 /* maybe +2 pad: check stride */; v }
                0x09..=0x11 => break,   // frame_count is 0 for these anyway
                other => { warn!("channel type {other:#x}"); break }
            };
            push_key(&mut tracks, key, sub, tick, vals);   // sub 1 => Vec3 translation, 2 => Quat(x,y,z,w).normalize(), 3 => scale
        }
    }
    Clip { name, duration: ticks as f32 * frame_dt, frame_dt, tracks }
}
```

### 2.5 Clip names and instance ids **[src: s3pi `FNVCLIP.HashString`, s3py `ClipResource.create_key`]**

TS3 hashing (all resources): **FNV-1** (multiply, then XOR — not FNV-1a) over the **lower-cased** bytes
of the name (s3pi's table lower-cases ASCII A–Z and Latin-1 À–Þ except × ). 32-bit: offset
`0x811C9DC5`, prime `0x01000193`. 64-bit: offset `0xCBF29CE484222325`, prime `0x100000001B3`.

CLIP instance ids are NOT a plain FNV-64 of the name. Algorithm:

```
fn clip_instance(name) -> (group, instance):
    value = name; mask = 0
    idx = name.find('_')
    if 0 < idx <= 5:
        prefix = lower(name[..idx])                      // actor/age prefix, e.g. "a", "c", "a2o", "c2a", "ad"
        i2 = prefix.find('2')
        if 0 < i2 <= 2:                                  // two-actor clip "x2y_..."
            x = prefix[..i2]; y = prefix[i2+1..]
            if y != "" && !(x in {a,o} && y in {a,o}):
                value = (x[0]=='o' ? "o" : "a") + "2" + (y[0]=='o' ? "o" : "a") + "_" + name[idx+1..]
                mask  = 0x8000 | AGE[x] << 8 | AGE[y]
        else if prefix not in {"a","o"}:                 // one-actor clip with non-adult prefix
            value = "a_" + name[idx+1..]
            mask  = 0x8000 | AGE[prefix] << 8
    inst  = fnv64(value) & 0x7FFF_FFFF_FFFF_FFFF
    inst ^= mask << 48
    group = if AGE[x] > 6 || AGE[y] > 6 { 0x48000000 } else { 0 }   // pets (s3py)
AGE = { b:1, p:2, c:3, t:4, h:5, e:6, ad:8, cd:9, al:0xA, ac:0xD, cc:0xE, ah:0x10, ch:0x11, ab:0x12, ar:0x13 }
      (b baby, p toddler, c child, t teen, h ?, e elder, ad adult dog, cd puppy, al adult little dog,
       ac adult cat, cc kitten, ah adult horse, ch foal, ab ?, ar ?)
```

Consequences: adult clips `a_...` / `a2o_...` / `o2a_...` have instance = `fnv64(name) & 0x7FFF...`
and group 0; the child version `c_foo` has the **same low 48 bits** as `a_foo`, which is how the game
finds age variants. Example: `fnv64("a_walk") = 11A06AB91BCA6BDE` → `a_walk` iid `11A06AB91BCA6BDE`,
`c_walk` iid `92A06AB91BCA6BDE`.

Practical tip: the S3CLIP blob contains its own clip name (`anim_name_offset`), so you can build a
name → TGI table by scanning all CLIP resources once (cache it) instead of guessing names.

### 2.6 Which clips to use for idle / walk

What is documented **[wiki/MTS]**:

- Clip names are `<actor-prefix>_<category>_<name>_<actor-letter>`: e.g. `a_idle_neutral_stretchArms_x`
  (quoted on MTS as an argument to `Sim.PlaySoloAnimation`), `a_dance_med_posAHipsShake_x`, `a2o_test_x`.
  `_x` / `_y` = which jazz actor the clip is for ("if an animation ends with `_x` or `_y` it is probably
  the one to use"); the outer CLIP's `actor_name` field holds that actor name.
- Fallback rule: a requested `t2c_foo` falls back to `a2a_foo` / `a2o_foo` / `a_foo` — so always try
  the adult `a_` clip if the age-specific one is missing (§2.5 hashing makes the low 48 bits identical).
- The Movie-Maker "standing idles" are named `neutral_loop, scratchArm, scratchNose, twistBody,
  scratchHead, stretchArms, rollShoulders, shiftWeight, rubNeck, lookRight, rightLeft, upAndOver, down`
  and mood idles `Sad, Depressed, Tense, Stressed, Uncomfortable, Miserable, Angry, Furious, Happy,
  Elated, VeryHappy` — strongly suggesting clips `a_idle_neutral_<name>_x` and
  `a_idle_<mood>_..._x` **[guess for the exact names]**.
- Walking is played by the engine's routing/locomotion system per *walkstyle* (Walk, FastWalk, Run,
  sneak, etc.), from jazz state machines (JAZZ 0x02D5DF13) — the walk-cycle clip names are **not
  documented online** that I could find. Walk-style mods edit "CLIP resources pertaining to the walk
  cycles" in FullBuild0 but don't publish names.

Computed instance ids (via §2.5) for names to try first:

| clip name | instance (group 0) | status |
|---|---|---|
| `a_idle_neutral_stretchArms_x` | `47D462559B7E2301` | name documented on MTS |
| `c_idle_neutral_stretchArms_x` | `C4D462559B7E2301` | child variant (same low 48 bits) |
| `a_idle_neutral_loop_x` | `3D7F317DED70332D` | guessed name |

**Recommended practical approach** (robust, no guessing): scan every CLIP (type 0x6B20C4F3) in
`FullBuild0.package` (+ EP packages / DeltaBuild), read only the S3CLIP header + `anim_name` string
(cheap: ~0x30 bytes + string), and build `name → TGI` (cache to disk). Then grep the names:
`*walk*` / `*run*` / `*_loop*` / `a_idle_*`. Prefer clips whose root/`b__ROOT__` translation track
advances linearly (a locomotion cycle) for walking, and play them looping while moving the entity
along the route at the speed implied by the root translation per cycle (or zero out the root
translation and drive movement yourself). The `src_name` (Maya file name) is also useful for grouping
start/loop/stop variants.

(If you only need *something* moving quickly: any `a_idle_*` clip demonstrates the whole pipeline;
for walking, a hand-made procedural swing of the thigh/calf/upper-arm bones is a viable placeholder.)

---

## 3. Verified data: the human rigs (new format) **[verified on real files]**

The [blender-sims3-geom](https://github.com/SmugTomato/blender-sims3-geom) add-on ships the game's rigs
as `io_simgeom/data/rigs/{auRig,cuRig,puRig,acRig,adRig,alRig,ahRig,ccRig,cdRig,chRig}.grannyrig`
(despite the extension they are the **new-format** RIG resources, extracted from game data). I parsed
`auRig` with the §1.2 layout and it consumed the file **exactly to the last byte**:

- `major=4, minor=2`, **257 bones** (cuRig and puRig: also 257, same names, different proportions;
  pet rigs 215 bones), skeleton name `"auRig"`, then 5 IK chains (L/R arm UpperArm→Forearm→Hand with
  pole `L_armExportPole`, L/R leg Thigh→Calf→Foot, and one for `b__ROOT_bind__`).
- Every bone's stored hash == FNV-32 of its name (lower-cased) → GEOM/CLIP hash lookups will work.
- All scales are 1. Flags values seen: 0x20–0x5A (meaning unknown; ignore).
- Composing `W = W_parent * T(pos) * R(xyzw)` gives sensible world positions: `b__ROOT_bind__`
  (0, 1.012, 0); `b__Head__` (0, 1.674, −0.003); `b__L_Foot__` (0.099, 0.111, −0.01);
  `b__L_Toe__` (0.102, 0.0, 0.106); `b__L_Hand__` (0.59, 1.094, −0.02).
  ⇒ **metres, +Y up, sim faces +Z, sim's left = +X**, bind pose is an A-pose. Same handedness/up axis as
  Bevy, so no conversion is needed.
- Rig names confirmed by the skeleton-name field: `auRig`, `cuRig`, `puRig`. That teen/YA/adult/elder
  all share `auRig` (with body differences coming from meshes/BOND/blend data) is my inference from
  there being no `tuRig`/`euRig` in the set **[guess]**; there is no baby rig in that set either.
- s3pi `RigResource.Parse` uses exactly this detection: `dw0 == 0x8EAF13DE && dw1 == 0` → wrapped
  Granny; `dw0 ∈ {3,4} && dw1 ∈ {1,2}` → new format; otherwise raw Granny. s3pi reads the skeleton name
  unconditionally and the IK chains only if `major >= 4` (matches the wiki, not s3py).

Bone hierarchy (index: name → parent index). Deform skeleton core:

```
0 b__ROOT__ (-1)            1 b__ROOT_bind__ (0)      2 b__Pelvis__ (1)
3 b__R_Thigh__ (2)  4 b__R_Calf__ (3)  5 b__R_Foot__ (4)  6 b__R_Toe__ (5)
26 b__L_Thigh__ (2) 27 b__L_Calf__ (26) 28 b__L_Foot__ (27) 29 b__L_Toe__ (28)
55 b__Spine0__ (1)  56 b__Spine1__ (55) 57 b__Spine2__ (56) 58 b__Neck__ (57) 59 b__Head__ (58) 60 b__HeadNew__ (59)
115 b__R_Clavicle__ (57) 116 b__R_UpperArm__ (115) 119 b__R_Forearm__ (116) 123 b__R_Hand__ (119)
149 b__L_Clavicle__ (57) 150 b__L_UpperArm__ (149) 153 b__L_Forearm__ (150) 157 b__L_Hand__ (153)
fingers: b__{L,R}_{Index,Mid,Ring,Pinky,Thumb}{0,1,2}__ under the hand
face (children of b__HeadNew__): b__Jaw__, b__JawComp__, b__Chin__, b__{Left,Right}Eye__ (+_mod, UpLid, LoLid),
  brows, lips, b__Tongue1/2__, b__NoseArea__..., b__{Left,Right}Ear__, b__HeadDome__
helpers: *_Compress__, *Twist__/*Untwist__/*Twisted__, b__{L,R}_Bicep__, b__{L,R}_Wrist__, b__{L,R}_breast__,
  b__belly__, b__upper_skirt__/b__lower_skirt__, many *_slot / *Target_slot bones (attachment points),
  b__carryGroup*_noBind__, and an IK/export subtree under b__ROOT_export__ (216): L/R_slotOffset,
  L/R_footOffset, world_offset, rootWorld, L/R_footWorld, L/R_slotInfo + L/R_Info1..10, L/R_footInfo*,
  Root_info, rootOffset, {L,R}_{arm,leg}ExportPole
```
Slots/IK/export bones carry no skin weights; keep them anyway (cheap) so bone indices match the rig.

---

## 4. Body meshes — `GEOM` 0x015A1849 **[src: s3pi MeshChunks/GEOM.cs, s3py BodyGeometry, SmugTomato geom_load.py; wiki]**

### 4.1 RCOL container (wrapper around GEOM, VPXY, MATD, MODL, MLOD, ...)

```
u32 version
u32 public_chunk_count
u32 unused
u32 external_count
u32 internal_count
ITG internal[internal_count]     // u64 instance, u32 type, u32 group  (ITG order!)
ITG external[external_count]
{u32 offset /*absolute*/, u32 size} chunk_info[internal_count]
<chunks at those offsets>
```
RCOL chunk references inside chunks are 1-based with flags in the top nibble (0x0… public internal,
0x1… private internal (+public count), 0x3… external). GEOM is the exception: it uses its own TGI list.

### 4.2 GEOM chunk (version 5 in TS3)

```
char[4] "GEOM"
u32 version                 // 5 (s3pi rejects anything else)
u32 tgi_offset              // relative to the position right after this field
u32 tgi_size
u32 embedded_shader         // 0, or FNV32 shader name: SimSkin = 0x548394B9, SimEyes = 0xCF8A70B4 (wiki)
if embedded_shader != 0:
    u32 mtnf_size
    MTNF block               // "MTNF", u32 0, u32 data_size, u32 param_count,
                             // param_count × {u32 name_hash, u32 type(1 float,2 int,4 texture), u32 count, u32 offset},
                             // then data. Texture params index the GEOM TGI list.
u32 merge_group
u32 sort_order
i32 vertex_count
i32 element_count
element_count × { u32 usage; u32 data_type; u8 byte_size }      // 9 bytes each
vertex_count × (elements in declared order, interleaved)
u32 face_point_size_count   // always 1
u8  face_point_size         // always 2 (u16 indices)
u32 index_count             // number of u16 indices (triangles = index_count / 3)
u16 indices[index_count]
i32 skin_controller_index   // legacy
u32 bone_count
u32 bone_hash[bone_count]   // FNV-32(lower(bone name)) -- these are the skin palette
TGI list (at tgi_offset): u32 count; count × {u32 type, u32 group, u64 instance}
```

Vertex element usages (all TS3 human meshes use float formats):

| usage | meaning | layout |
|---|---|---|
| 1 | position | 3 × f32 (model space, metres, Y-up, bind pose) |
| 2 | normal | 3 × f32 |
| 3 | UV | 2 × f32 — may repeat (pets have >1 UV set). **Flip V** for Bevy/Blender: `v' = 1 − v` (SmugTomato does `-v + 1`) |
| 4 | bone assignment | 4 × u8 — **indices into this GEOM's `bone_hash` array** (not rig indices) |
| 5 | weights | 4 × f32 (s3pi/s3py/SmugTomato all read floats) |
| 6 | tangent | 3 × f32 |
| 7 | colour / "TagVal" | 4 × u8 |
| 10 (0x0A) | vertex id | u32 — stable ids used by morphs (BGEO) and face sliders |

Skinning setup:

```
palette[k] = rig.index_of(fnv32 == geom.bone_hash[k])          // build once per GEOM
for each vertex: joints = [palette[a0], palette[a1], palette[a2], palette[a3]], weights = [w0..w3]
// unused influences have weight 0 (index may be garbage -- SmugTomato skips out-of-range ones)
skin_matrix[j] = bone_world_current[j] * inverse(bone_world_bind[j])   // positions are already in bind pose
```

Positions are in the same space as the rig's bind pose (the face mesh sits at ~1.6 m), so the inverse
bind matrices come straight from the rig (§1.2/§3). Normalise weights if they don't sum to 1.

Naming / ids: **GEOM instance = FNV-32 of the lower-cased mesh name, zero-extended to 64 bits**
(verified: `amFace_lod0_1/2/3` → `AF383E50/AF383E53/AF383E52`, `ymface_lod0_2` → `90BF60AB`, which are
the instances quoted on MTS; groups seen: `0x00117A4F`, `0x0020033C`). The face CASP has three GEOMs
per LOD — face, eyelashes, eyes (`*Face_lod0_1/_2/_3`; MTS doesn't say which is which; the eyes one
should have the `SimEyes` embedded shader). The female nude body mesh family is called `afBodyNude_*`,
and clothing creators clone `afTopNude` / `afBottomNude` (MTS threads). Exact suffixes are unverified —
use the package's NMAP (0x0166038C) to get names, or just follow CASP → VPXY → GEOM.

Morphs: CAS fat/fit/thin/special and face sliders are BGEO (0x067CAA11) vertex-delta resources
referenced from BBLN/FACE blend data (§7). Skip for a first pass (the base GEOM is the "average" body).

---

## 5. CAS parts — `CASP` 0x034AEECB **[src: s3pi CASPartResource.Parse (version 0x12), s3py CASPart; wiki]**

```
u32  version                        // 0x12 (18) in current data
u32  tgi_offset                     // TGI table at (offset of this field + 4) + value  (s3pi: value + 8)
u32  preset_count
preset_count × { i32 char_count; utf16le xml[char_count]; u32 unknown }    // CAS preset XML (see §6.3)
7bitstr name                        // part name, UTF-16 **BIG-endian**; length prefix = 7-bit-encoded BYTE count
f32  sort_priority                  // CAS sorts descending
u8   has_unique_texture_space       // (s3pi "unknown2"; s3py's name)
u32  clothing_type                  // body slot, table below  (s3py "body_type")
u32  data_type_flags                // Hair 1, Scalp 2, FaceOverlay 4, Body 8, Accessory 0x10
u32  age_gender_species_handedness  // see flags below
u32  clothing_category              // Naked 1, Everyday 2, Formal 4, Sleep 8, Swim 0x10, Athletic 0x20,
                                    // Singed 0x40, MartialArts 0x80, Career 0x100, FireFighting 0x200,
                                    // Makeover 0x400, SkinnyDippingTowel 0x800, Racing 0x1000, Jumping 0x2000,
                                    // Bridle 0x4000, Outerwear 0x40000, Plumbotwear 0x80000,
                                    // ValidForMaternity 0x100000, ValidForRandom 0x200000, IsHat 0x400000,
                                    // IsRevealing 0x800000, IsHiddenInCAS 0x1000000, pet region bits 25..28
u8   naked_casp_index               // TGI idx of the CASP shown when this part is removed (e.g. top -> afTopNude)
u8   parent_casp_index              // "base" part
u8   blend_fat_index, blend_fit_index, blend_thin_index, blend_special_index   // BBLN 0x062C8204
u32  overlay_priority               // draw/compositing layer (s3py "draw_layer")
u8 n; u8 vpxy_index[n]              // → VPXY (0x736884F1) = the mesh list (§5.2)
u8 n; LODInfo[n]:  { u8 level; u32 dest_texture; u8 m; m × { u32 sorting; u32 spec_level; u32 cast_shadow } }
u8 n; u8 diffuse_txtc_index[n]      // → TXTC 0x033A1435 (texture compositor) — primary
u8 n; u8 specular_txtc_index[n]
u8 n; u8 diffuse2_txtc_index[n]     // secondary set
u8 n; u8 specular2_txtc_index[n]
u8 n; u8 bond_index[n]              // → BOND 0x0355E0A6 (slot/bone adjust, §7.3)
7bitstr shoe_material               // UTF-16BE, e.g. "bare", "heel", "leath", "rub", "sand", "slip"
u8   tgi_count                      // <- tgi_offset points here
tgi_count × { u64 instance; u32 group; u32 type }    // **IGT** order
```

7-bit string: `len = 0; shift = 0; loop { b = u8; len |= (b & 0x7F) << shift; shift += 7; if b & 0x80 == 0 break }`
then `len` **bytes** of UTF-16BE (len/2 chars). (s3py's variant adds 7-bit groups without shifting —
identical for lengths < 128.)

`age_gender_species_handedness` (u32) **[src: s3pi AgeGenderFlags]**:

```
bits 0..6   age:     Baby 0x01, Toddler 0x02, Child 0x04, Teen 0x08, YoungAdult 0x10, Adult 0x20, Elder 0x40
bits 8..11  species: (dword >> 8) & 0xF  -- 0 or 1 = Human, 2 Horse, 3 Cat, 4 Dog, 5 LittleDog, 6 Deer, 7 Raccoon
                     (s3pi uses mask 0xCF00 to also cover boats/sim-walking-pets etc.; s3py treats 0 as human)
bits 12..13 gender:  Male 0x1000, Female 0x2000
bits 20..21 handedness: Left 0x100000, Right 0x200000
```

`clothing_type` (u32) **[wiki]**: 0 None, 1 Hair, 2 Scalp, 3 Face, 4 Body (full body), 5 Top, 6 Bottom,
7 Shoes, 8 FirstAccessory, 9 Necklace, 0x0A NoseRing, 0x0B Earrings, 0x0C Glasses, 0x0D Bracelets,
0x0E RingL, 0x0F RingR, 0x10 Beard, 0x11 Lipstick, 0x12 Eyeshadow, 0x13 Eyeliner, 0x14 Blush,
0x15 Makeup, 0x16 Eyebrow, 0x17 EyeColor, 0x18 Glove, 0x19 Socks, 0x1A Mascara, 0x1B Moles,
0x1C Freckles, 0x1D Weathering, 0x1E EarringL, 0x1F EarringR, 0x20 ArmBand, 0x21 Tattoo,
0x22 TattooTemplate, 0x23 Dental, 0x24/0x25 Garter L/R, 0x26 BirthMark, 0x27–0x2E body hair,
0x2F PetBody … 0x3B PetBeard, 0x3C Last.

CAS geom flags (LODInfo asset flags) **[wiki]**: 0x1 Mergeable, 0x2 IncludeMorphs, 0x4 IncludeTweaks,
0x8 IncludeTangents, 0x10 FourBoneSkinning, 0x20 TwoBoneSkinning, 0x40 TwoQuatSkinning,
0x80 OneQuatSkinning, 0x100–0x2000 SpecLevel0..5, 0x4000 Sorted, 0x8000 ShadowCaster.

### 5.1 Finding the default (nude) body, head, scalp

A naked sim consists of (one per slot, filtered by age/gender bits): **Top + Bottom + Shoes** nude
parts *or* a **Body** (full-body) part, plus **Face** (head mesh incl. eyes & lashes), **Scalp** (under
hair), and optionally Hair/Eyebrows. Recipe:

1. Index every CASP once (they're in `FullBuild0.package` and EP/SP packages); for each record
   `(name, clothing_type, age_gender, clothing_category, vpxy, txtc...)`.
2. Nude parts = `clothing_category & 0x1 (Naked)`, matching age/gender bits, `clothing_type ∈ {4,5,6,7}`.
   Names follow `<age><gender><Slot>Nude`: `afTopNude`, `afBottomNude`, `afShoesNude`, `amTopNude`, …
   (`a`=adult — YA/teen/elder mostly reuse adult parts; `f`/`m`/`u` = female/male/unisex; `c`/`p` child/toddler).
   Every clothing CASP's `naked_casp_index` also points at the matching nude part — an easy cross-check.
3. Face: `clothing_type == 3` for the age/gender (meshes `afFace_lod0_{1,2,3}`, `amFace_lod0_*`,
   `cuFace_*`, `puFace_*`, `ef/em/tf/tm/yf/ymFace_*`). Scalp: `clothing_type == 2` (`afScalp`, …).
4. CASP instance ids appear to be FNV-64 of the part name (s3oc/TSRW clone convention) **[guess]**:
   `afTopNude` → `78CE86CE40987A8F`, `afBottomNude` → `C1E55AEF1301DA97`, `afShoesNude` → `E9952DBC65F47892`,
   `amTopNude` → `C17093510E4B1F52`, `amBottomNude` → `1A76E24B5B8B9818`, `amShoesNude` → `73D9C6F643AB11A3`,
   `afBodyNude` → `94F17A5995AC8BDE`, `amBodyNude` → `B8514408AB8A82FD`. If these miss, fall back to
   scanning CASPs and filtering by fields (step 2) — that is robust regardless of naming.

### 5.2 CASP → meshes: VPXY 0x736884F1 **[wiki]**

RCOL with one `VPXY` chunk:

```
char[4] "VPXY"; u32 version (4); u32 tgi_offset; u32 tgi_size
u8 entry_count
entry_count × { u8 kind;
    kind 0: u8 lod; u8 n; u32 tgi_index[n]     // GEOMs for this LOD (face lod 0 → face, lashes, eyes)
    kind 1: u32 tgi_index }
u8 0x02; f32 bbox[6]; u8[4] unused; u8 modular; if modular == 1 { u32 ftpt_index }
TGI list: u32 count; {u32 type, u32 group, u64 instance}
```
Use the `kind 0, lod 0` entry for full quality. **[verified]** Not every part has one: the
baby bodies (`buBody`, `bfBody`) start at **lod 1**, whose entry lists two GEOMs: the
swaddling blanket (309 vertices) and the baby itself (886 vertices, head and hands included).
Fall back to *all* meshes of the lowest LOD present, not just its first. The `kind 1`
entries point at BOND (0x00AE6C67) bone deltas and the part's own rig (0x8EAF13DE).

Babies have no face, scalp or hair parts: the face is painted on the baby skin-tone texture
(TONE entry for age 0x01, types 4 and 8 share one 256×256 texture with eyes and mouth).

### 5.3 Carrying babies and toddlers **[verified on clips]**

- Adult rigs have `b__carryGroupOffset_noBind__` → `b__carryGroup_noBind__` →
  `b__L_carry_slot` / `b__R_carry_slot` under `b__Spine1__`, all at zero offset; carry clips
  (`a2b_*_x`) animate only the two group bones' rotations (about ±11°, cancelling Spine1's
  bend), so the slot sits upright at the upper chest.
- The baby clip of a pair (`a2b_*_y`, rig `buBody`: `b__ROOT__` → `offsetBone` →
  `transformBone` → `b__waist__` …) keeps the root still and puts a small offset and a large
  rotation on `transformBone`, relative to the slot. Applied to the upright slot as-is, the
  baby hangs head-down: the game also IK-solves the arms (the adult clips carry
  `L_slotOffset` / `R_slotOffset` hand targets under `b__ROOT_export__`), and the arm tracks of
  feeding / changing clips only make sense with that IK. The idle carry clips
  (`a2b_idle_carry_*_x`) hold one forearm out palm-up, which makes a usable cradle.
- Toddler pair clips (`a2p_*_y`, rig `puRig`) are in the toddler's own frame instead: the
  grown-up stands about 0.6 m in front, facing the toddler; `b__ROOT_bind__` goes from the
  floor (y 0.033 — toddlers sit on the floor when idle, `p_idle_breathe_x`) up to
  (0.17, 1.22, 0.60) when held (`a2p_pickUp_y`, `a2p_carry_chat_loop*_y`).

---

## 6. Skin and clothing textures

### 6.1 What the game does (compositor) **[wiki: Sims_3:Texture_Layering, TXTC page; MTS skin tutorials]**

The game *composites* one texture per "texture space" per sim at runtime (cached in
`Documents/Electronic Arts/The Sims 3/simCompositorCache.package`). Body parts share one body UV space
(top/bottom/shoes/body meshes all map into the same atlas); the face/head has its own. Inputs:

1. **Skin** from the sim's skin-tone resource (TONE 0x0354796A): per age/gender/part (body, face, scalp)
   "detail" textures + a **tone ramp** coloured by the sim's skin-tone slider.
2. Each worn CASP's **preset**: a "complate" (compositing template — an XML resource 0x0333406C, e.g.
   `CasRgbaMask`, instance = `FNV64("CasRgbaMask")` = `E37696463F6B2D6E`) plus parameter values (texture
   keys and colours) from the CASP preset XML. Some parts use binary TXTC (0x033A1435) instead of XML.
3. Makeup/face overlays (CASPs with clothing types 0x11–0x15 etc.) composited onto the face texture.

Render targets are 1024×1024 (wiki).

### 6.2 Skin tone — TONE 0x0354796A **[wiki v4/v6; src: s3py SkinTone]**

```
u32 version              // 4, or 6 after patch 1.17 (muscle/cleavage normals)
u32 tgi_offset; u32 tgi_size     // key table (u32 count; {type, group, u64 instance})
u32 n_shader_keys
n × { u32 age_gender; u32 edge_color_argb; u32 specular_color_argb; f32 specular_power; u8 is_genetic }
u32 tone_ramp_index          // → DDS (0x00B2D882 in FullBuild2) with the same instance as a PNG 0x2F7D0004 in FullBuild0
u32 sub_skin_ramp_index      // → DDS (wiki and s3py disagree on which of these two comes first — check data)
u32 n_texture_keys
n × { u32 age_gender; u32 type_flags;              // type_flags presumably DataTypeFlags (Scalp 2 / Face 4 / Body 8) [guess]
      u32 specular_idx, detail_dark_idx, detail_light_idx, normal_idx, overlay_idx;
      if version >= 6 { u32 muscle_normal_idx, cleavage_normal_idx } }
u8 is_dominant
```

Facts from MTS tutorials: `FullBuild0` has **7 TONE resources** — 6 default skin tones + an unused
"mannequin"; hair tone is a separate type 0x03555BA8. The **tone ramp** is "what the game looks at when
you move the slider": a small gradient image (tutorials use 64×64), **lightest at the left, darkest at
the right**; the ramp DDS (FullBuild2) and PNG (FullBuild0) share an instance (e.g. `AlienSkinToneRamp`).
The detail ("multiplier") textures are DXT3/DXT5 DDS, separate **face** and **body** textures per
age/gender, each with a **light and a dark** version (= TONE `detail_light` / `detail_dark`);
"EF, EM, YAM, YAF, TF, and TM all share the AF/AM body multiplier textures" and "TF and TM share the
YAM/YAF face textures". "The game 'overlays' another colour on top of your multiplier layer."

**Simplest viable skin** (my recommendation; the blend math is a guess):

```
t = skin_tone_slider (0..1, e.g. 0.5)
detail = lerp(sample(detail_light), sample(detail_dark), t)   // or just detail_light to start
tint   = sample(tone_ramp, u = t, v = 0.5)
albedo = overlay(detail, tint)   // Photoshop overlay: d<0.5 ? 2*d*c : 1-2*(1-d)*(1-c);  try multiply-2x as alt
```
Pick the TONE texture key whose `age_gender` matches (adult bits for teen..elder) and whose
`type_flags` selects body vs face vs scalp; use the body texture on body GEOMs and the face texture on
the face GEOM. Even `detail_light` alone as albedo should already look like skin.

### 6.3 Clothing — CASP preset XML + complate **[wiki: Texture_Layering]**

Preset XML (UTF-16, inside the CASP):

```xml
<preset>
 <complate name="CasRgbaMask" reskey="key:0333406C:00000000:E37696463F6B2D6E">
  <value key="Overlay"    value="key:00B2D882:00000000:F9F07373B76D4042" />
  <value key="Mask"       value="key:00B2D882:00000000:6A27D9DFE5206BCB" />
  <value key="Multiplier" value="key:00B2D882:00000000:F9F07373B76D4040" />
  <value key="Skin Specular" value="key:00B2D882:00000000:4DB46D1662895FDD" />   <!-- = FNV64("amBody_s") -->
  <value key="Skin Ambient"  value="key:00B2D882:00000000:4DB46D1662895FCF" />   <!-- = FNV64("amBody_a") -->
  <value key="Part Mask"     value="key:00B2D882:00000000:B6E46F5107C8FC74" />   <!-- = FNV64("amTopMask") -->
  <pattern name="solidColor_1" reskey="key:0333406C:00000000:71D5EFB6C391BC17" variable="Pattern A">  <!-- FNV64("solidColor_1") -->
    <value key="Color" value="0.4455,0.1011,0.1011,1.0000" />
  </pattern>  ... Pattern B/C/D, Logo, Stencil A..F, tiling/rotation ...
 </complate>
</preset>
```
Texture instance ids are **FNV-64 of the texture's base file name** (verified on the keys above:
`amBody_s`, `amBody_a`, `amTopMask`; and Multiplier/Overlay/Specular of one part differ only in the
last byte because FNV-1 XORs the last char: `_m`, `_o`, `_s`). Group 0, type DDS 0x00B2D882.

`CasRgbaMask` diffuse recipe (wiki steps, condensed):

```
B = white
for p in A..D: alpha(B) = Mask.channel(p); B.rgb = lerp(B.rgb, Pattern_p (tiled, default 4x4), alpha(B))
B.rgb = lerp(B.rgb, Overlay.rgb, Overlay.a)
B.a   = Multiplier.a ; B.a += Overlay.a
B.rgb = 2 * B.rgb * Multiplier.rgb          // srcBlend=DestColor, dstBlend=SrcColor  => "modulate 2x"
(stencils A..F alpha-blended on top)
A (the sim's texture, already containing skin) = lerp(A, B, B.a)
```
So a clothing part is drawn *over the skin* using the multiplier's alpha as coverage. Simplest viable
version: `albedo = skin`, then `albedo = lerp(albedo, 2 * pattern_A_colour * Multiplier.rgb, Multiplier.a)`.
Specular target: R = ambient occlusion (skin/clothing), B = specular, A = clothing specular.

TXTC 0x033A1435 (binary compositor, used by some parts and referenced from CASP `diffuse*_txtc`):
version, offset, embedded TXTCs (v≥7), pattern size, part type, entries of `{u32 property_id (FNV32 of
name); u8 0; u8 data_type; data}` terminated by property 0 — same step semantics as the XML complate
(property ids e.g. 0x687720A6 "ID"/step type, 0x8A7006DB image source, 0xA2C91332 render target,
0x048F7567 destination blend, 0xB01748DA colour, 0xB67C2EF8 HSV shift). Only needed if a part has
no usable preset XML.

---

## 7. Sim outfits (SIMO 0x025ED6F4), blend data (FACE/BBLN), BOND

### 7.1 SIMO — sim outfit **[src: s3pi SimOutfitResource.Parse; wiki]**

A saved/premade sim's outfit: which CASPs + texture compositors + sliders + skin tone. Layout for
version ≥ 0x08 (current data uses up to 0x15):

```
u32 version
u32 tgi_offset                         // from after this field
if v >= 0x10: u32 n; n × { u8 ?; i32 chars; utf16le xml[chars] }   // presets
elif v >= 0x09: u32 n; u32[n]
if v >= 0x0E: u32 ?, u32 ?
f32 heavy_weight_slider, strength_slider, slim_weight_slider
if v >= 0x09: u32 ?
u32 age; u32 gender; u32 species       // each an AgeGenderFlags dword
if v >= 0x09: u32 handedness
skin_tone_index: v >= 0x15 ? i16 : u8  // → TONE in TGI table
if v == 0x08: u8 hair_tone_index
f32 skin_tone_slider                   // ← the ramp coordinate for §6.2
if v >= 0x09:
    if v >= 0x0E:
        if v >= 0x11: f32 muscle; if v >= 0x12: f32 breast
        u32 hair_base_argb, hair_halo_high_argb, hair_halo_low_argb
        if v >= 0x13: f32 num_curls, curl_pixel_radius; if v >= 0x14: TGI fur_map
    else: u8 ?
else: u32 ?
u8 casp_count
casp_count × { (v >= 0x15 ? i16 : u8) casp_index; if v >= 0x0E: u32 clothing_type;
               u8 n; n × { idx txtc1, idx txtc2 } }   // idx = i16 if v >= 0x15 else u8
u8 0
u8 face_count; face_count × { idx face_part (→ FACE 0x0358B08A); f32 amount }
if v < 0x0A: u32 ?
tgi table: (v >= 0x15 ? i16 : u8) count; count × { u64 instance; u32 group; u32 type }   // IGT
```
You don't need SIMO to render a default sim (pick CASPs directly), but it is how household/premade
sims specify outfits (and the face-slider amounts).

### 7.2 FACE 0x0358B08A / BBLN 0x062C8204 — blend (slider) definitions **[wiki; src: s3py BlendData]**

```
u32 version                  // 7 (8 for breast slider; 10 Medieval)
u32 tgi_offset; u32 tgi_size // v8: size has a +8 quirk
7bitstr part_name            // UTF-16BE
if version == 8: TGI bgeo_key   // → BGEO 0x067CAA11
u32 entry_count
entry_count × { u32 facial_region_flags;        // Eyes 1, Nose 2, Mouth 4, TranslateMouth 8, Ears 0x10,
                                               // TranslateEyes 0x20, Face 0x40, Head 0x80, Brow 0x100,
                                               // Jaw 0x200, Body 0x400, Eyelashes 0x800
                u32 n_geom; n × { u32 age_gender; f32 amount; u32 tgi_index }   // → VPXY of morph GEOMs
                u32 n_bone; n × { u32 age_gender; f32 amount; u32 tgi_index } } // → bone morphs
TGI list
```
A slider = a set of morph meshes (vertex deltas keyed by GEOM vertex id) and bone deltas, applied
with `amount * slider_value`.

**Body shape [verified, implemented]:** a CASP names four BBLNs right after its naked/parent
indices (u8 key indices: fat, fit, thin, special). Body BBLNs are **version 8**: after the part
name come `u32 (2)` and the TGI (type, group, u64 instance) of a BGEO; the entry (region 0x400
Body, one geom ref) points at a null key — the deltas are in the BGEO.

BGEO 0x067CAA11 **[verified]**:
```
char[4] "BGEO"; u32 version (0x300); u32 blends; u32 lods (4)
u32 total_vertices; u32 total_vectors
u32 blend_header_size (8); u32 lod_entry_size (12)
u32 blends_offset; u32 vertices_offset; u32 vectors_offset     // from the file start
blends × { u32 age_gender; u32 region;
           lods × { u32 first_vertex_id; u32 vertex_count; u32 vector_count } }
total_vertices × u16      // per vertex id (first_vertex_id + k), LOD after LOD
total_vectors × 3×u16     // (u ^ 0x8000) as i16 / 2000
```
Each vertex u16: bit 0 = has a position delta, bit 1 = has a normal delta, bits 2..15 = a
signed step added to a running index (not reset between LODs) into that LOD's vectors; the
position delta is at the index, the normal delta right after it. Vectors are shared between
vertices. Vertex ids are GEOM vertex element usage 10 (u32); LOD 0 meshes use ids from the
BGEO's second LOD block (the first is empty), so match by id range, not LOD number. Deltas run
up to ~12 cm (heavy) and ~3 cm (fit, thin).

### 7.3 BOND 0x0355E0A6 — bone/slot adjust **[wiki; src: s3py BoneDelta]**

RCOL chunk **without** a 4-byte tag:

```
u32 version        // 3 seen
u32 count
count × { u32 bone_hash (FNV32 name);
          f32 offset[3]; f32 scale[3]; f32 quat[4] /*x,y,z,w*/ }
```
Per-outfit (CASP `bond_index`) or per-slider tweaks of bone/slot local transforms. s3py applies them
additively in Blender pose space: `loc += offset; scale += scale_delta; rot += quat` (approximate).
Use only for accessory slot alignment / body-shape sliders; ignore initially.

---

## 8. Minimal implementation path (recommended order)

1. **Hashing utils**: FNV-1 32/64 with TS3 lower-casing; `clip_instance(name)` (§2.5).
2. **RIG**: implement the new format (§1.2) first — ~40 lines. Look the rig up by FNV-64 name in
   DeltaBuild/EP packages; test against the expectations in §3 (257 bones, `b__Head__` ≈ (0, 1.674, 0)).
   Only if the user's install has no new-format copy, implement GR2: container (§1.4) → Oodle1 (§1.5)
   → type-tree walker (§1.6) → `Skeletons[0].Bones` (§1.7). Unit-test Oodle1 by checking that the
   decompressed section's relocations/type tree make sense (member names are readable ASCII such as
   `"ArtToolInfo"`, `"Skeletons"`, `"Bones"`, `"ParentIndex"`).
3. **CASP index**: parse CASPs (§5), keep `(name, clothing_type, age_gender, category, vpxy, presets)`;
   pick `afTopNude/afBottomNude/afShoesNude` (or Body) + `afFace` + `afScalp` style parts (§5.1).
4. **Meshes**: CASP → VPXY (lod 0) → GEOMs (§4, §5.2). Build Bevy `Mesh` with `ATTRIBUTE_JOINT_INDEX`
   remapped through `bone_hash` → rig index and `ATTRIBUTE_JOINT_WEIGHT`; flip V. Inverse bind = inverse
   of rig world bind matrices; `SkinnedMesh` joints = one entity per rig bone (keep the hierarchy).
5. **Textures**: TONE → `detail_light` body/face DDS as albedo (§6.2); later add the ramp tint, then the
   clothing multiplier composite (§6.3).
6. **CLIP**: parse outer + S3CLIP (§2.1–2.4); convert to per-bone keyframe tracks (ticks × 1/30 s);
   apply as local transforms on the bone entities with lerp/slerp. Start with any `a_idle_*` clip; find
   walk clips by scanning names (§2.6).

---

## 9. Unknowns / things to verify on the user's data

- **Does `DeltaBuild0.package` contain new-format copies of `auRig`/`cuRig`/`puRig`?** (wiki says yes;
  the rigs shipped with blender-sims3-geom prove the game has new-format human rigs somewhere.) Also
  confirm the rig instance = FNV-64(name) and its group.
- Oodle1 literal-context detail: nwn2mdk uses the absolute output pointer `% 4`; I assume the section
  buffer is 4-aligned so this equals `pos % 4`. Irrelevant for 1-section files if the Rust buffer is
  aligned/you use `pos % 4`.
- GR2 v6 `granny_bone` member set for TS3's Granny version (tag `0x8000001C`): with/without
  `LODError`, `LightInfo`, `CameraInfo` — the type tree answers it; and whether `InverseWorldTransform`
  loads directly as a column-major `Mat4`.
- S3CLIP: exact per-key payload padding for indexed `0x03` channels (6 vs 8 bytes; use the next curve's
  offset to tell); meaning of key-flag bits 4..15; outer-header `unknown1/unknown2`; whether
  `b__ROOT__` carries root motion in walk clips; frame rate is assumed 30 fps (`frame_duration`).
- Walk/run clip names — not documented online; get them by scanning CLIP names.
- TONE: order of the two ramp indices (wiki: tone ramp then sub-skin ramp; s3py: sub-skin then tone),
  meaning of `type_flags`, and the exact blend the game uses for ramp × detail (I suggest an overlay /
  2× multiply; tune visually).
- CASP instance = FNV-64(part name) is a convention, not verified for EA parts — filter by fields instead.
- Which of `*Face_lod0_1/_2/_3` is face vs lashes vs eyes (check embedded shader / vertex counts).
- BGEO morph decoding and BOND application are only sketched (low priority).

---

## 10. Links

Reader source code:
- s3pi wrappers (CLIP, FNV, CLIP hashing): https://github.com/mattparizeau/s3pi-wrappers
  (`s3piwrappers.AnimationResources/ClipResource.cs`, `s3piwrappers.Helpers/Cryptography/FNVCLIP.cs`, `FNVHash.cs`)
- s3pi core library source (CASP, SIMO, GEOM, RIG detection): https://github.com/marcos4503/sims3-package-interface
  (`S3PI-Library-DLLs-Source/s3pi Wrappers/CASPartResource/*.cs`, `MeshChunks/GEOM.cs`, `RigResource/RigResource.cs`);
  original: https://sourceforge.net/projects/s3pi/
- s3py (Sims 3 CLIP Tool backend): https://github.com/garthand/s3py (`s3py/animation/__init__.py`, `animation/rig.py`,
  `animation/blender.py`, `cas/catalog.py`, `cas/geometry.py`); tool site: https://sims3cliptool.wordpress.com/
- Sims4Tools S3CLIP (EA channel/sub-target enum names): https://github.com/s4ptacle/Sims4Tools/blob/master/s4pi%20Wrappers/AnimationResources/S3CLIP.cs
- blender-sims3-geom (GEOM import, new-format rig files): https://github.com/SmugTomato/blender-sims3-geom
- nwn2mdk (Oodle1 + GR2 v6): https://github.com/Arbos/nwn2mdk/blob/master/nwn2mdk-lib/gr2_decompress.cpp ,
  `gr2_file.cpp`, `gr2.h`
- opengr2: https://github.com/arves100/opengr2 (`libopengrn/oodle1.c`), format notes: https://github.com/arves100/opengr2/wiki/File-Format-documentation
- xoreos-tools granny decoder (origin of the Oodle1 port): https://github.com/berenm/xoreos-tools/blob/wip/granny-decoder/src/decompress.cpp
- LSLib GR2 reader: https://github.com/Norbyte/lslib/tree/master/LSLib/Granny/GR2

MTS / SimsWiki pages:
- RIG https://modthesims.info/wiki.php?title=Sims_3:0x8EAF13DE
- CLIP https://modthesims.info/wiki.php?title=Sims_3:0x6B20C4F3
- CASP https://modthesims.info/wiki.php?title=Sims_3:0x034AEECB
- SIMO https://modthesims.info/wiki.php?title=Sims_3:0x025ED6F4
- FACE (also BBLN 0x062C8204) https://modthesims.info/wiki.php?title=Sims_3:0x0358B08A
- BOND https://modthesims.info/wiki.php?title=Sims_3:0x0355E0A6
- GEOM https://modthesims.info/wiki.php?title=Sims_3:0x015A1849
- TONE https://modthesims.info/wiki.php?title=Sims_3:0x0354796A
- VPXY https://modthesims.info/wiki.php?title=Sims_3:0x736884F1
- TXTC https://modthesims.info/wiki.php?title=Sims_3:0x033A1435
- BGEO https://modthesims.info/wiki.php?title=Sims_3:0x067CAA11
- RCOL https://modthesims.info/wiki.php?title=Sims_3:RCOL , MATD/MTNF https://modthesims.info/wiki.php?title=Sims_3:0x01D0E75D
- Texture layering (preset/complate) http://simswiki.info/wiki.php?title=Sims_3:Texture_Layering
- Object rigs after 1.26 https://modthesims.info/wiki.php?title=Tutorial:Fixing_Object_RIGs_for_Patch_1.26

MTS threads used for names/skin facts:
- Template body/face/scalp meshes + skin multipliers (naming): https://modthesims.info/showthread.php?t=411795
- Default replacement skintones (shared multipliers, DXT3): https://modthesims.info/showthread.php?t=351328
- TONE files / tone ramps (7 TONEs in FullBuild0, ramp DDS+PNG share instance): https://modthesims.info/d/showthread.php?t=383795
- Face mesh instances (`amFace_lod0_1..3` = `0xAF383E50/53/52`): https://modthesims.info/t/442374
- afBodyNude / afTopNude / afBottomNude: https://modthesims.info/t/519909 , https://modthesims.info/t/482467
- Movie-maker idle names: https://narisims.tumblr.com/post/104047381651/movie-maker-cheats-guide-specific-looping-idles

## Premade households (world `OBJS`, 0x06B981ED)

A world file carries the scripts' saved object graph: every household, Sim description,
relationship and career of the town, as serialized by the game's `ScriptCore`. Decoded in
`crates/s3formats/src/objs.rs`; the households in `premade.rs`.

* Header: `u16 version, u16 0, "OBJS", u32 classes, u32 objects, u32 class-table offset,
  u32 object-offset table, u32 key table`.
* Class table: per class a type descriptor (`flags` byte; `0x20` generic = nested descriptor of
  the generic definition followed by its arguments; `0x40` array and `0x02` wrapper nest one
  descriptor; otherwise a length-prefixed name), then the field count (`0x7F` = none) and
  fields (name, type code). Lengths and counts are one byte, or two when the high bit is set
  (`(b & 0x7F) << 7 | next`).
* Objects: `0x10, u32 class` then field values; arrays `0x11, u32 count, 0x10 u32 element
  class, u32 refs`. References are 1-based object indices. `List<T>` holds one reference to
  its backing array; `Dictionary<K,V>` is `count` then key/value pairs.
* Field type codes: `01` reference, `02` bool, `03/04` byte, `06–08` 16-bit, `09` i32,
  `0E` u32, `0C` f32, `05/0A/0B/0D/0F` 64-bit, `10` struct (`u32 class` + fields), `17` enum
  (class, value type code, value), `19` resource key (index into the key table).
* Names are localization keys (`Gameplay/Excel/PV/Sims:Gunther`): FNV-64 of the key is the
  string-table id. Traits and skills are the game's enum values (`TraitNames`, `SkillNames`,
  tabled in `crates/s3formats/src/enums.rs`). Age and gender are `CASAgeGenderFlags`
  (`0x1` baby … `0x40` elder, `0x1000` male, `0x2000` female).
* Household portraits: PNG resources `0x6B6D837E` keyed by household id.
* The Sims' outfits (`SIMO` keys) aren't shipped in the game's packages, so clothes are
  picked from the CAS catalogue; natural hair colour (`GeneticColor`), skin shade and body
  shape come from the description.
