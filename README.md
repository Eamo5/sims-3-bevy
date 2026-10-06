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
  billboard pictures, under a sky made from the game's own sky textures: drifting clouds from
  its cloud noise, the sun's halo, the moon and the night's stars, coloured through the day.
- **Houses**: every lot's real walls (with door and window openings) and floors in the very
  wallpapers, sidings, tiles, carpets and paving the builders chose — the game's texture
  recipes (complates) re-rendered from each lot's designs — plus foundations, stairs and the
  house's own furniture; nearby lots are shown in full detail with
  the game's roofs, distant ones as the game's pre-rendered imposters.
- **Sims**: Create-a-Sim from real CAS parts (browsed as the game's own style pictures), skin
  tones, hair colours and animation clips, with the game's
  body-shape morphs (weight and fitness sliders; town Sims keep their builds, working out
  firms Sims up); needs, moods,
  skills, careers, relationships, autonomy, social interactions; children, teens, adults and
  elders who grow up and grow old (birthdays, new traits, passing away). A birthday cake
  baked at the fridge (§20) lets someone grow up there and then: they blow out the candles
  with the game's animation, the household cheers, and the cake is cut for everyone.
- **Gardening** from the game's plant and produce tables: households start with a few seeds
  and buy more at the grocery store (rarer ones as their skill grows); seeds are planted
  anywhere outdoors on the home lot in the game's garden soil and grow (the game's bush, vine
  or tree) while kept watered and weeded, then bear tomatoes, apples, onions, garlic... to
  harvest into the gardener's inventory, to sell or eat. Planting, watering (with the watering can), weeding and harvesting use
  the game's gardening animations and teach Gardening; green-thumbed Sims tend their plants
  on their own. Each plant has the game's quality, Horrifying to Perfect: it improves while
  watered and weeded (faster for a skilled gardener) and suffers when neglected, and its
  produce sells at the game's multiplier for its quality (up to four times for Perfect).
  Perfect produce counts towards The Perfect Garden lifetime wish. The garden is kept in
  saves.
- **Breakdowns**: showers, baths, sinks, TVs, computers and stereos break now and then with
  use (toilets clog), less often the better they are; Sims repair them with the game's own
  repair animations, learning Handiness (handy and neat Sims do it on their own), or the
  repairman can be phoned: he pulls up in the game's handyman pickup, in his overalls, fixes
  each broken thing in turn with the same animations (the household leaves the repairs to
  him), and is paid §75 and §25 a fix. Handy grown-ups can upgrade appliances too ("Upgrade
  ›", with the same animations, building Handiness): at level 3 a water heater for the shower
  and the bath, improved channels for the TV, better speakers for the stereo and a faster
  processor for the computer (half as much again of what they give); at level 5 Unbreakable.
  Electronics can shock the unskilled; upgrades are kept in saves. Fishing trips teach Fishing
  and bring back a catch to sell.
- **Writing novels** by the game's Writing tuning: at a computer, "Write Novel" offers the
  genres the Sim has opened up (Fiction and Non-Fiction to start; Sci-Fi, Trashy, Drama,
  Humor, Mystery and Romance with skill — sooner for the good-humoured and hopeless romantics;
  Children's with painting too; Satire, Fantasy, Vaudeville and the Masterpiece after enough
  books of others), or carries on with the book under way. Pages go by at the Sim's writing
  speed (bookworms and skilled writers faster) with a little pay for partial work; a finished
  book gets one of the game's titles, turns out a flop, a success, a hit or a best seller, and
  pays royalties at noon each week, six times. The Skills tab shows the book under way, books
  written and the week's royalties. Kept in saves.
- **Ranked chess** by the game's Logic tuning: "Play a Ranked Match" at a chess table pits the
  Sim against an opponent of their rank (stronger each rank), won by Logic and a little luck;
  wins lift them from Unranked through Apprentice, Tenderfoot, Journeyman and Instructor to
  Grand Master. The record shows under Logic in the Skills tab and is saved.
- **Lifetime wishes**, the base game's own (CEO of a Mega-Corporation, Leader of the Free
  World, Become a Master Thief, Rock Star, Swimming in Cash, Super Popular, Heartbreaker,
  Master of the Arts, Renaissance Sim, Professional Author, The Culinary Librarian, The
  Perfect Garden, The Perfect Aquarium, Gold Digger, Chess Legend, Surrounded by Family, Jack
  of All Trades and more; all 32 of them), with their icons, targets and rewards
  from the game's dream tables. Create-a-Sim offers five that suit the Sim's traits; a child
  growing into a teen picks one in the game's dialog; Sims moving in without one take the
  best fit (one not already half done). The wish sits beside the others with its progress
  (§12,000 of §50,000, Painting 7/10 · Guitar 4/10, level 6 of 10), and fulfilling it is worth
  20,000 to 35,000 lifetime happiness. Kept in saves. A townie marrying in brings their share
  of their family's funds (marry a Landgraab, an Alto or a Goth to start on Gold Digger).
