# Sims 3 in Bevy

A standalone reimplementation of The Sims 3 in Rust and [Bevy](https://bevyengine.org), running
on the data of an installed copy of the game (base game plus any expansions and stuff packs).

Everything is converted **once, up front** into a local cache of GPU-ready assets
(block-compressed DDS textures, pre-decoded meshes, packed world data), so the game itself
loads a town in well under a second and never touches the original packages while playing.

## Running

```sh
# 1. Convert the installation (≈1 minute the first time; later runs only redo what changed)
cargo run --release -p s3bake -- --all-worlds
#    the install defaults to S:\Games\Sims 3\The Sims 3;
#    use --data <path> or SIMS3_DATA=<path> for another location

# 2. Play
cargo run --release -p sims3
```

The cache lives in `baked/` (or `SIMS3_CACHE`). If the cache is missing, the game converts on
its first run instead. Once converted, every cached world stays playable even when the
installation is unreachable.

## What's in it

- **Towns**: all installed worlds (Sunset Valley, Twinbrook, Bridgeport, Appaloosa Plains,
  Moonlight Falls, Starlight Shores, Sims University, Isla Paradiso, Oasis Landing and the
  World Adventures destinations Shang Simla, Al Simhara and Champs Les Sims) with their
  terrain paint, the game's own terrain colour and shadow maps,
  roads with markings, water, placed objects and SpeedTree trees drawn from their own
  billboard pictures.
- **Houses**: every lot's real walls (with door and window openings) and floors in the very
  wallpapers, sidings, tiles, carpets and paving the builders chose — the game's texture
  recipes (complates) re-rendered from each lot's designs — plus foundations, stairs and the
  house's own furniture; nearby lots are shown in full detail with
  the game's roofs, distant ones as the game's pre-rendered imposters.
- **Sims**: Create-a-Sim from real CAS parts, skin tones and animation clips, with the game's
  body-shape morphs (weight and fitness sliders; town Sims keep their builds, working out
  firms Sims up); needs, moods,
  skills, careers, relationships, autonomy, social interactions; children, teens, adults and
  elders who grow up and grow old (birthdays, new traits, passing away).
- **Thought and speech balloons**: the game's own balloon art and balloon table. Sims think about
  needs running low, new moodlets and their traits, dream while asleep, and take turns showing
  what they're talking about (small talk about the weather, their work or interests; insults,
  compliments and flirts with the game's like and dislike marks).
- **Families grow**: couples can try for a baby; pregnancy brings morning sickness, then a
  baby in a crib (the game's swaddled baby model). Grown-ups feed, change, cuddle and settle
  babies (cradled in their arms) and pick up, read to and put toddlers to bed; toddlers toddle
  about, play with toy boxes, xylophones and peg boxes, nap in cribs and cry when neglected.
- **Town families**: every world's premade households (the Goths, Landgraabs, Altos, …) with
  their names, bios, family portraits, traits, careers, skills, marriages and homes — play one
  of them, or meet them around town.
- **Play**: choose a furnished house (with the game's lot pictures) or an empty lot, buy
  mode with the game's own catalogue pictures (base game and every installed pack's),
  multi-storey navigation by stairs, day/night with lamps and street lights. Sims deciding on
  something by themselves think of it in a balloon with its picture.
- **Build mode painting**: the catalogue's wallpapers and floor coverings (every base-game
  pattern with its name and price, swatched from its texture recipe) on Wallpaper and Floors
  tabs; click a wall to paper that side, or a floor to cover the whole room. Repainting is
  charged, sounds like the game's build tools, and is kept in saves.
- **Sound**: the game's own audio, converted once (EALayer3 rewrapped as MP3, EA-XAS decoded to
  WAV): object sounds and Simlish voices cued by the animations, footsteps by floor and
  shoe, interface clicks, stings for promotions, weddings and other moments, birds by day
  and insects at night, music for the menus, Create-a-Sim and buy mode, and the radio
  stations on stereos.
- **Careers** from the game's own career tables: all eleven base-game careers (Professional
  Sports included) with every level's title, pay, hours and workdays, and the part-time jobs
  (bookstore, grocery, spa, mausoleum) for teens.
- **Interface**: the game's own icons and words — moodlets (name, description, time left),
  traits (Create-a-Sim and the Sim's panel) and wishes — read from the game's tuning tables
  (`GameplayData.package`) and string tables, with tooltips on hover. Sims' portraits are
  photographed live (a small studio camera with its own light and backdrop, retaken after new
  clothes or a birthday) for the household buttons, ringed in each Sim's mood colour, for
  balloons about someone, and for the Relationships panel (R): everyone the Sim knows with
  what they are to each other and friendship and romance bars.
- **Options**: Escape pauses with a game menu (resume, options, save, main menu, quit); the
  options set master, music, effects, voice and ambient levels, aging and life span (short
  to epic), free will, shadows and the frame-rate counter, kept in `settings.json`.

## Controls

| Input | Action |
|---|---|
| Left-click Sim / object | Pie menu of interactions |
| Left-click ground | Go here |
| WASD / arrow keys (Shift = faster) | Pan |
| Q / E | Rotate |
| Mouse wheel, Z / X, = / − | Zoom |
| Page Up / Page Down | View the floor above / below |
| Tab | Next Sim |
| C | Centre the camera on the selected Sim |
| Space | Pause |
| 1 / 2 / 3 | Game speed |
| B or F2 | Buy mode (, and . rotate, Delete sells, Esc leaves) |
| R | Relationships panel |
| Esc | Game menu: pause, options, save, quit |

## Layout

| Crate | Purpose |
|---|---|
| `s3pkg` | DBPF packages, RefPack, Resource.cfg install discovery |
| `s3formats` | Decoders: models, textures and the TXTC compositor, CAS, rigs and clips, worlds, lots |
| `s3bake` | The converter and the cache format (`s3bake --info <world>` describes a baked world, `--dump-sound <name> <dir>` extracts a baked sound) |
| `s3tool` | Inspection tools used while reverse-engineering formats |
| `game` | The game (`sims3`) |

Format notes are in `docs/formats/`.

## Testing switches

`sims3 --world <name> --lot <id | house> --cam x,z,dist,yaw,pitch --hour <h> --speed <0-3>
--view-level <n> --do <interaction> --screenshot <png> --shot-delay <s> --exit-after-shot`,
and `--ui-flow <dir>` clicks through the menus saving a screenshot of each screen.
