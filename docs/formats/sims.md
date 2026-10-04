# Sims (CAS) rendering & animation — format notes

Research notes for rendering and animating Sims from the user's installed TS3 data.
Status: **work in progress** (written incrementally). Each claim is tagged:

- **[src]** — transcribed from reader source code that is known to work (s3pi, s3py, nwn2mdk, LSLib, ...)
- **[wiki]** — from the Mod The Sims (MTS) wiki / SimsWiki; usually right, sometimes stale
- **[guess]** — my inference; verify against real data before relying on it

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
Candidate names (s3py `CASPart.get_rig()` builds `"%s%sRig" % (age_char, species_char)` with age
`a/b/c/e/t/p` and species `u` human, `c` cat, `d` dog/little dog, `h` horse/deer, `r` raccoon). Real
data, I believe, only has `auRig` (teen..elder share it), `cuRig`, `puRig` (toddler), `buRig` (baby) for
humans **[guess]**:

| name | FNV-64 (lower-cased) | FNV-32 |
|---|---|---|
| `auRig` | `D057FCC534C1BCBB` | `43AC09FB` |
| `cuRig` | `2183C4B327C38FC9` | `9E5E5F09` |
| `puRig` | `192F5C47F5D28A72` | `585CC6B2` |
| `buRig` | `193F4CBE84707158` | `C6868098` |

(Check these against the RIG instance list in FullBuild; if they don't match, list RIG instances and
look for a name table — `0x0166038C` NMAP — in the same package.)

**Object rigs vs sim rigs:** both use type 0x8EAF13DE. Object rigs (referenced from an object's VPXY /
MODL via the RCOL chain, typically group != 0, names like `<object>_rig`) are small (a few bones,
`b__ROOT__`, slots, `_IK_` helpers); sim rigs are the `?uRig` skeletons with ~100+ joints including
slider/compress/twist helper bones. Same container format, same reader. The S3 RIG Maker / RIGfix tools
mentioned on MTS deal with object rigs after patch 1.26 broke old-format object rigs
([tutorial](https://modthesims.info/wiki.php?title=Tutorial:Fixing_Object_RIGs_for_Patch_1.26)).

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
    u32 flags              // usually 0x23; 0x3F seen on mirrored bones [wiki]
i32 skel_name_len ; char skel_name[...]   // wiki: always present (v3 => length 0);
                                          // s3py reads it only if major >= 4 -- verify
if major >= 4:                            // wiki; s3py reads chains unconditionally
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
                0x09..=0x11 => constant_for(chan),
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

(Walk/idle clip names: see §2.6, to be filled in.)
