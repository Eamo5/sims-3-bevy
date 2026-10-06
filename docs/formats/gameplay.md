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

### Opportunities

`Opportunities_BaseGame` (and `Opportunities_EP*` for the packs) has four sheets joined by
`GUID`: `OpportunitiesSetup` (icon, `Target` / `TargetData` — e.g. `RabbitHole` / `CityHall`,
`TargetInteractionName`, `…Length` in minutes, `…StartTime` / `…EndTime` like `9:00AM`,
`Timeout` `SimDays` with `TimeoutData` days, `CompletionEvent`, `EventListenerInfo1..3` for
feats to perform first, `Object` for things to deliver), `OpportunitiesRequirements`
(`Requirement1..` like `Skill,Athletic,5,10` or `WorldHasRabbitHoleType,Stadium`),
`OpportunitiesCompletion` (`CompletionWinReward1..` like `Money,650`, `CareerPerformance,25`,
`CareerRaise,10`, `SkillPercentage,…`; `CompletionModifier…`) and `Names` (string keys). An
empty `OpportunityType` means `Career` (the template row's default); the career is the GUID's
prefix (`BusinessCareer_…`). Texts are under `Gameplay/Excel/Opportunities/Names:<key>` and
use `{10.Money}`, `{9.Number}`, `{RabbitHoleName}`. The rabbit-hole type of a lot is the class
of its rabbit-hole object (`Sims3.Gameplay.Objects.RabbitHoles.Grocery`; one lot can hold
several, like Sunset Valley's downtown block with the bookstore, grocery and theatre).

### Lifetime wishes

Wishes are "dreams" in the tables. `DreamsAndPromisesNodes` lists every kind of wish as a
`Primitives` row: `Name` (a designer's note like `*$n simoleons in cash`), `Category`
(`Lifetime Dreams`, `Lifetime Dreams (EP1)`...), `Id`, `TriggerEvent`, `CheckFunction` (the
script that tests it, e.g. `NSimoleonsInCashMajorDreamCheckFunction`), `PrimaryIcon`
(`w_lifetime_simoleon_cash`), `RequiredProductVersions` (`BaseGame`, `EP2`...) and, for a
lifetime wish, `LifeEventIsLifetimeDream` `True` with its number in `LifeEventInputNumber`
(50000 simoleons, 20 friends, career level 10). `DreamNodeInstanceDefaults` has the
`DreamNodeInstance` rows that use them (`PrototypeId` = the primitive's `Id`), each with a
`FulfillmentScore`: a tenth of the lifetime happiness it's worth (2,000 to 3,500 for the
base game's lifetime wishes). The base game has 32; a career one names its branch in its
check (`Level10OfCriminalThiefBranch…`), and a few have no check (an event fulfils them).

Their in-game names aren't keyed by anything in these rows; the string tables do hold them
("Swimming in Cash", "Become a Master Thief", "The Tinkerer"), with their scrapbook lines.

### Writing

The Writing skill's tuning is an XML named after its class with a hash
(`Writing_0x49f73ba878b269dc`); its `<Current_Tuning>` holds `<kName value="…">` entries:
per genre `kLength<Genre>Min/Max` (pages), `kRoyalty<Genre>Min/Max` and `kValue<Genre>Min/Max`;
the writing speed (`kRateBasePPM` 0.12 pages a minute, `kRateBookWormBonusPPM`,
`kRateMaxWritingSkillPPM` at level 10); quality odds per level
(`kQualityLevel<N>ChanceFlop/Hit/BestSeller`, percent) shifted by
`kQualityPercentChangePerHiddenSkillPoint` per book written in the genre; royalty multipliers
(`kRoyaltyMaxWritingSkillMultiplier`, `kRoayltyMultiplierChangePerHiddenSkillPoint` — sic,
`kRoyaltyTraitMultiplier`, `kRoyaltyQualityMultiplierFlop/Hit/BestSeller`), `kRoyaltyLength`
(6 payments) at `kRoyaltyPayHour`; genre unlocks (`kMinLevelForMystery`,
`kMinLevelForRomanceHelplessRomantic`, `kNumSciFiWrittenForFantasy`,
`kNumBooksWrittenForMasterpiece`...); partial work every `kPartialWorkSubmitEveryXPercent` at
`kPartialWorkPageValue<level>` a page. Titles for written books are `Books`' `WrittenBookTitles`
rows (one column per genre), text under `Gameplay/Excel/Books/WrittenBookTitles:<key>`.

### Recipes and food

`RecipeMasterList` has a `Data` row per recipe: `Recipe_Key` (`Spaghetti`), `CodeVersion`
(empty for the base game), `Level` (Cooking), `Auto_Learn` (`x`: known at that level; the rest
come from recipe books), meal-time flags `Breakfast`/`Brunch`/`Lunch`/`Dinner`/`Dessert`/`Snack`,
`Is_Vegetarian`, `Ingredient_1..3`, `RegisterCost`, and the food's models as
`<model>#<geometry state>[:<material state>]` for the serving dish (`Group_Full/Half/Empty`) and
a plate (`Single_Full/Half/Empty`), e.g. `foodServeSpaghetti#full`. The name is found through
the name maps (`0x0166038C`: the model's MODL/VPXY instance; a few are hidden catalogue
objects such as `FoodEatHamburger`); the food sits on the game's `PlateServing` / `Plate`
objects. A geometry state is a run of a mesh's index buffer (MLOD mesh: after the bounds, the
skin controller, joint list and scale offset, `count` × {FNV-32 of the state name, start index,
min vertex, vertex count, primitive count}); the whole buffer is every state together. Names
are `Gameplay/Excel/RecipeMasterList/Data:<key>`. Recipe books are `Books`' `BookRecipe` rows
(`Recipe`, `Value` = price).

Produce is `Ingredients`' `Data` rows: `Ingredient_Key` (`Onion`), `Plant_Name` (the
`plants` row bearing it), `Price`, and `Model_Name` in the same `<model>#<state>` form
(`onionsWhole#onionsWhole`, `lime#limesWhole`, `apple#foodWhole`; `Thumbnail_Model_Name`, when
given, is the state the game pictures it in: `#plantSurface`, `#thumbnail`). A name map entry
for the model's name can belong to its rig (0x8EAF13DE) or footprint (0xD382BF57) rather than
the MODL, so only names whose instance has a MODL count; a few are catalogue objects instead
(`apple`). `tomatoesWhole` has a rig and footprint in the base game but no model. A state's
vertex range sits inside a buffer shared by all states, so its bounds are its own indexed
vertices'.

### Career branches

A `Careers` level row's `BranchName` is `Base` (or empty) until the career branches, then
the branch's name (`Thief` / `Evil` from Criminal level 7, `ElectricRock` / `Symphonic` from
Music 6, `SpecialAgent` / `ForensicAnalyst` from Law Enforcement 6); the branched levels
repeat the level numbers once per branch.

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
