# The game's interface: layouts, pictures, fonts

The Sims 3 draws its interface with EA's UTFWin framework (shared with Spore). Everything it
needs is in `Game/Bin/UI/UI.package`, which is **not** listed in any `Resource.cfg`: the
executable opens it itself. `s3bake::ui` bakes it (`ui.bin` + `ui.pack`); `game::layout` draws
it with Bevy UI; `game::livehud` drives the live-mode HUD with it.

| Type | What |
|---|---|
| `0x025C95B6` | Layout (LAYO): UTF-8 XML window tree. 654 in the Steam build. Instance = fnv64 of the lowercase layout name (`HUDSimDisplay`, `HUDPuck`, `HUDSkewer`, `HUDMotives`, `HUDNavigation`, `HUDMoodletGridCell`...). The names are in `UI.dll`'s strings: 452 of 654 match. |
| `0x025C90A6` | Style sheets, CSS-like, one per language (the English one uses Helvetica Rounded). |
| `0x062E9EE0` | Fonts (OpenType/CFF). Helvetica Rounded LT Std Bold is the HUD's; Helvetica Neue LT Std 65 Medium the body text's. |
| `0x2F7D0004` | The pictures the layouts use are **not** in UI.package: they're the PNG UI images in `FullBuild`/`DeltaBuild` packages of the base game and packs. |
| `0x08C88F06`, `0xF0FF5598` | Certificates; trigger (key binding) configs. |

## Layout XML

```xml
<graph class="Layout" type="Layout">
  <object cls="Window" clsid="0x4ec1b8d8" id="0x00000001">   <!-- an export -->
    <prop name="WindowFlags" type="uint32" value="0x00002013" />
    <prop name="ControlID" type="uint32" value="0x06f5b840" />
    <prop name="Area" type="rectf" value="0,3,29,133" />       <!-- left, top, right, bottom -->
    <prop name="FillDrawable" type="object"> <object cls="StdDrawable">...</object> </prop>
    <prop name="WinProcs" type="object"> <object cls="SimpleLayout"><prop name="Anchor" value="6"/></object> </prop>
    <prop name="Children" type="object" count="2"> <object .../> <object .../> </prop>
  </object>
  <object ... id="0x00000002"> ... </object>                   <!-- more exports -->
</graph>
```

* A layout may export several top-level windows (`id`); code picks one with
  `GetWindowByExportID` (a moodlet cell: 1 positive, 2 negative, 3 neutral). 166 layouts do.
* **Children are listed front to back: the first child is drawn on top.** (The Sim display's
  moodlet `ItemGrid` comes before its background; a skewer slot's mood-colour backing is its
  last child, behind the portrait.)
* `WindowFlags`: 0x1 visible, 0x2 enabled, 0x10 ignores the mouse, 0x400 clips its children.
* Classes: `Window`, `Button`, `Text`, `Sims3CustomWindow`, `Sims3IconButton`, `ItemGrid`...
  Window procs also include effects (`Fade`, `Glide`, `Inflate`, `GlowModulate`, `Rotate`).
* `ControlID`s are what the code looks windows up by (`GetChildByID`). Item templates reuse
  small ids (a skewer slot: 1 thumbnail, 2 mood window, 3 select button), so look them up
  within their slot.
* `Caption` / `TooltipText` carry the designer's English text and a `key` (type 0x12) that is
  fnv64 of the string name (`Ui/Tooltip/HUD/SimDisplay:Mood`); the baker swaps in the string
  table's text where there is one.

### Placement (window procs)

Anchor bits: **1 top, 2 bottom, 4 left, 8 right**.

* No proc: the area is from the parent's top-left.
* `SimpleLayout(anchor)`: each coordinate is measured from the edge it's anchored to; a right
  or bottom coordinate on an anchored right/bottom edge is negative inwards. Left+right (or
  top+bottom) stretches. (`[20,110,-29,-70]` anchored 15 = 20 in from the left, 110 down,
  29 in from the right, 70 up from the bottom.) Unanchored on an axis: from the top-left.
* `HudLayout(anchor, dimensions 1024×768)`: top-level panels; the area is in a 1024×768
  design screen and keeps its distance from the anchored edges of the real one. Anchor 6
  (bottom+left) for the whole live HUD; 15 stretches to the screen.
* `CenterInParentLayout(vertical spacing)`: centred across, that fraction of the free space down.

When code changes a window's area (a meter's size-control window), anchored children follow:
the mood meter's bar is anchored bottom+left inside a clip window, so it stays put while the
size-control window grows up from the meter's foot.

### Drawables

* `StdDrawable`: `Image` × 8 per state, **0 normal, 1 disabled, 2 highlighted (hover),
  3 pressed (active), 4–7 the same when selected**; `ScaleType` 0 stretch, 1 own size centred
  (state images may differ in size), 2 nine-slice by `ScaleArea` borders (fractions of the
  image); `GlowMask` for hover glow.
