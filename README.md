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
  roads with markings, water, placed objects and SpeedTree trees drawn from their own 360°
  billboard pictures (each tree turned to the view, showing the side of it that's seen, and
  casting its own outline as a shadow), under a sky made from the game's own sky textures: drifting clouds from
  its cloud noise, the sun's halo, the moon and the night's stars, coloured through the day.
- **Houses**: every lot's real walls (with door and window openings) and floors in the very
  wallpapers, sidings, tiles, carpets and paving the builders chose — the game's texture
  recipes (complates) re-rendered from each lot's designs — plus foundations (as tall as the
  lot makes them: the Goths' manor stands high on its slope), staircases built from the game's
  own stair pieces and railings (as wide as the builders made them), and the house's own
  furniture; the ground never shows through a floor; nearby lots are shown in full detail with
  the game's roofs, distant ones as the game's pre-rendered imposters.
- **Sims**: Create-a-Sim from real CAS parts (browsed as the game's own style pictures), with
  favourites (a food from the game's recipes, a music, a colour, each picked by the game's own
  pictures; a favourite meal is an amazing one, and their favourite music playing nearby is
  enjoyed; shown on the Simology tab and kept in saves), clothes chosen for each of the game's
  outfits (everyday, formal, sleepwear, athletic and swimwear: the wardrobe's clothes of each
  kind, the Sim shown dressed in the one being chosen, and worn when they change into it; kept
  in saves), each item in any of the game's colourways for it (its presets, rendered from
  their patterns and colours: the swatch row under the styles, as the game's; kept in saves)
  and, in Create a Style, with any colour of the palette on each of its colour channels
  (solid colours and patterned fabrics alike; the picker in place of the list, as the game's;
  the item rendered afresh from its pattern with those colours, from the installed game, in
  the background; kept in saves), skin tones, hair colours and animation clips, and what the
  household's Sims are to each other (roommates, spouses, partners, siblings, parent and
  child, as fits their ages: they start married, family or at least friends, and in the family
  tree); the game's three Simlish voices to choose from (each heard as it's picked; taking a
  trait, the Sim says the game's line for it; the town's Sims keep their own); a Face tab with
  the game's eye colours (its iris drawn in the colour, as the game does; babies get a
  parent's eyes) and its beards (full ones drawn as hair is; goatees, chinstraps and the like
  painted onto the face; all in the Sim's hair colour), glasses, lipsticks and eye shadows
  (town Sims wear them now and then too). Every Sim's face has its own shape, as the game's
  face sliders make them: jaw, chin, mouth, eyes, brows, nose, cheeks and head leaning one way
  or the other (the game's own bone adjustments per age and sex), sculpted on the Face tab
  slider by slider, part by part (head, eyes, nose, mouth; the camera closes in on the face),
  kept in saves, and handed down (a baby's face somewhere between their parents'), with the game's body-shape
  morphs (weight and fitness sliders; town Sims keep their builds, working out firms Sims up);
  needs, moods, skills, careers, relationships, autonomy, social interactions; children,
  teens, adults and elders who grow up and grow old (birthdays, new traits, passing away). A
  birthday cake baked at the fridge (§20) lets someone grow up there and then: they blow out
  the candles with the game's animation, the household cheers, and the cake is cut for
  everyone.
- **Gardening** from the game's plant and produce tables: households start with a few seeds
  and buy more at the grocery store (rarer ones as their skill grows); seeds are planted
  anywhere outdoors on the home lot in the game's garden soil and grow (the game's bush, vine
  or tree) while kept watered and weeded, then bear tomatoes, apples, onions, garlic... to
  harvest into the gardener's inventory, to sell or eat. Planting, watering (with the watering can), weeding and harvesting use
  the game's gardening animations and teach Gardening. Stale harvest/weeding requests grant
  no duplicate rewards; failed plant creation retains the seed. Green-thumbed Sims tend their plants
  on their own. Each plant has the game's quality, Horrifying to Perfect: it improves while
  watered and weeded (faster for a skilled gardener) and suffers when neglected, and its
  produce sells at the game's multiplier for its quality (up to four times for Perfect). A
  garden sprinkler, turned on, runs for two hours watering the plants in its reach (its dome
  of spray from the game's model, and droplets), and Sims play in it while it runs, with the
  game's animations.
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
- **Skill journals**: clicking a skill in the Skills tab opens its journal, with what the Sim
  has done with it (hours of strength training and cardio, friends and best friends, jokes
  that landed, dishes prepared and recipes known, fish caught and kinds of fish, harvests,
  repairs, upgrades, discoveries through the telescope, paintings, books and royalties) and
  the game's skill challenges: Body Builder, Fitness Nut, Marathon Runner, Super Friendly, Everybody's Best
  Friend, Comedian, Star Chef, World-Class Chef, Menu Maven, Commercial Fisherman, Amateur
  Ichthyologist, Master Farmer, Botanical Boss, Electrician, Plumber, Tinkerer, Celestial
  Explorer, Teacher Extraordinaire, Chess Grand Master, Brushmaster, Proficient Painter, Master Painter, Prolific
  Writer, Speed Writer and Specialist Writer, with the scripts' own thresholds (200 hours,
  150 fish, 50 repairs…) and the game's texts. Earning one brings the game's notice and,
  where there's something to change, its reward: Fitness Nuts aren't tired by cardio nor Body
  Builders by the gym, Super Friendly Sims' friendships never fade, Comedians' jokes land,
  Electricians are never shocked, Plumbers' repairs never break again, and Master Painters'
  paintings are worth double. Kept in saves.
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
  costs in its dialog (icon, name and description): Steel Bladder (bladder stays full), Dirt
  Defiant and Hardly Hungry (hygiene and hunger decay at one quarter speed), Fast Learner
  (including gardening, repairs/upgrades, career study and rabbit-hole lessons),
  Fast Metabolism, Professional Slacker, No Bills Ever (new household bills stop while a
  reward owner belongs to the household), Opportunistic (bigger
  opportunity rewards), Attractive, Extra Creative (paintings and books earn more), Super
  Green Thumb (improves plant quality at planting, retained in saves), Discount Diner,
  Complimentary Entertainment, Bookshop Bargainer, Haggler, Speedy
  Cleaner (cleaning up twice as fast), Multi-Tasker (more done at work, homework in half the
  time), High Roller (bigger royalties), Vacationer (a missed day at work goes unnoticed),
  Legendary Host (every party a hit), Long Distance Friend (friendships never drift), Fertility
  Treatment (a baby likelier), a new lifetime wish, and a Mid-Life Crisis (all their traits
  chosen again, by the game's trait pictures; Escape thinks better of it, and the lifetime
  happiness comes back); and the reward objects, which go into the Sim's inventory to place on the
  lot: the Food Replicator (a plate of something at the push of a button, eaten at the table),
  the Body Sculptor (an hour inside to come out fitter, slimmer or fuller) and the Moodlet
  Manager (set a mood: flirty, inspired, pumped, having fun, well rested), the Teleporter
  (*Teleport To ›* any lot there is to visit: there at once, no drive), and the Collection
  Helper (gold gem tags in Map View on the gems, metals and space rocks lying about the lot
  being visited, a click to collect one, and where they turn up on other lots, a click to go).
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
  heart. A swimmer whose pool's ladders are taken away can't get out: they swim on, tiring,
  and drown (the game's drowning animation; the tombstone at the pool's side). Each death has
  the game's own last moments, and its ghost its own colour.