- **Lifetime rewards**: the game's reward traits, bought with lifetime happiness at their
  costs in its dialog (icon, name and description): Steel Bladder, Dirt Defiant, Hardly
  Hungry, Fast Learner, Fast Metabolism, Professional Slacker, Opportunistic (bigger
  opportunity rewards), Attractive, Extra Creative (paintings and books earn more), Super
  Green Thumb, Discount Diner, Complimentary Entertainment, Bookshop Bargainer, Haggler, and a
  new lifetime wish.
- **Opportunities** from the game's own tables: by day, Sims are offered ones that suit their
  career or skills in the game's dialog (its icon, name, description and reward); taken-on
  opportunities are listed in the Opportunities panel (O) with their deadlines and are done
  at the right venue during its hours (Business Sims sign deals at the office, cooks compete
  at the restaurant...), paying money, performance at work or skill, with the game's
  completion and failure texts. Kept in saves.
- **Map View**: zoomed out over town, every lot gets the game's map tag — the household's
  home in green with a star, other homes and empty lots, and each venue's own glyph (cut from
  the game's map-tag atlas); hover for the name, click a venue for its activities or a home to
  fly there.
- **Parties**: phone round to throw one; friends (then townsfolk) arrive two hours later and
  stay till midnight, everyone mingling, and the household gets the game's "Threw a Great
  Party" or "Threw a Lame Party" moodlet depending on how much talking went on; party animals
  have an "Awesome Party".
