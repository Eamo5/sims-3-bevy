# Audio

Decoded in `crates/s3formats/src/audio.rs`, baked by `crates/s3bake/src/sounds.rs`, played by
`crates/game/src/sound.rs`.

## How a sound is found

1. Something asks for a sound by name — an animation clip's event table, or the game code
   (`ui_piemenu_primary`, `sting_career_positive`, `music_mode_buy`, `stereo_pop` …).
2. **Sound properties** `0x8070223D`, instance = FNV-64 of the lowercased name. Several
   packs can carry the same instance (each expansion adds songs to `stereo_pop`); merge them.
3. The record's sample list (property `0x701ED91E`) names the **variants**, played at random.
   A record without samples inherits them (and gain, distances, looping) from its parent
   (`0x5F6317D5`), e.g. `foot_drag_cloth_sand` → `foot_drag_lino_sand`.
4. Each sample is an **SNR** resource `0x01A527DB` (same instance); streamed samples keep their
   data in an **SNS** resource `0x01EEF63A` of the same instance.

Name maps (`0x0166038C`) in the audio packs give names for most instances (`u32 version,
u32 count, {u64 instance, u32 len, chars}`).

### Sound property records (big-endian)

`u32 count`, then per property `u32 name hash, u16 type, u16 flags, value`. Flags with bits
`0x0C` set mean an array: `u32 count, u32 element size, elements`. Types: `0x01` bool (1 byte),
`0x0A` u32, `0x0D` f32, `0x13` UTF-16 string (`u32 chars` + chars), `0x3E8` resource key (16
bytes, the first 8 the little-endian instance).

| hash | meaning |
|---|---|
| `0x701ED91E` | sample keys |
| `0x5F6317D5` | parent record |
| `0x25DF0108` | linear gain |
| `0x0F616B72` | full-volume distance (m) |
| `0x03593710` | fade-out distance (m) |
| `0x6DD08218` | looping |
| `0x09D0B38E`, `0x1E4DB1EB` | falloff curves (float arrays) |

## Clip sound cues

A CLIP's event table starts at `u32 @0x18` + 0x18: `"=CE=", u32 version (0x103), u32 count,
u32 size, u32 ?`. Each event: `u16 type, u16 ?, u32 id, f32 time, f32 -1, f32 -1, u32 ?,
u32 name length, name (padded to 4)`, then a type-specific payload (sound: a 128-byte name
buffer; effect: 24 bytes; script: none). Events are easiest found by the two `-1.0` floats.

* Type 3 (sound): the payload names the sound, e.g. `bed_getin_made`, `foot_step`.
* Type 4 (script): `play_sound_X`, `start_looping_sound_X`, `stop_looping_sound_X`, where X
  lists price tiers: `a_chp__cheap___a_norm__normal___a_exp__expensive`.

Cue names are completed by context:

| cue | concrete sound |
|---|---|
| `vo_…a` (adult voice) | `+ _fa/_fb/_fc` (female, three pitches) or `_ma/_mb/_mc` |
| `vo_…c` (child voice) | `+ _ca/_cb/_cc` |
| `foot_step`, `foot_slide`, … | `+ _<surface>_<shoe>`; surfaces `wood cpet lino marble cment grass gravel dirt`, shoes `rub bare leath heel sand slip` |
| `sit_sofa_…` | `+ _cloth/_leath/_wood/…` (seat material) |
| a loop start without `_lp` | `+ _lp` |

## SNR / SNS

SNR header (big-endian): `u32 version:4 codec:4 channels-1:6 sample_rate:18`,
`u32 type:2 loop:1 samples:29`, then `u32 loop start` if looping (streams add a loop offset).
Type 0 keeps the sample in memory: blocks follow the header. Type 1 streams it from the SNS.

Blocks: `u8 flags (0x80 = last), u24 size (including this header), u32 samples`, data.

Codecs in The Sims 3: **4 = EA-XAS v1** and **5 = EALayer3 v1** (version 0 headers use the
"v1b" PCM-block layout); 2 = big-endian PCM.

### EA-XAS v1

Per channel, 0x4C-byte frames of 128 samples, interleaved channel by channel. Four groups:
`u32 LE header` each (low 16 bits: history-2 `& 0xFFF0` with the coefficient index in the low
nibble; high 16 bits: history-1 `& 0xFFF0` with the shift in the low nibble), then 15 rows of
4 bytes (one byte per group, high nibble first). Coefficients ×256: `(0,0) (240,0) (460,-208)
(392,-220)`; `s = ((nibble << 28 >> 28) << 12 >> shift); out = ((s << 8) + h1*c1 + h2*c2 +
128) >> 8`, clamped.

### EALayer3 v1(b)

A block is a run of MSB-first bit-packed granules:

```
u8  flag            0x00, or 0xEE when a PCM block follows
u2  version index   (MPEG: 3 = MPEG-1, 2 = MPEG-2, 0 = MPEG-2.5)
u2  sample-rate index, u2 channel mode, u2 mode extension
u1  granule index   (MPEG-1 pairs granules 0 and 1 into one frame)
u4  scfsi per channel (MPEG-1 granule 1 only)
per channel: u12 part2_3_length, then the rest of the granule side info (47 bits, MPEG-2 51)
main data (sum of part2_3_length bits), padded to a byte
if flag == 0xEE: u16 offset, u16 PCM samples, (v1b) u32 0, then big-endian PCM
```

All-zero version/rate/mode bytes mark padding at a block's end. Rebuilding standard MPEG
frames is lossless: write a frame header and the side info with each granule's fields, and
lay the main data out with the bit reservoir (`main_data_begin`) so each frame can use the
smallest standard bitrate. The PCM blocks only hold a few samples of silence at the start in
the files checked, so they are dropped.

Baked output: EALayer3 → `.mp3` (same size as the original), XAS → 16-bit `.wav`.