- **Meals**: cooking serves eight portions, matching the installed game's group-meal tuning,
  and platters switch to the recipe's original half-depleted model at four servings when available,
  with meals' source objects, recipes, remaining servings and positions retained in saves
  (including delivered pizza and cut birthday cakes),
  with trait-aware post-meal cleanup that preserves queued player commands. Favorite-food
  satisfaction applies to both seated and standing meals. Menus follow the original hours:
  breakfast from 1am, lunch from 11am, dinner from 5pm and weekend brunch from 9am–3pm.
  The meal goes on the game's serving platter on the nearest counter, and the cook calls the household to it (the
  game's wave over): whoever's hungry and not busy with something they were told to do comes
  for a plate. Sims grab a plate and sit down at a dining table or counter stool to eat (the
  game's sit-down and bar-stool eating animations, plate on the table, fork in hand), or eat
  standing when every seat is taken, and leave dirty dishes that neat Sims clear away. Guests
  eat but don't cook, children don't use the stove. Or phone for a pizza: within the hour the
  delivery, in the game's pizza uniform, carries the pizza box in to the kitchen counter.
  What's left of a meal goes in the fridge once it's been out a while (neat Sims see to it
  first, slobs never), to be had another time ("Have Leftovers": a plate of it at the table);
  the fridge keeps a dozen servings, and they're kept in saves. A quick meal is a plate of
  something simple taken out of the fridge and carried to the table; a microwave dinner is put
  in (its door swinging open), checked on while it heats, taken out and carried to the table
  the same way.
- **Recipes** from the game's recipe list: the stove offers "Cook Breakfast/Lunch/Dinner ›"
  and "Cook Dessert ›" with the recipes the Sim knows for that time of day (Mac and Cheese and
  Waffles to start; Goopy Carbonara, Spaghetti, Key Lime Pie, Lobster Thermidor... as Cooking
  grows; meatless ones for vegetarians), the rest from recipe books bought at the bookstore
  at the game's prices. The food is the game's own models in their geometry states — the dish
  on the serving platter, a plateful in front of each Sim — scraped clean once eaten. The
  Culinary Librarian lifetime wish asks for every one. Learned recipes are kept in saves.
- **Props in hand**: the game's own plates, forks, books and guitars, held as its animation
  clips place them (each Sim clip's companion prop clips, and their parent events moving a
  prop from slot to slot: the SimLife Goggles go from the hand onto the face).
- **Endings**: interactions end the way the game's do before the Sim moves on: getting out of
  bed (the cover thrown back), stepping out of the shower, closing the book, folding the paper,
  the last of the washing-up, the knife set down; Sims sit down into chairs, sofas and bar
  stools and get up out of them with the game's animations.
- **Objects that move**: objects with moving parts are skinned to their own rigs, and play
  their half of the Sim's animation in step with it: the fridge door swings open, food comes
  out, and the door shuts again; shower doors, dresser drawers, swings and the rest likewise.
  Left alone, they come back to rest.
- **Games**: the Maxoid Game Simulator, sitting on the floor with the game's controller in hand
  (children too), and the SimLife Goggles, put on to explore virtual worlds (action, fantasy
  and space adventures, teens and up), with the game's animations; both are great fun. Teddy
  bears and the little toys (a boat, a rocket, a pony, a robot, a dragon...) are played with by
  children (sitting with the toy in their hands) and toddlers (hugging it), with the game's
  clips for each.
- **Thought and speech balloons**: the game's own balloon art and balloon table. Sims think about
  needs running low, new moodlets and their traits, dream while asleep, and take turns showing
  what they're talking about (small talk about the weather, their work or interests; insults,
  compliments and flirts with the game's like and dislike marks).
- **Socials** by the game's categories (Friendly, Funny, Romantic, Mean, Special) with its own
  animations: chatting, jokes, funny and dramatic stories, bragging, goofing around, silly
  faces, crying on a shoulder, cheering up and apologising; flirting, embracing, gazing into
  eyes, back rubs, leaping into arms and kisses up to the dip kiss as romance grows; insults,
  yelling, irritating, slaps, declaring a nemesis and fights (the grapple and headlock, won
  by the athletic and the brave, the loser embarrassed) for those who don't get on. On the
  phone (the game's cell phone in hand, with its dialling and chatting animations) Sims chat
  with friends who aren't about — half an hour's talk, good for Social and the friendship on
  both sides (lonely Sims at home alone call a friend by themselves) — or invite them over
  (they come up to the front door and ring the bell, the game's ring, and wait there until
  someone of the household goes to greet them and let them in; left two hours, they go home),
  order pizza and call the services. Friendships fade after three days without seeing or
  speaking to each other (good friends more slowly; spouses, partners and family stay close),
  and grudges soften.
- **Families grow**: couples can try for a baby; pregnancy brings morning sickness, then the
  game's pregnant waddle once she's showing (and no running), then a baby in a crib (the
  game's swaddled baby model). Grown-ups feed, change, cuddle and settle babies (cradled in
  their arms) and pick up, read to and put toddlers to bed; toddlers crawl about until a
  grown-up has taught them to walk (three lessons, with the game's kneel-and-hold-hands
  animation; talking is taught the same way, and both show on the toddler's Skills tab), play
  with toy boxes, xylophones and peg boxes, nap in cribs and cry when neglected.
- **Town families**: every world's premade households (the Goths, Landgraabs, Altos, …) with
  their names, bios, family portraits, traits, careers, skills, marriages and homes — play one
  of them, or meet them around town.
- **Play**: choose a furnished house (with the game's lot pictures) or an empty lot (moved
  onto with a starter home of furniture on a wooden deck: real floor in the catalogue's
  Rustic Wooden Planks, over the highest ground under it on a foundation down the slope, to
  build on, paint a plank at a time or wall in as rooms), buy
  mode with the game's own catalogue pictures (base game and every installed pack's) and
  every catalogue object on its tab by the game's own grouping (smoke and burglar alarms,
  gnomes, sprinklers, picnic tables and trees under Electronics, Outdoors and Misc), each in
  any of the game's designs for it (the catalogue's colour and pattern presets, drawn from
  their texture recipes: the country couch in blue dots, green stripes, red plaid, cowhide or
  roses; picked from swatches while it's in hand, and kept in saves) or, in Create a Style,
  any colour of the palette on each of its colour channels (solid colours, and fabric, wood
  and the like shifted to the colour as the game's colour wheel does, keeping the grain;
  rendered afresh from the installed game in the background); the eyedropper takes up a new
  one of any object clicked in its design (the town's own furniture too), or the covering of
  a wall side or floor to paint with; paintings, mirrors and
  wall lamps hang on the wall under the pointer, and go with it when it's cut away; the
  town's houses are furnished in the very designs their builders chose (each placed object's own, drawn from
  the lot data: quilts, cribs, counter tops and upholstery as they were),
  multi-storey navigation by stairs, walls up, cut away or down (the HUD's walls button or
  Home), day/night with lamps and street lights. Sims deciding on
  something by themselves think of it in a balloon with its picture.
- **Build mode painting**: the catalogue's wallpapers and floor coverings (every base-game
  pattern with its name and price, swatched from its texture recipe) on Wallpaper and Floors
  tabs; click a wall side or one floor tile, or Shift-click to cover its room. Room fills follow
  current wall boundaries, including diagonal partitions, and leave the other side untouched.
  Identical coverings are skipped; prices reflect only the changed surface area. Each comes in
  the game's colour presets for it (swatches under the patterns) and, in Create a Style, with
  any colour of the palette on each of its colour channels (solid colours, tinted patterns,
  and woods and fabrics shifted to the colour keeping their grain). Repainting is charged,
  sounds like the game's build tools, and is kept in saves. Clicking bare ground with a floor
  pattern lays paving that follows the current terrain, including sculpted slopes; undo restores
  the previous ground and funds, and saved paving retains its corner heights.
  Shift-click or the room-fill button also lays paving throughout a bare enclosed room,
  respecting diagonal walls and pools. Outside an enclosure it still affects only one tile.
  Hover outlines show the wall sides or floor triangles that will change, following paving
  slopes; green means affordable and red means insufficient funds. Previewing changes neither
  the lot nor its funds/save history.
  A cursor-side price quote shows the exact charge before clicking, or “No change · §0” for
  identical coverings; unaffordable quotes are red and cannot alter the lot.
  Hover targeting is cached per surface and refreshed after geometry, terrain, texture or
  fill-mode changes, avoiding repeated room searches while the pointer stays on a surface.
  The covering eyedropper samples the exact triangle of diagonal flooring, supports outdoor
  paving, and chooses the nearer covered floor or wall instead of always preferring a wall.
  Newly rendered custom coverings keep a style recipe beside their cached texture. Sampling
  them restores the source pattern's price, preset and editable colours, including after reload.
  Custom furniture textures likewise retain their preset and colour recipe when picked up,
  sampled with the eyedropper or retrieved from inventory, so further edits start from that design.
  Wallpaper picking and outlines follow each wall's authored foundation/split-level elevation.
  Wall-mounted objects also snap at that elevation; multi-tile objects cannot bridge wall
  sections with different base heights.
  Floor picking uses the nearest occupied tile elevation, including raised/split-level tiles;
  holes expose lower visible floors instead of selecting an empty storey plane.
  Upstairs furniture requires support at its placement height across the entire footprint,
  including interior tiles; it cannot bridge holes or lower sections of a split-level floor.
  Existing door/window slots reject overlapping openings in both preview and placement;
  moving an opening ignores its own retained source so it can return to the same slot.
  Doors, windows and wall decorations snap to 45-degree walls as well as axis-aligned walls,
  with diagonal grid-section widths, facing and occupancy checks.
