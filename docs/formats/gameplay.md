# Gameplay data

How The Sims 3 stores the designers' tuning, the interface pictures that go with it and the game's
script code. All of it is read by `s3bake::gamedata` (baked to `global/gamedata.bin` and
`global/icons.pack`).

## 1. Tuning tables

`Game/Bin/Gameplay/GameplayData.package` (not listed in `Resource.cfg`) holds XML resources
(`0x0333406C`) exported from the designers' spreadsheets, one per workbook, named in the
package's name map (`0x0166038C`): `Buffs`, `Traits`, `Skills`, `Careers` plus one table per
career's levels, `Balloons`, and many more.

Each workbook is a flat list of row elements named after the sheet, whose child elements are the
columns:

```xml
<Buffs>
<BuffList>
  <Hex>Hungry</Hex>
  <BuffName>Hungry</BuffName>
  <ThumbFilename>moodlet_hungry</ThumbFilename>
  ...
 </BuffList>
```

The first row of a sheet is often a template listing every column with default values (empty
key). Rows marked with a `SKU` / `CodeVersion` / `ProductVersion` other than `BaseGame` belong to
expansion packs.

### Text

Names and descriptions in the tables are string-table keys. The English text is in the STBL
resources under key `fnv64(lower("Gameplay/Excel/<Workbook>/<Sheet>:<Key>"))`, for example
`Gameplay/Excel/Buffs/BuffList:Hungry`.

### Interface pictures

Icons named in the tables (`moodlet_hungry`, `trait_genius_s`, `w_tv`…) are PNG resources of type
`0x2F7D0004` with instance `fnv64(lower(name))` in the main packages.

## 2. Balloons

The `Balloons` workbook says what Sims think and say. It has four sheets, each a list of rows with
`BalloonName` (an icon), `ReferencedKey` (another list to draw from), `BalloonAxis` (`kNeutral`,
`kLike`, `kDislike`) and `Weight`. A row with an empty key column continues the list of the row
above it.

| Sheet    | Key column  | Keys                                                                   |
|----------|-------------|------------------------------------------------------------------------|
| `Idle`   | `IdleKey`   | `Motive<Need>`, `Buff<buff hex>`, `Trait<trait hex>`                   |
| `Social` | `ActionKey` | social interaction names (`Chat`, `Insult`, `Compliment`…)             |
| `Topic`  | `Key`       | conversation topics (`Weather`, `Books`, `Career Business`…)           |
| `Random` | `Key`       | random sets (`Random`, `RandomFlirty`, `Dream<X>GoodBalloons`…)        |

Some `BalloonName`s are not icons but names of the script's pickers, resolved at run time:
`GetSpeechBalloonImageForChat` (small talk), `Thumbnail Target` / `Thumbnail Actor` (the Sim's
picture), `Actor Career Topic` / `Target Career Topic`, and a few more.

The balloon icons are textures, not interface PNGs: 64 × 64 DDS (`0x00B2D882`, instance
`fnv64(lower(name))`) in `FullBuild2.package`. The balloons themselves are textures there too:

| Texture               | Use                                                  |
|-----------------------|------------------------------------------------------|
| `thought_balloon`     | thought cloud (128 px)                               |
| `thought_balloon2`    | a second cloud shape (dreams here)                   |
| `thought_balloonLead` | the little bubble leading up to a cloud (32 px)      |
| `speech_balloon`      | speech balloon with its tail (128 px)                |
| `speech_balloon2`     | round balloon without a tail                         |
| `sb_like`             | smiley badge for `kLike`                             |
| `sb_dislike`          | red "no" sign drawn over the icon for `kDislike`     |
| `t_balloon_routefail` | the Sim can't get somewhere                          |

## 3. Catalogue thumbnails

Buy-mode pictures are not in the main packages but in `Thumbnails/AllThumbnails.package` (and
`EP*/Thumbnails`, `SP*/Thumbnails` for the packs), outside `Resource.cfg`:

| Type         | Contents                                             |
|--------------|------------------------------------------------------|
| `0x0580A2B4` | objects, small PNG                                   |
| `0x0580A2B5` | objects, 54 px PNG                                   |
| `0x0580A2B6` | objects, 128 px PNG on transparency                  |
| `0x0589DC44`–`46` | wallpapers and floors (three sizes)             |
| `0x626F60CC`–`CE` | CAS parts, in `CasThumbnails.package`           |

For objects the instance is the object's OBJD instance and the group its colour variant (0 =
the default). `s3bake::gamedata` keeps the first variant of each at 128 px.

Map tags are drawn from `hud_icon_maptagbase_r2` (a white orb the game tints by kind) and a
white glyph from the atlas `ATLAS_MapTagColors_00` (512 × 1024). The atlas has no layout table
in the packages; `s3bake::gamedata::MAP_TAG_GLYPHS` lists the glyphs' rectangles, found by
connected components on its alpha.

## 4. Script assemblies

The game logic is .NET (Mono) code in `Game/Bin/gameplay.package`, `scripts.package` and
`simcore.package`: resources of type `0x073FAA07` (S3SA), one per assembly
(`Sims3GameplaySystems`, `Sims3GameplayObjects`, `UI`, `ScriptCore`, `SimIFace`, …).

An S3SA resource starts with a version byte (2), a 32-bit character count and that many UTF-16
characters of game version (`0.0.0.32`), then a 32-bit value, a block count and one 32-bit entry
per 512-byte block, followed by the obfuscated assembly. s3pi's `ScriptResource` wrapper undoes
the obfuscation (its `Assembly` property is a reader over the plain DLL); `cache/s3sa_extract.ps1`
calls it through PowerShell reflection to write the DLLs out. The assemblies' string heaps are
the quickest way to find resource names the code uses, such as the balloon icon names above.