* `ImageDrawable`: one `Image`; `ImageDrawableFlags` bit 1 scale to fit, bit 2 keep the
  shape; `AlignmentHorizontal/Vertical` 1 left/top, 2 right/bottom, 3 centre; `Scale`.
* `IconDrawable`: as `ImageDrawable`, plus `StateColors` × 8 (the icon's tint per state).
* `IconButtonMultiDrawable`: `Drawables` list (a `StdDrawable` background, then the icon).

`ShadeColor` (ARGB) tints a window's drawable; code sets it for meters and mood colours.

### Text

`Text`: `Caption`, `TextFont` (a style id), `TextColor`, `HorizontalAlign` (0 left, 1 centre,
2 right, 4 justified: paragraphs) and `VerticalAlign` (0 top, 1 middle, 2 bottom, 3 middle),
unlike the images' 1/2/3 (judged from the layouts: the motives' names, wider than their bars,
are 1; the managed code doesn't name them), `WordWrap` (0 none, 4 on, 5 inherit).
Buttons: `CaptionColors` × 8, `CaptionHAlign`, `CaptionVAlign`.

Styles (`TextFont`) are the style sheet's: `Name(0xid) : Parent { font-family; font-size: 8pt;
line-spacing: 14; font-style: italic }`, inheriting. Point sizes at 96 dpi (8pt = 10.7 px).
Families are matched to fonts by PostScript name (`HelveticaRounded LT Std Bd` =
`HelveticaRoundedLTStd-Bd`), else by family prefix.

### ItemGrid

`VisibleCols`, `VisibleRows`, `CellArea` (cell size), `GridPadding` (l, t, r, b),
`HorizontalScrolling`, `UseArrowsForScrolling`; its arrow buttons are its children
(0x06000000 up, 0x06000001 down). Cells are windows of another layout (code fills them).

## The live HUD

The HUD's behaviour is in `UI.dll` (decompiled with ILSpy from the S3SA in
`Game/Bin/*.package`, see gameplay.md) and its numbers in `Sims3GameplaySystems.dll` and the
GameplayData tuning:

* `SimDisplay` (`HUDSimDisplay`): bust 0x06F5B801; time control base 0x47E97A00 (+1 pause,
  +2 normal, +3 double, +4 triple, +5 skip, +6 time text); mood meter 0x06F5B840 size
  reference, 0x41 back fill, 0x42 fore fill, 0x43 size control, 0x44 glow, 0x45/0x46 halfway
  and bonus markers; moodlet grid 0x06F5B821 (cells `HUDMoodletGridCell`: icon 0x06F5B830,
  time 0x31, no-timeout 0x32); wishes: staging 0x06F5B804 (page 0x05/0x06), slots 0x10–0x13
  (NW, NE, SW, SE; icon = child 1), lifetime 0x06F5B807; expand 0x06F5B800.
* Mood meter fill (`UpdateMoodBar`): mood v in [min −100, 0, lifetime 50, max 150] maps
  piecewise to [0, halfway, bonus, 1], halfway/bonus = 1 − marker.top / reference height;
  the size-control window's top = bottom − f × height.
* `PuckController` (`HUDPuck`): base 0x8FEFFA00: +2 build, +3 buy, +4 live, +5 funds,
  +7 walls trigger, +0x12..0x14 walls up/cutaway/down pictures, +0x0F/+0x10 level up/down,
  +0x16 options, +0x17 record, +0x18 snapshot; camera +0x100.. rotate left/right, zoom
  out/in, pitch down/up, Sim/house/map view.
* `Skewer` (`HUDSkewer`): slots 0xF6FDA501–508 (bottom up), each 1 thumbnail, 2 mood window
  (tinted `MoodManager.GetMoodSingleColor`), 3 select button (selected = current Sim).
* `Navigation` (`HUDNavigation`): background 0x1BA48C20; tabs 0x1BA48C01.. Simology, Career,
  Skills, Lifetime rewards, Relationships, Inventory, Opportunities, Motives.
* `MotivesPanel` (`HUDMotives`): motive i at 0x06FDDE00+i (hunger, bladder, energy, social,
  hygiene, fun); within it 0x06FDDF02 size reference, 0x03 size control (width = fraction ×
  reference width), 0x04 fore fill, 0x05 back fill.
* Colours: `Motive.ComputeMotiveColor` / `MoodManager.ComputeMoodColor` interpolate HSV
  triples over value ranges, with an alpha over its own ranges: fill colours green
  (`kLHS*`), back colours red → amber → dark (`kRHS*`, the back fill drawn over the bar fading
  as the need fills). Values in GameplayData's `Sims3.Gameplay.Autonomy.Motive` and
  `MoodManager` tuning (copied into `livehud.rs`).