- **Out on the town**: Sims drive to the parks, the library, the gym, the beach and the
  other community lots (from the lot's pie menu or its map tag) and use what's there: chess
  tables, computers, bookshelves, swings, grills, restrooms. Each lot gets its own walk grid
  and floors, upstairs as well (reached by its stairs and elevators: Bridgeport's rooftop
  clubs at the top of their towers, the camera rising with the Sim and the floors above
  them out of the way), its walls come down as the camera moves in, Sims on it keep to it in
  their own choices, townies turn up during the day to use the place too (and to meet), and
  "Go Home" drives them back while the household carries on at home. Bridgeport's
  apartments and storefronts stand inside building shells, taken away like walls and roof
  when the camera comes in close.
- **Sim panel tabs**, as the game's: Needs; Skills (each skill learned with its icon, level
  and progress to the next); Career (the job and level, hours, days and pay, the performance
  meter and the next promotion, and how they go about their work: normally, working hard —
  half as fast again to promotion but tiring and no fun — taking it easy, hanging with
  co-workers, or studying the career's skill on the job; elders can phone to retire, on a
  pension of half their average day's pay, paid each morning); Simology (the lifetime wish and how far along it is, traits
  with what they mean, lifetime happiness and the rewards bought). F5 to F8 switch tabs.