- **Mail**: on Mondays and Thursdays the mail carrier walks up in uniform, puts the bills in
  the mailbox and the flag up (the game's animation); they're paid there (Sims see
  to them on their own too); bills left three days bring the repo man, who takes things worth
  what's owed. The paper is delivered every morning, to read or look for a job in.
- **Death**: an elder whose time has come collapses and the Grim Reaper appears in his robe
  with his scythe (the game's own Reaper animations), raises a tombstone where they fell and
  vanishes; the family is heartbroken and can mourn at the tombstone. Sims can also die of
  hunger (a day and a half starving, with a warning halfway) and of electrocution: fixing a
  TV, computer or stereo with little Handiness can shock them (the game's Singed moodlet and
  shock animation, the repair failed), and a second shock while still singed stops their
  heart. Each death has the game's own last moments, and its ghost its own colour.
- **Meals**: cooking serves a group meal on the game's serving platter (on the nearest counter),
  with a serving for everyone at home; Sims grab a plate and sit down at a dining table or
  counter stool to eat (the game's sit-down and bar-stool eating animations, plate on the
  table, fork in hand), or eat standing when every seat is taken, and leave dirty dishes that
  neat Sims clear away. Guests eat but don't cook, children don't use the stove. Or phone for
  a pizza: within the hour the delivery, in the game's pizza uniform, carries the pizza box in
  to the kitchen counter.
- **Recipes** from the game's recipe list: the stove offers "Cook Breakfast/Lunch/Dinner ›"
  and "Cook Dessert ›" with the recipes the Sim knows for that time of day (Mac and Cheese and
  Waffles to start; Goopy Carbonara, Spaghetti, Key Lime Pie, Lobster Thermidor... as Cooking
  grows; meatless ones for vegetarians), the rest from recipe books bought at the bookstore
  at the game's prices. The food is the game's own models in their geometry states — the dish
  on the serving platter, a plateful in front of each Sim — scraped clean once eaten. The
  Culinary Librarian lifetime wish asks for every one. Learned recipes are kept in saves.
- **Props in hand**: the game's own plates, forks, books and guitars, held as its animation
  clips place them (each Sim clip's companion prop clips and their parent events).
- **Thought and speech balloons**: the game's own balloon art and balloon table. Sims think about
  needs running low, new moodlets and their traits, dream while asleep, and take turns showing
  what they're talking about (small talk about the weather, their work or interests; insults,
  compliments and flirts with the game's like and dislike marks).
- **Socials** by the game's categories (Friendly, Funny, Romantic, Mean, Special) with its own
  animations: chatting, jokes, funny and dramatic stories, bragging, goofing around, silly
  faces, crying on a shoulder, cheering up and apologising; flirting, embracing, gazing into
  eyes, back rubs, leaping into arms and kisses up to the dip kiss as romance grows; insults,
  yelling, irritating, slaps, declaring a nemesis and fights (the grapple and headlock, won
  by the athletic and the brave, the loser embarrassed) for those who don't get on.
- **Families grow**: couples can try for a baby; pregnancy brings morning sickness, then a
  baby in a crib (the game's swaddled baby model). Grown-ups feed, change, cuddle and settle
  babies (cradled in their arms) and pick up, read to and put toddlers to bed; toddlers
  crawl about until a grown-up has taught them to walk (three lessons, with the game's
  kneel-and-hold-hands animation; talking is taught the same way, and both show on the
  toddler's Skills tab), play with toy boxes, xylophones and peg boxes, nap in cribs and cry
  when neglected.
- **Town families**: every world's premade households (the Goths, Landgraabs, Altos, …) with
  their names, bios, family portraits, traits, careers, skills, marriages and homes — play one
  of them, or meet them around town.
- **Play**: choose a furnished house (with the game's lot pictures) or an empty lot, buy
  mode with the game's own catalogue pictures (base game and every installed pack's) and
  every catalogue object on its tab by the game's own grouping (smoke and burglar alarms,
  gnomes, sprinklers, picnic tables and trees under Electronics, Outdoors and Misc),
  multi-storey navigation by stairs, day/night with lamps and street lights. Sims deciding on
  something by themselves think of it in a balloon with its picture.
- **Build mode painting**: the catalogue's wallpapers and floor coverings (every base-game
  pattern with its name and price, swatched from its texture recipe) on Wallpaper and Floors
  tabs; click a wall to paper that side, or a floor to cover the whole room. Repainting is
  charged, sounds like the game's build tools, and is kept in saves.
- **Out on the town**: Sims drive to the parks, the library, the gym, the beach and the
  other community lots (from the lot's pie menu or its map tag) and use what's there: chess
  tables, computers, bookshelves, swings, grills, restrooms. Each lot gets its own walk grid
  and floors, its walls come down as the camera moves in, Sims on it keep to it in their own
  choices, townies turn up during the day to use the place too (and to meet), and "Go Home"
  drives them back while the household carries on at home.
- **Sim panel tabs**, as the game's: Needs; Skills (each skill learned with its icon, level
  and progress to the next); Career (the job and level, hours, days and pay, the performance
  meter and the next promotion); Simology (the lifetime wish and how far along it is, traits
  with what they mean, lifetime happiness and the rewards bought). F5 to F8 switch tabs.
- **Fences and railings**: every lot's fences (picket, rail, iron, garden edging) and its porch
  and balcony railings, from the lot's fence posts and the catalogue's fence pieces (straight
  and diagonal runs, posts); Sims can't walk through them.
- **Pools**: the lots' pools (Le Petit Shark Pool Center, the apartments', the gym's, estates')
  are let into the ground with their tiled floors and sides, a stone coping and clear turquoise
  water over the mosaics; the terrain is left open over them. Sims swim from the ladders: they
  change into swimwear (the game's own: trunks and a bare chest, swimsuits and two-pieces),
  climb down, swim about the pool with the game's swim cycle (building Athletic skill) and
  climb out back into their clothes. At bedtime they change into their sleepwear (pyjamas,
  nightgowns, boxers), and into athletic wear to work out, and dress again after.
- **Water effects**: fountains gush and fill their basins, showers spray from the head and taps
  run while someone's at them, each from the object's own effect slot (the game's RSLT slots).
  TVs show a programme on the screen (at the TV's screen slot) while anyone's watching, its
  light flickering on the room.
- **Lots' own ground**: lots keep their sculpted ground; its dips are carved back into the
  flattened world (Central Park's sunken plaza with the fountain basin set into it, hollows on
  the beaches), and ground-level paving follows the ground.
- **Fireplaces**: Sims light a fire with the game's animations; it burns a few hours, flames
  dancing in the hearth and a flickering warm light on the room, and they warm their hands by
  it or put it out. Now and then a spark catches the floor in front.
- **Homework**: children and teens come home from school (Monday to Thursday) with homework,
  and sit down at a table to do it (the game's homework animations; sooner for bookworms and
  geniuses); done, it lifts their grades; left undone, it drags them down.
- **Dates**: Sims ask each other on dates; for a few hours the date keeps them company
  (flirting, chatting, complimenting), and it ends a Great Date or a Bad Date (the game's
  moodlets) by how much closer they've grown and how they feel.
- **Babysitter**: when the grown-ups and teens are all out and a baby or toddler would be home
  alone, a babysitter arrives by car, feeds them, changes them and plays with them, and goes
  home (paid §10 an hour) once someone's back. Visitors don't sleep in the family's beds or
  use its shower.
- **Families**: who is whose parent, from the town's premade families and the household's
  births and adoptions (kept in saves). The Relationships panel names family ("Son · Good
  Friend", "Grandmother", "Cousin") and partners as the game does (Wife, Boyfriend, Fiancée),
  and relatives, out to cousins, can't romance one another. Its Family Tree button shows the
  selected Sim's grandparents, parents, brothers and sisters and spouse, children and
  grandchildren, with their portraits (or initials, for those not about).
- **Maid**: hired by phone (§15 an hour, kept in saves), she comes every morning at nine in the
  game's maid service car and French maid's uniform, clears away the dirty dishes and spoiled
  food, empties the trash cans and goes. The service Sims wear the game's own uniforms (never
  worn by anyone else) and aren't kept in saves.
- **Moving house**: the phone's "Move to a New Home" saves the game and opens the lot chooser;
  on the chosen lot the household starts afresh with everything else they had (Sims, skills,
  jobs, relationships, money, wishes, garden seeds, collection, the town's story), the old
  home's bought furniture sold back for four-fifths of its price.
- **Story progression**: the rest of the town lives on. Each morning the world's other Sims
  grow older (elders passing away in time), single grown-ups pair off and couples marry,
  married couples have babies, and grown-ups find jobs and are promoted. The household hears
  about Sims it knows, the morning paper carries the town's news, and it's all kept in saves:
  the town's Sims come back older (or not at all) when the game is loaded.
- **Social worker**: a baby or toddler left starving for six hours, or a child for twelve
  (warned halfway), brings the social worker by car; she takes the child away and the family
  is heartbroken. She also brings adopted children: the phone's "Adopt a Child ›" (a baby —
  given a crib — a toddler or a child, a girl or a boy), who join the family a couple of hours
  later.
- **Fire**: a poor (or clumsy) cook can set the stove alight; the flames flicker and light
  the room, spread to what's beside them and ruin it, and Sims nearby panic with the game's
  fire animations. Anyone caught in the flames burns: they may stop, drop and roll, or die
  (the Grim Reaper comes). A smoke alarm calls the fire department at once (otherwise someone
  phones a little later), and a firefighter in uniform arrives by fire truck to put out each
  fire with the extinguisher.
- **Ghosts**: the household's dead rise from their tombstones in the small hours, see-through
  and tinted by how they died (pale blue for old age, ember-orange for fire), float about the
  lot with the game's ghost animations and give the living a fright (Scared). Graves and who
  lies in them are kept in saves.
- **Burglars**: some nights a burglar in black slips onto the lot and bags the priciest thing
  in the house. A burglar alarm (or an awake Sim who spots them) calls the police; an officer
  in uniform arrives in the cruiser, and if they catch the burglar with the game's cuffing
  animation, the loot goes back where it was.
- **Traffic**: the game's cars (sedans, hatchbacks, pickups, vans, sports cars) drive the
  town's roads around the camera, keeping right along the world's own road graph and turning
  at its intersections; the school bus, the carpool and taxis pull up at the curb to take Sims
  away and bring them back.
- **Collecting and fishing**: the world builders' spawners leave gems, metals and space rocks
  about the community lots (from the game's own tables, with their odds and prices); Sims
  crouch to collect them, catch the butterflies flitting about by day and the beetles in the
  grass (each kind in its colour, caught with the game's catching animations), and fish at the lots' fishing spots with the game's casting and
  reeling animations, rod in hand, catching what each spot holds by Fishing skill. Finds go in
  the finder's inventory, and the Collection Journal (J) shows everything found and still to
  find, how many the household holds, and sells them all.
- **Grills, coffee makers and alarm clocks** from the catalogue: a grill's *Grill ›* menu
  cooks the game's grilled recipes the Sim knows (hot dogs, tofu dogs, burgers, veggie
  burgers, grilled salmon, tri-tip) with the barbecue animations and serves them like a
  group meal; a hot-beverage maker brews a cup for energy (and a trip to the bathroom
  later); a set alarm clock wakes the household's workers an hour before their shift and
  the schoolchildren an hour before the bus, and stays set in saves.
- **Dirty surroundings**: dirty dishes left about, food left out until it spoils (eight
  hours) and a full trash can give nearby household Sims the game's *Dirty Surroundings*,
  then *Filthy Surroundings*, moodlets. Clearing dishes away fills the nearest trash can;
  once full, someone has to *Empty Trash* (neat Sims see to it, slobs never do).
- **Fish bowls**: a fish from a Sim's inventory (fish now come in the game's qualities, a
  master angler often landing a perfect one) put in a bowl swims there, its own model
  turning slowly in the glass, and can be taken out again; bowls are kept in saves, and
  bowls of perfect fish of different kinds count towards The Perfect Aquarium lifetime wish.
- **Inventories**: each Sim carries what they've picked, caught, found and painted (finished
  paintings, amateur to masterpiece by Painting skill, go in the painter's inventory to sell
  or keep), in stacks on the
  Sim panel's Inventory tab (F9) with the objects' catalogue pictures (produce by quality);
  pick a stack to sell one or all of it, or to eat a piece of produce (a snack's worth,
  more for finer produce). Inventories are kept in saves.
- **Water**: the sea and the ponds ripple, reflect the sky and the sun's glints (more at
  grazing angles), deepen in colour with the depth of water over the ground, and turn clear
  and turquoise in the shallows with a line of foam on the shore.
- **Ponds**: the lots' own sculpted ground and water tables give Sunset Valley its ponds
  and lakes (Summer Hill Springs with its island and bridge, Central Park's twin ponds,
  Crystal Springs between its rocky hills, Stoney Falls, Pinochle Pond, the estates' pools);
  Sims walk around them and fish from the shore.
- **Building**: wall and room tools that draw along the lot's grid (straight or diagonal, with
  a live outline and price), the sledgehammer, floor tiles and staircases up to a new floor
  (stairwell opened, landing floored, Sims climb them), and hip roofs over the rooms in any
  of the catalogue's 27 roof patterns (the plain tiles recovered from each pattern's atlas of
  tiles, ridges and hips); closing off a room lays its
  floor and turns its walls' inner sides to wallpaper. Doors, archways and windows from the
  catalogue snap into walls and cut their openings; knocking a wall down sells what was in
  it. Works on an empty lot or the house that came with it, a floor above the top too, and
  every change is replayed from the save. A Fences tab holds the catalogue's 19 fences and
  hedges (pictured by rendering a piece of each); the fence tool drags runs along the grid
  (straight or diagonal, posts at the ends, standing on the ground out of doors), Ctrl+drag
  takes them down, and Sims walk round them. The pool tool digs a pool out of doors (§40 a
  tile, never under a floor or a garage): the ground opens over it, a coping goes round it
  and the terrain round its edge is laid along the lot's grid; Ctrl+drag fills it in. Pool
  ladders from the catalogue snap to a pool's edge, and Sims swim in the pool they stand
  at (a lot's other pools are left alone).
- **Sound**: the game's own audio, converted once (EALayer3 rewrapped as MP3, EA-XAS decoded to
  WAV): object sounds and Simlish voices cued by the animations, footsteps by floor and
  shoe, interface clicks, stings for promotions, weddings and other moments, birds by day
  and insects at night, music for the menus, Create-a-Sim and buy mode, and the radio
  stations on stereos.
- **Careers** from the game's own career tables: all eleven base-game careers (Professional
  Sports included) with every level's title, pay, hours and workdays, and the part-time jobs
  (bookstore, grocery, spa, mausoleum) for teens. Careers branch where the game's do
  (Criminal into Thief or Evil, Music into Electric Rock or Symphonic, Law Enforcement into
  Special Agent or Forensic Analyst): the promotion there asks which path to take.
- **Career uniforms**: Sims head off to work in their level's uniform from the game's own
  outfits (suits, chef's whites with toques, police blues with the peaked cap, lab coats,
  scrubs, fatigues, the henchman's mask) in the outfit's own colours and fabrics, with their
  own hair colour under a hat. They come home in it and change for bed, a swim or a workout.
- **Changing clothes**: a dresser's *Change Into ›* puts a Sim in their everyday, formal (the
  wardrobe's suits, tuxedos and cocktail dresses among them), sleep, athletic or swim wear,
  or their career's uniform, kept on until it's time for something else; *Plan Outfit*
  opens the wardrobe for their age and gender (hair, tops, bottoms, outfits and shoes, the
  base game's first, pictured by the game's Create-a-Sim thumbnails), the Sim dressing in
  each piece as it's picked.
- **Blinking**: Sims blink every few seconds (the rig's eyelid bones closing over the eyes on
  top of whatever they're doing), and sleep with their eyes shut.
- **Looking at each other**: in conversation a Sim's head turns towards the other's face on
  top of their animation, within a natural reach, easing round.
- **Pie menus** in the game's style: pale bubbles with dark writing, blue under the pointer,
  in a ring round the acting Sim's portrait (what was clicked named beneath it), with
  submenus (›) for long lists.
- **Interface**: the game's own icons and words — moodlets (name, description, time left),
  traits (Create-a-Sim and the Sim's panel) and wishes — read from the game's tuning tables
  (`GameplayData.package`) and string tables, with tooltips on hover. Sims' portraits are
  photographed live (a small studio camera with its own light and backdrop, retaken after new
  clothes or a birthday) for the household buttons, ringed in each Sim's mood colour, for
  balloons about someone, and for the Relationships panel (R): everyone the Sim knows with
  what they are to each other and friendship and romance bars.
- **Saves**: a game saves (as JSON in `saves/`, or the folder `SIMS3_SAVES` names) back to the
  file it was loaded from; a new game takes a file name no other save has ("Goth - Sunset
  Valley (2).json"), so one household never writes over another's game. Each save goes to a
  temporary file first and keeps the one it replaces as `.json.bak`, and saves from older
  versions of the game still load.
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
| O | Opportunities panel |
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