- **Fences and railings**: every lot's fences (picket, rail, iron, garden edging) and its porch
  and balcony railings, from the lot's fence posts and the catalogue's fence pieces (straight
  and diagonal runs, posts); Sims can't walk through them.
- **Pools**: the lots' pools (Le Petit Shark Pool Center, the apartments', the gym's, estates')
  are let into the ground with their tiled floors and sides, a stone coping and clear turquoise
  water over the mosaics; the terrain is left open over them. Sims swim from the ladders: they
  change into swimwear (the game's own: trunks and a bare chest, swimsuits and two-pieces),
  climb down, swim about the pool with the game's swim cycle (building Athletic skill) and
  climb out back into their clothes. From a diving board they walk out and dive in (the game's
  dives, cannonball included), then swim. At bedtime they change into their sleepwear (pyjamas,
  nightgowns, boxers), and into athletic wear to work out, and dress again after.
- **Jogging**: clicking the selected Sim offers "Go Jogging": out to the street in athletic
  wear and up and down its sidewalk at a jog (the game's jogging clip) for an hour, or until
  they're worn out, then home (and usually to the shower). It trains Athletic, builds fitness,
  and counts the game's six kilometres an hour towards the Marathon Runner challenge. Athletic
  Sims go jogging on their own by day, and some of the town's passers-by jog past in their
  athletic wear (most of them mornings and evenings). The sidewalk they all use runs beside
  the street nearest the lot.
- **Walk styles**: each way a Sim goes is gone in the game's walk style for it, by the game's
  own rules: about their business on their own lot they walk; sent across it, they hurry (the
  fast walk) the long ways; elsewhere the further the faster, from a walk to a fast walk, a
  fast jog and a run (10, 15 and 30 m on; a child's 5, 10 and 20, an elder's 15, 25 and 45),
  going of their own accord by chance up to that. Children walk or run, the fatigued don't
  jog, the athletic (skill 5) run flat out, and the heavily pregnant waddle. Each style is the
  game's clip, played in step with the Sim's pace so their feet stay planted (the paces
  measured from the clips themselves: a walk 1.8 m/s, a run 5.7).
- **Seasons and weather**, from the Seasons pack's own tuning (its `Seasons` table and
  SeasonsManager's managers): summer, fall, winter and spring come round a week each, the days
  shorter away from summer (the sun up later and down earlier), with the season, its day, the
  temperature and the weather under the clock. Temperatures run between the season's lows and
  highs for the morning, noon, evening and night, warmer or cooler from day to day. The
  weather comes by its chances in the season among the temperatures it comes at: sun, rain,
  snow, fog and hail for their hours, rain and snow light, moderate or heavy. Clouds gather
  and the light dims under them; rain streaks down round the camera, snowflakes drift and
  hail pelts; fog closes in; heavy rain brings lightning, the sky flashing and the game's
  thunder after, sooner the nearer the strike; and the game's rain, snow, hail and wind loops
  play, with the seasons' own birdsong when it's dry. Rain wets the ground and the roads
  (darker, glossier) and melts snow, which settles when it snows (frost first, the ground
  white as it deepens) and melts as it warms, all at the game's rates. Trees turn orange, gold
  and red in fall and drop their leaves, bare through winter until spring (the conifers and
  palms stay green), and frost over in the snow. Sims feel it: out of doors their temperature
  heads for the world's (indoors back to comfortable), with the game's Getting Chilly, Teeth
  Chattering, Frostbitten, Getting Warm and Sweating Profusely moodlets, and out in the rain
  long enough they're Soaked; in rain and hail they leave what's out in it alone. Out in the
  cold (below 50°F) they put on the pack's outerwear (coats, parkas, snow pants, boots,
  snowsuits; an Outerwear outfit chosen in Create a Sim and kept in saves) and take it off
  again indoors, and the townies walking by wrap up too. Out in the snow (from the ground's pie
  menu) Sims build snowmen, rolling and patting their three balls and giving them a face as
  the snowman grows, and lie back to make snow angels, both melting away with the snow (and
  knocked down or stomped out, if they like); out in snow or rain they catch flakes or drops
  on their tongues; and through fall leaf piles gather on the home lot every ten to twelve
  hours, to play in, jump into and rake up. All with the pack's own objects and animations.
  The season
  and the weather are kept in saves; the World Adventures destinations have none.
- **Pets**, from the Pets pack: cats, dogs, little dogs and horses (and their young) in the
  game's own bodies on their own skeletons, each in one of the game's 233 breeds (its coat
  painted from the breed's colours and markings through their masks over the fur's shading,
  with its tail, ears or mane), moving with the pack's own animations: walking, standing
  about looking round, sitting, lying down, asleep. A stray cat and dog wander the sidewalk in
  front of the lot by day, stopping to sit or lie about, and curl up asleep at night. The
  premade households' pets come home with them (Appaloosa Plains' cats, dogs and horses: the
  Lionhearts' four cats, the Johnsons' two horses...), each in a breed of its kind, and potter
  about the lot (the horses out of doors, swishing their tails and pawing the ground), kept in
  saves. Clicking a pet offers what the pack has Sims do with it (pet it, let it sniff a hand,
  praise it, hug it, feed it a treat, scold it; rub a horse's neck): the Sim walks up to it,
  the pet stops and turns to them, and the two play their sides of the game's animation
  together (kneeling to stroke a cat, which looks up at them), for the Sim's fun and social.
  Grown-ups can adopt a pet by phone (a cat, kitten, dog, puppy, little dog, horse or foal, for
  its fee): one of the game's breeds of its kind comes home with a name of its own.
- **Supernaturals**, from the Supernatural pack (and Late Night's vampires): vampires,
  werewolves, witches, fairies and zombies, chosen in Create a Sim's Life State row and kept in
  saves. Moonlight Falls' families come as the pack has them, a third of them supernatural (the
  Hoppcrafts are werewolves), and some of its townies. Vampires are pale
  with red eyes, drink plasma juice from the fridge rather than eat, dash when they run and
  feel the daytime sun (Too Much Sun) out of doors; zombies are grey-green and shamble and
  groan. The moon waxes and wanes over the game's eight-day lunar cycle (shown on the clock),
  and under the full moon werewolves turn, with the pack's transformation, into their wolf
  form (its shaggy hair and beard, darker and furrier, amber-eyed, prowling and sniffing about)
  and howl at it, turning back at dawn. Fairies wear the game's glowing wings on their backs,
  fluttering. Each kind has its own socials, with the pack's animations: a vampire's hypnotic
  gaze and talk of plasma, fairies' frolics, tricks and bragging about their wings, werewolves'
  sparring, and witches' charms and curses cast with the wand.
- **Water effects**: fountains gush and fill their basins, showers spray from the head and taps
  run while someone's at them, each from the object's own effect slot (the game's RSLT slots).
  TVs show a programme on the screen (at the TV's screen slot) while anyone's watching, its
  light flickering on the room.
- **Lots' own ground**: lots keep their sculpted ground; its dips are carved back into the
  flattened world (Central Park's sunken plaza with the fountain basin set into it, hollows on
  the beaches), and ground-level paving follows the ground.
- **Cooking dinner**: the cook first fetches the ingredients from the fridge (its door swinging
  open and shut), carries the tray to the nearest counter and chops them there, then takes the
  frying pan to the stove, each with the game's animations and props; the meal cooked, the
  platter is carried to the nearest counter or table and set down, and dinner is served.
- **Television**: *Watch TV* turns the set on, then the Sim sits down on a sofa or chair facing
  it to watch (the game's seated and armchair TV animations; standing when there's no seat), the
  screen flickering with pictures as long as someone watches.
- **Reading**: *Read a Book* at a bookshelf takes a book down (the game's animation), carries it
  to the nearest free sofa or chair and reads it sitting there, turning the pages (standing at
  the shelf when there's no seat free). The morning paper is picked up off the ground and carried
  to a seat to be read the same way (or read standing where it lay).
- **Fireplaces**: Sims light a fire with the game's animations; it burns a few hours, flames
  dancing in the hearth and a flickering warm light on the room, and they warm their hands by
  it or put it out. Now and then a spark catches the floor in front. Candles are lit and blown
  out too: a little flame and a soft flickering light for a few hours.
- **Homework**: children and teens come home from school (Monday to Thursday) with homework,
  and sit down at a table to do it (the game's homework animations; sooner for bookworms and
  geniuses); done, it lifts their grades; left undone, it drags them down. A teen or grown-up
  can Help with Homework (a Friendly social, offered while there's homework): it's done, with
  better grades, the helper learns a little Logic, and the hours count towards Teacher
  Extraordinaire (whose teachers get through it twice as fast).
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
- **Changing household**: the game menu's "Change Household" lists the town's other families
  (each with who's in it, their money and their home): the households played before, then
  the world's families never played. The household being played stays in town just as it is,
  kept in the save to come back to, and the one chosen takes over at the same time, in the
  same town, with the town's story and family tree as they stand. A household played before
  picks up where it was left; a town family moves into their own home. Who knew whom goes
  across both ways, and the families played before are about town while another is played
  (the town's story leaves them as they were); their homes stand as they left them, with the
  walls and floors they built and painted and the furniture they bought (and not what they
  sold). Changing doesn't save the game by itself.
- **Edit Town** (game menu): the town from above with every lot outlined (your home gold,
  homes green, empty homes white, community lots blue) and a list of every lot with who lives
  there. Pick a lot to play its household, or evict them to the household bin (a household
  played before has its furniture sold for four-fifths of its price; the house stands); pick
  an empty home to move a family from the bin into it. A house no one lives in can be
  bulldozed (the lot left empty, to move onto and build on), an empty lot can have any of the
  town's houses on lots its size put down on it (furnished as it stands, roof and all), and a
  lot no one lives in (without a career's rabbit hole) can change type: residential, or a
  park, gym, library, pool, art gallery, beach or fishing spot to visit. Where everyone lives,
  and all the town's changes, are kept with the town's story in the save, and the lot chooser (a new game, moving house) won't move anyone
  into a home already lived in.
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
  fire animations. Anyone caught in the flames burns, in flames head to foot, dashing about in
  a panic (the game's on-fire run) between bouts of flailing: they may stop, drop and roll, or die
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
  away and bring them back. A household with a car of its own takes it out on its trips: it
  leaves the driveway and drives off with them, and comes back when they do.
- **Collecting and fishing**: the world builders' spawners leave gems, metals and space rocks
  about the community lots (from the game's own tables, with their odds and prices); Sims
  crouch to collect them, catch the butterflies flitting about by day and the beetles in the
  grass (each kind in its colour, caught with the game's catching animations), and fish at the lots' fishing spots with the game's casting and
  reeling animations, rod in hand, catching what each spot holds by Fishing skill. Finds go in
  the finder's inventory, and the Collection Journal (J) shows everything found and still to
  find, how many the household holds, and sells them all.
- **Bars**: *Make a Drink* at any of the catalogue's bars: the Sim pours from the blender,
  blends, serves and sips, with the game's animations and props.
- **Grills, coffee makers and alarm clocks** from the catalogue: a grill's *Grill ›* menu
  cooks the game's grilled recipes the Sim knows (hot dogs, tofu dogs, burgers, veggie
  burgers, grilled salmon, tri-tip) with the barbecue animations and serves them like a
  group meal; a hot-beverage maker brews a cup for energy (and a trip to the bathroom
  later); a set alarm clock wakes the household's workers an hour before their shift and
  the schoolchildren an hour before the bus, and stays set in saves.
- **Fire pits, picnics, toys and the rest of the catalogue**: a fire pit is lit with the
  lighter (flames in its bowl, a glow on the ground about it), hands are warmed by it and
  marshmallows roasted on the skewer and eaten off it, and it's put out with the poker; a
  picnic basket gives a picnic on the ground off paper plates; a buffet table serves a plate
  of food; children bake pretend cakes in the toy oven, bowl and spoon in hand; men and boys
  use the urinal (whistling); recycling bins take the trash; a bath can be a bubble bath
  (with bubble bath in the house) or a play with the rubber duck (with one about); a
  football or baseball about lets two Sims *Play Catch* a few metres apart, throwing it soft,
  hard, wimpy or showing off; and a house phone on the wall or a table opens the phone's
  menu. All with the game's own animations and props.
- **The packs' games and instruments**: pool tables (the cue fetched from the rack, shots
  round the table as the balls roll), dartboards, bowling lanes (the ball fetched from the
  return, bowled, and the pins' fall watched, in turn), karaoke machines with the microphone,
  the arcade cabinets, the claw machine, Whack-a-Gnome, skee-ball, horseshoe pits,
  shuffleboard, the mechanical bull, trampolines and sandboxes; and Late Night's grand piano
  and keyboard and drum kit (sticks in hand) and Into the Future's laser harp, each building
  its skill (Piano, Drums, Laser Harp, shown with the game's icons). Sims use each from the
  spot the game's own routing slots put them (the piano bench's side, the bowling lane's
  end, the pool table's edge), so the game's animations line up with the object, whose own
  half of each animation plays with them; a jukebox plays music to dance to.
- **Dirty surroundings**: dirty dishes left about, food left out until it spoils (eight
  hours) and a full trash can give nearby household Sims the game's *Dirty Surroundings*,
  then *Filthy Surroundings*, moodlets. A dish cleared away (picked up with the game's
  animation) has its scraps in the nearest trash can and goes in the dishwasher, if there's
  one working, or is scrubbed at the sink, carried there in hand (the game's carry animation
  over the arms while the legs walk); plates taken from a platter are carried to the table the
  same way. Once full, someone has to *Empty Trash* (neat Sims see to it, slobs never do),
  pulling out the bag, and an indoor can's bag is carried out to the trash can outdoors. A trash compactor holds three times as
  much. Dishwashers break now and then, and are repaired with the game's animations.
- **Fish bowls**: a fish from a Sim's inventory (fish now come in the game's qualities, a
  master angler often landing a perfect one) put in a bowl swims there, its own model
  turning slowly in the glass, and can be taken out again; bowls are kept in saves, and
  bowls of perfect fish of different kinds count towards The Perfect Aquarium lifetime wish.
- **Paintings**: at an easel, *Paint ›* offers a small, medium or large canvas (longer to paint,
  worth more), and what a Sim paints is one of the game's own pictures, from the Painting
  skill's picture table: chosen by their skill, in their traits' version of it (an evil Sim's,
  a gloomy one's, a virtuoso's) and now and then a brilliant painting or a masterpiece at higher
  skill. While they paint, the canvas stands on the easel's ledge for its size, the game's
  work-in-progress strokes and then the picture. Finished paintings go in their inventory with
  their picture; *Hang on a Wall* holds
  one up in Buy mode to hang on any straight wall (not over a window or door), where it's the
  game's canvas at its size with the picture on it. Hung paintings can be moved or sold (for
  what they're worth), go back to the inventory if not put up, and are kept in saves.
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
  (stairwell opened, landing floored, Sims climb them; put together from the game's stair
  pieces with railings up both sides, in the house's own stair style or, on a lot with none,
  the one most of the town's houses have), and hip roofs over the rooms in any
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
  at (a lot's other pools are left alone). A Terrain tab paints the ground on the lot with
  the world's own terrain paints (pictured from their textures), in three brush sizes, or
  erases back to how the world had it; its terrain tools raise, lower, flatten and smooth the
  ground (not under the house, the pool or anything standing on the lot, nor at its edges).
  Paint and sculpted ground are kept in saves.
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
  opens the wardrobe for their age and gender, for any of their outfits (everyday, formal,
  sleepwear, athletic, swimwear: they put it on to plan it; hair, tops, bottoms, outfits and shoes, the
  base game's first, pictured by the game's Create-a-Sim thumbnails), the Sim dressing in
  each piece as it's picked. A mirror's *Change Appearance* does the same for their looks:
  hairstyle and hair colour, facial hair, glasses, lipstick and eye shadow.
- **Blinking**: Sims blink every few seconds (the rig's eyelid bones closing over the eyes on
  top of whatever they're doing), and sleep with their eyes shut.
- **Looking at each other**: in conversation a Sim's head turns towards the other's face on
  top of their animation, within a natural reach, easing round.
- **Pie menus** in the game's own bubbles (its `HUDPieMenu` layout): the first option at the
  top and the rest round clockwise, each in the bubble whose place is nearest (its tail or
  groove towards the middle) widened to its words, lit under the pointer with the game's own
  pictures and colours; the acting Sim's face in the middle (what was clicked named beneath
  it), with submenus (›) for long lists. The interaction queue at the top left and the
  notices at the top right are the game's own too: each queued action in its frame with the
  object's picture or the other Sim's face, the running one's progress bar, a cross to cancel;
  each notice a speech card with the Sim's face or a system card, closed with its cross.
- **The game's own HUD**: the live-mode interface is the game's, drawn from its own layouts
  (`UI.package`'s window trees, pictures and fonts: every caption in its Helvetica Rounded),
  and driven as its own interface code drives it: the puck at the bottom left (live, buy and
  build; the camera's orbit, zoom, tilt and views; walls up, cutaway or down; floors; the
  household's funds; the snapshot and options buttons), the Sim display (the selected Sim's
  bust; the clock's pause, play, fast and faster buttons with the day and time, the season
  and weather on hover; the mood meter filled and coloured by the game's own thirds and
  tuning, glowing in the band that earns lifetime happiness; the moodlets in the game's
  green, red or grey cells with their time left, scrolled three by two; the offered wish
  in its staging area, paged and clicked to promise, the four promised wishes in their slots
  and the lifetime wish in its frame), the skewer of the household's faces up the left, each
  ringed in its mood's colour (click to select, again to go to them), and the info panels
  under their tabs: the motives' bars filled and coloured as the game's (green, fading to
  amber and red as a need runs low); Skills (each skill's icon and its level in the game's
  bubbles, its journal a click away; the wheel scrolls); Simology (the name, the age bar from
  this life stage's icon to the next's with the days left on hover, the traits with their
  icons and meanings, the favourite food, music and colour, the family tree button); Career
  (the job's icon and title, its pay, hours and working days, the time till work, the
  performance meter up or down from the middle; school and its grade for children and teens;
  at work, its button chooses how they work); Inventory (the stacks in the game's cells with
  their pictures and counts, each clicked for its menu: sell one or all, eat, hang, place;
  the cell phone opens the phone, the green book the collection journal). Relationships,
  Opportunities and the lifetime rewards open their windows. Buttons light up, press and grey
  out with the game's own pictures. The Sim's own menu has their phone too.
- **The game's own Buy Mode catalogue**: its function categories and subcategories, and its
  room pictures with clickable furniture groups, from `BuyCatalog` and each object's OBJD
  flags. The original catalogue cells expand upwards, scroll with the wheel, and use the
  game's skinned scrollbar (arrows, paging and draggable thumb). The preview shows an object's
  picture, name, price, description and design swatches; unaffordable prices are red. The
  day/night button previews the lighting without changing game time, and the grid button
  shows or hides the home lot's rotated tile grid. Shopping stays paused and restores the
  previous speed on return to Live Mode, including a game that was already paused.
  Objects snap to quarter tiles on the lot's own grid, on the floor in view, with a green or
  red placement outline; objects must fit inside the lot, upper-floor furniture needs floor
  underneath (including diagonal tiles and interior gaps), and ordinary furniture cannot
  overlap a pool or straddle a wall. Hold Alt for free positioning between grid points.
  Placement checks use the floor actually hit by the pointer. Moving furniture keeps the original
  object, with its upgrades, breakage, contents, lights and animated parts; cancelling puts it
  back intact.
  Buy and Build Mode's undo/redo buttons (Ctrl+Z / Ctrl+Y, or Ctrl+Shift+Z) share a history for
  furniture purchases, moves, designs and sales; walls, rooms, floors, stairs, fences, pools,
  wallpaper and roof patterns; and terrain painting/sculpting. Each held terrain brush is one
  edit. Undo restores funds, save records and original furniture entities, including doors and
  windows sold during demolition. Up to 100 transactions are retained across Buy/Build switches;
  returning to Live starts a fresh history. Finish a brush stroke or put down/cancel an owned
  object before reversing a transaction.
  Purchase wishes settle on return to Live Mode, for purchases the household actually kept.
  The Design Tool (R in Buy/Build) edits an existing object's presets and colours in place:
  Apply/Enter commits, Cancel/Escape restores the original, and Buy Mode undo reverses the edit.
- **Build Mode navigation**: the original house diagram opens walls, wallpaper, floors, doors,
  windows, roofs, fences, stairs, pools and terrain tools. The original puck provides mode,
  camera, floor, wall-view, lighting and grid controls. Wall/room and rectangular-pool tools
  use their original panels and Back button; catalogue browsers appear above the navigation.
  Floor and wallpaper catalogues use the original Build panels and expandable thumbnail grids,
  with wheel/skinned-scrollbar navigation and single-surface/room-fill buttons. Pattern names
  and prices appear on hover; selecting one retains its colour presets and Create a Style.
  Original material tabs filter by the catalogue's own category flags (wood, tile, paint,
  wallpaper and the other wall/floor groups), with All restoring the complete list.
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
- **Options**: Escape pauses with a game menu (resume, options, save, save as a new game (in a
  file of its own, never over another), change household, main menu, quit); the options set
  master, music, effects, voice and ambient levels, aging and life span (short to epic), free
  will, shadows and the frame-rate counter, kept in `settings.json`.

## Parity status

Full parity is still a work in progress. Remaining milestones include the original-layout
complete Build Mode, Create-a-Sim and main-menu interfaces; household furniture
inventory; complete object placement and routing compatibility; the rest of the expansions'
gameplay; and systematic side-by-side visual and interaction checks against the original.

## Controls

| Input | Action |
|---|---|
| Left-click Sim / object | Pie menu of interactions |
| Left-click ground | Go here |
| WASD / arrow keys (Shift = faster) | Pan |
| Q / E | Rotate |
| T / G | Tilt the camera up / down |
| Mouse wheel, Z / X, = / − | Zoom |
| Page Up / Page Down | View the floor above / below |
| Tab | Next Sim |
| C | Centre the camera on the selected Sim |
| Space | Pause |
| 1 / 2 / 3 | Game speed |
| F1 / F2 / F3 | Select Live / Buy / Build mode |
| B | Toggle Buy mode (, and . rotate, Delete sells; Esc cancels the tool, then leaves) |
| H / E / K in Buy or Build | Hand / eyedropper / sledgehammer |
| R in Buy or Build | Create a Style for an existing object; Enter applies, Esc cancels |
| Ctrl+Z / Ctrl+Y in Buy or Build | Undo / redo edits (Ctrl+Shift+Z also redoes) |
| R | Relationships panel |
| O | Opportunities panel |
| Esc | Game menu: pause, options, save, quit |
| Print Screen or F12 | Screenshot (into a Screenshots folder beside the saves, or `SIMS3_SCREENSHOTS`) |

## Layout

| Crate | Purpose |
|---|---|
| `s3pkg` | DBPF packages, RefPack, Resource.cfg install discovery |
| `s3formats` | Decoders: models, textures and the TXTC compositor, CAS, rigs and clips, worlds, lots |
| `s3bake` | The converter and the cache format (`s3bake --info <world>` describes a baked world, `--dump-sound <name> <dir>` extracts a baked sound) |
| `s3tool` | Inspection tools used while reverse-engineering formats |
| `game` | The game (`sims3`) |

Format notes are in `docs/formats/` (the interface's layouts in `ui.md`).

## Testing switches

`sims3 --world <name> --lot <id | house> --cam x,z,dist,yaw,pitch --hour <h> --speed <0-3>
--view-level <n> --do <interaction> --screenshot <png> --shot-delay <s> --exit-after-shot`,
and `--ui-flow <dir>` clicks through the menus saving a screenshot of each screen.
`--do "Paint: Large Canvas"` paints on that canvas; `PAINTINGS=<level>` paints six (three into
the inventory, three hung on the nearest walls, the camera on them), `PAINTINGS=look` puts the
camera on a hung painting (`PAINTINGS=easel`: on the easel), `PAINTINGS_BUY=1` holds one up in Buy mode and `BUY_CLOSE_AT=<s>`
closes Buy mode then, logging the inventory. `DIVE=1` puts a diving board on the lot's pool.

`PRESS_KEY=F2@5;F3@7;F1@9` presses a sequence of keys at the given seconds;
`UI_CLICK=05b706c0@6;05b706d0@7` clicks visible original-layout controls (hex control ids).
Build/Buy input scripts and their regression probes count seconds from entering Live Mode;
loading and cache conversion time do not consume their scheduled delays.
`OBJ_STYLE=1` with `PRESS_KEY=F2@5` and `BUY_PICK=0@6` renders a custom furniture design,
checks its cached recipe, and samples the placed object to verify editable preset/channel
restoration. Capture after 32 seconds.
`GIVE=Tomato/3*2` with `EAT=check` verifies a produce item is consumed before eating
finishes, then cancels the action and checks that the snack stays consumed. Run at speed 1
and capture after 22 seconds; the log reports `inventory eating PASS`.
`HALF_MEAL_TEST=1` creates a platter using an original recipe, reduces it to four servings,
and verifies its full mesh is replaced by the half-depleted mesh exactly once. Capture after
12 seconds; the log reports `half platter PASS`.
Run that probe with `--save-at 9` and an isolated `SIMS3_SAVES` directory, then use
`HALF_MEAL_TEST=restore --load 0` (the variable set in the environment) to check restored
servings and half-depleted geometry, plus a pizza's source model and remaining slices;
the log reports `saved platter PASS`.
`CLICK_AT=1000,450@8` clicks at window-relative coordinates (`CURSOR_AT` moves without clicking).
`BUY_HISTORY_TEST=1` verifies selling, undoing and redoing furniture transactions in the running
game. Use `BUY_HISTORY_TEST=buttons` with `UI_CLICK=05ac150d@7;05ac150e@8;05ac150d@9` to verify
the HUD controls, or `BUY_HISTORY_TEST=keys` with `PRESS_KEY=F2@5;Ctrl+Z@7;Ctrl+Y@8;Ctrl+Z@9`
to verify keyboard shortcuts. Both check funds, save records, identity, upgrades and breakage.
`BUY_DESIGN_TEST=apply` verifies in-place design changes and undo/redo;
`BUY_DESIGN_TEST=cancel` verifies cancellation (`preview` leaves the editor open for screenshots).
`BUILD_NAV_TEST=1` with `PRESS_KEY=F3@5` and
`UI_CLICK=1002@6;302@7;06e89130@8;1013@9;06e89130@10;1003@11;8ffffa04@12;8feffa03@13`
verifies original Build navigation and mode switching; capture after 15 seconds.
`BUILD_HISTORY_TEST=room`, `pool`, `demolish`, `terrain` or `sculpt`, with `PRESS_KEY=F3@5`
and `--view-level 1`, performs a real drag then clicks the original Undo/Redo buttons. It checks
building data, furniture identity, funds, save records, terrain heights and paint-map pixels;
capture after 11 seconds.
`BUILD_HISTORY_TEST=floor` and `floor-room` additionally verify single-tile and Shift-room
covering, including undo/redo through the original Build buttons.
These covering probes also verify that hover quotes match the charge and previews leave saves
and funds untouched. `BUILD_HISTORY_TEST=floor-poor` checks rejection with insufficient funds.
`BUILD_HISTORY_TEST=wall` and `wall-room` cover single-side and Shift-room wallpaper selection,
quoted charges, zero-cost repeated designs, and exact undo/redo through the original controls.
`WALL_SNAP_TEST=1` checks both sides of the loaded lot's axis-aligned and diagonal walls for correct placement
height, half-tile offset and facing, plus occupied versus self-excluded opening slots;
capture after 9 seconds.
`BUILD_HISTORY_TEST=pave` checks laying new outdoor flooring and reversing it; paving follows
the current terrain (including sculpted slopes), respects pools and lot bounds, and saves its heights.
`BUILD_CATALOG_TEST=1`, with `PRESS_KEY=F3@5` and `UI_CLICK=1012@6` (floor) or `1003@6`
(wallpaper), checks native catalogue expansion, scrollbar movement, pattern selection, surface/
room tools, material-category/All filtering and hidden Live panels. Capture after 12 seconds.
`BUY_MOVE_TEST=move` or `cancel` checks that a piece of furniture keeps its identity, upgrades
and breakage after a scripted placement or cancellation.
`s3tool uiwindow <layout>` inspects a baked window tree; `s3tool uilayout <UI.package> <layout>`
prints its source XML, and `s3tool uibuy` lists the catalogue's categories and object counts.
