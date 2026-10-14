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
  billboard pictures (each tree turned to the view, showing the side of it that's seen, lit as
  a rounded crown the way the game's leaves are, bright on the sun's side and shaded on the
  far side and underneath, close up drawn in with leaves where the picture would blur, and
  casting its own outline as a shadow), under a sky made from the game's own sky textures: drifting clouds from
  its cloud noise, the sun's halo, the moon and the night's stars, coloured through the day.
- **Houses**: every lot's real walls (with door and window openings) and floors in the very
  wallpapers, sidings, tiles, carpets and paving the builders chose — the game's texture
  recipes (complates) re-rendered from each lot's designs — plus foundations (as tall as the
  lot makes them: the Goths' manor stands high on its slope), every storey where the game has it
  (houses with or without foundations, garages beside them, split-levels out over a hillside
  as Cormorant's, basements: storey 0 below the ground floor, its stairs, walls and rooms, the
  ground open over it to look in, Page Down to it; a hillside house's walk-out lower floor dug
  into the slope, as the Wolffs' garage and den, likewise; Isla Paradiso's houseboats afloat,
  their decks walked on over the water, a hull down from the deck's edge), staircases built
  from the game's
  own stair pieces and railings (as wide as the builders made them), and the house's own
  furniture; the ground never shows through a floor; nearby lots are shown in full detail with
  the game's roofs, distant ones as the game's pre-rendered imposters. Better than the game's
  engine allowed: the household's own house stays itself (walls up, roof on) out to 260 m
  rather than turning to its low imposter at 75, and the street round it (the nearest sixteen
  lots, further round as the camera draws back) with it; the imposters' ground on a sloping
  lot stays ground (whatever follows the ground picture's plan of the lot, however high: not
  shards of it over the terrain), and every cut-out (the imposters' fences and
  railings, real ones, leaves) is drawn from a copy of its texture with the colour bled under
  the cut-away part and fresh mipmaps, so far off they thin out instead of turning to thick
  black lines; leaves and imposters are cut out with alpha to coverage, soft-edged up close.
- **Sims**: Create-a-Sim from real CAS parts (browsed as the game's own style pictures), with
  human traits restricted to the original age groups in CAS, random generation, birthdays
  and Mid-Life Crisis, with conflicts taken from the original exclusion groups; changing to
  a younger age replaces traits no longer available. The game's own traits picker serves Create
  a Sim, the Mid-Life Crisis reward (every trait chosen again) and a child's or teen's birthday
  (the trait their new age brings chosen, their others kept). With
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
  needs (falling as the game's `Motives` tuning has them: hunger, bladder and energy by the band
  they're in, slowing as they run low; social and hygiene steadily; fun only down to nothing),
  moods, skills (learned in the game's skill points: each skill's points for each level from
  its `Skills` table, and each pastime's points an hour from its interaction tuning: an hour
  at the piano 420, at the easel 430, the workout bench 1014, the telescope 800, a jog 600;
  a repair or upgrade the object's own tuning; study at work the career's tone; more in a
  good mood, as the game's), careers, relationships, autonomy, social interactions; children,
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
  Things break and are repaired as the game tunes each kind (its `RepairableComponent` values
  for every class: a TV unbreakable for two days new or fixed, then 6% a use and 3% more each
  break; a repair's time from its longest at no Handiness to its shortest at the repair's
  complexity). Electronics can shock the unskilled at their own odds (a cheap stereo half the
  time, a computer hardly ever), surely one still singed or standing in a puddle, never the
  Handy, the pregnant or an Electrician; upgrades are kept in saves. Fishing trips teach Fishing
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
  original costs. High mood also earns passive lifetime happiness at the original rate,
  with fractional progress retained across saves. Its dialog shows each reward's icon,
  name and description: Steel Bladder (bladder stays full), Dirt
  Defiant and Hardly Hungry (hygiene and hunger decay at one quarter speed), Fast Learner
  (+15% learning, including gardening, repairs/upgrades, career study and rabbit-hole lessons),
  Fast Metabolism (+25% body-shape change), Professional Slacker, No Bills Ever (new household bills stop while a
  reward owner belongs to the household), Opportunistic (bigger
  opportunity rewards), Attractive, Extra Creative (paintings and books earn more), Super
  Green Thumb (1.15× plant quality at planting, retained in saves), Discount Diner,
  Complimentary Entertainment, Bookshop Bargainer (25% book discount), Haggler (15% shop
  discount, additive with Bookshop Bargainer), Speedy
  Cleaner (3× cleaning rate), Multi-Tasker (+3 work performance, 1.5× homework rate),
  High Roller (1.33333× royalties), Vacationer (20% of the normal missed-work penalty),
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
  what's owed. The paper is delivered every morning, to read or look for a job in. Bills are
  the game's 4.5% of what the home lot's things are worth, and things lose worth as the game
  has them do: 15% once the shopping's done (till then they sell back at full price), 10% of
  their price each midnight after, never more than 40%; burnt things are worth nothing. Sold,
  stored in the family inventory or billed, they go at what they're worth now.
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
  while seated meals use their half-eaten plate model at the original seven-bites-remaining threshold,
  with meals' source objects, recipes, remaining servings and positions retained in saves
  (including delivered pizza and cut birthday cakes),
  with trait-aware post-meal cleanup that preserves queued player commands. Favorite-food
  satisfaction applies to both seated and standing meals. Menus follow the original hours:
  breakfast from 1am, lunch from 11am, dinner from 5pm and weekend brunch from 9am–3pm.
  The meal goes on the game's serving platter on the nearest counter, and the cook calls the household to it (the
  game's wave over): whoever's hungry and not busy with something they were told to do comes
  for a plate (one at a time at the platter, in the order they get there: the cook, beside
  it, first). Sims grab a plate and sit down at a dining table or counter stool to eat (the
  game's sit-down and bar-stool eating animations, plate on the table, fork in hand), or eat
  standing when every seat is taken, and leave dirty dishes that neat Sims clear away. Guests
  eat but don't cook, children don't use the stove. Or phone for a pizza: within the hour the
  delivery, in the game's pizza uniform, carries the pizza box in to the kitchen counter.
  What's left of a meal goes in the fridge once it's been out a while (neat Sims see to it
  first, slobs never), to be had another time ("Eat Leftovers...", each dish in the fridge once,
  as the game lists them: a plate of it at the table);
  the fridge keeps a dozen servings, and they're kept in saves. The fridge's "Have Quick
  Meal ›" is the game's snacks (Cereal, Bread and Jam, Canned Soup, Ice Cream, Juice), taken
  out and carried to the table; a Microwave Meal (not for children) is put in the microwave
  (its door swinging open), checked on while it heats, taken out and carried to the table the
  same way. A group meal is served a plate at a time: the rest wait their turn.
- **Recipes** from the game's recipe list: the fridge (and the stove) offer the game's food
  menus, "Have Breakfast/Lunch/Dinner ›" (made for one: the cook's own plate), "Serve
  Breakfast/Lunch/Dinner ›" (a platter for everyone) and "Serve Dessert ›", each recipe with
  what its ingredients cost, cooked at the stove nearest the fridge, with the recipes the Sim
  knows for that time of day (Mac and Cheese and
  Waffles to start; Goopy Carbonara, Spaghetti, Key Lime Pie, Lobster Thermidor... as Cooking
  grows; meatless ones for vegetarians), the rest from recipe books bought at the bookstore
  at the game's prices. The food is the game's own models in their geometry states (those named
  only by their rig, Waffles, Autumn Salad, Sushi, found through it) — the dish
  on the serving platter, a plateful in front of each Sim — scraped clean once eaten. The
  Culinary Librarian lifetime wish asks for every one. Learned recipes are kept in saves.
- **Plumbob**: over the Sim's head wherever their animation has it (lying in the bath, sat
  down, a child's lower).
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
- **WooHoo**: WooHoo and Try for Baby, accepted, take the couple to the nearest free double
  bed (the game's `FindNearestBed`): into bed on either side, the game's WooHoo under the covers
  (its two parts, the bed's covers rolling with it), and out again, closer for it.
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
  Home), Sims walking round walls and fences however they lie on the grid (through doorways,
  arches and gates, under curtains, over a house's floors out over a hillside), day/night with
  lamps and street lights. Sims deciding on
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
  climb out, staying in their swimwear until there's something else to do (the hot tub's the
  same). From a diving board they walk out and dive in (the game's
  dives, cannonball included), then swim. At bedtime they change into their sleepwear (pyjamas,
  nightgowns, boxers), and into athletic wear to work out, and dress again after. They undress
  for a shower or a bath (the game's naked outfit, which has no anatomy) under its censor, a
  mosaic in their skin's colours over a man's hips or a woman's chest and hips, and dress again
  after (in pyjamas when it's late, as the game's `GettingOutOfBath`). Every change is the
  game's spin (`a_clothesChange`, a child's own), in a swirl of sparkles with its whoosh, the
  new clothes on mid-turn: by the bed, the shower or the bench before getting in or on (not on
  the way, and not to clean the shower), out of the bath once they're out, at the dresser, on
  the way out to work or into the cold (a walk waits for it, though never on the stairs or in
  a lift). Babies and toddlers are changed as they are. A Sim stopped part way up the stairs
  or in a lift by something else coming up is set down on the nearest floor.
- **Baths**: as the game's: stood in the tub, the taps turned on and the water rising (the
  tub's own water, raised by its half of the clip), sat down, arms and legs washed, then lying
  back; sat up and out at the end. The rubber ducky's played with in the water too, and a
  bubble bath is a tub of foam. A shower over a tub (the Shower of Power) is showered in,
  over its side and back with the game's shower-tub clips, or bathed in.
- **Playgrounds**: children climb a slide's ladder, sit at the top and slide down, and climb a
  jungle gym's tower, play in its lookout and come down its pole: the game's clips carrying them
  along (its `world_offset` channel, the travel the engine moves Sims by), left where they end
  up. The dollhouse is played at its open side.
- **Grime**: toilets, showers, tubs and sinks get dirtier with every use (the game's numbers:
  less after Neat Sims, more after slobs) and look it: the model's own Dirty and Very Dirty
  materials, their dirt drawn over the object's texture (a clogged toilet its Clogged look).
  Once one needs it, "Clean" is offered: the game's scrubbing, with the toilet brush or the
  sponge in hand. Neat Sims clean up on their own; others when it's filthy; slobs never. Kept
  in saves.
- **Idles**, as the game's (its `Idles` tuning, picked as its idle manager picks): Sims
  standing about with nothing to do fidget now and then, an everyday idle (stretching,
  scratching their head), their mood's (a smile and a nod; grumbling when tense), one of their
  moodlets' (yawning when tired, the bladder dance, sniffing an armpit when smelly) or one of
  their traits' (the Neurotic's, the Artistic's, the Grumpy's muttering...), weighted 1, 3, 6
  and 6 as the game weights them; half of those with a thought balloon show it.
- **Facial expressions**, the game's (its `FacialOverlays`): every few minutes a Sim's face shows
  what they feel most, over whatever the body is doing (on the face alone): a smile in a good
  mood, a frown when sad or lonely, a sneer when smelly or in a mess, a scowl when annoyed, wide
  eyes when scared, drooping lids when tired, and now and then the Evil's grin.
- **Reactions**, as the game's (its reaction broadcasters, with its `Reactions` clips), turned to
  face what they react to: near someone smelly (the same room, or in sight outdoors), a sniff
  and a grimace and the Disgusted moodlet while they're about (not the Slobs); at someone
  wetting themselves, shock, or what their traits make of it when they're not friends (the
  Mean-Spirited point and laugh, the Grumpy grumble, the Over-Emotional cry), and the one it
  happened to is Embarrassed only if someone was near; at a death, shock, then tears for someone
  close, a laugh from the Evil or an enemy, or a distressed look, and those left behind grieve
  (a partner Heartbroken, family and friends Mourning). Some moodlets play their own as they
  come: a wipe of the brow when Fatigued, tears of joy at a First Kiss, queasiness, a start when
  Scared, a jolt when Singed. As rain, snow or hail begins, most of those out in it look up at
  the sky. A reaction waits for its Sim to be standing free, then lapses.
- **Stink**: green wisps rise off a smelly Sim's armpits, as the game's `stinkAdult` does.
- **Privacy**: while a Sim's using the toilet, the shower or the bath, the room is theirs:
  others set out for anything in it wait where they are, thinking of the bathroom, until it's
  free (giving up after half an hour), and choose another bathroom of their own accord.
- **The game's own socials**, from its `SocialData` (Admire, Gossip, Brighten Day, Talk About
  Weather, Mock, Flatter, Deep Conversation, the Accuse of..., Ask About..., Complain About...
  and Joke About... families and more), each with its tone's effects, the ages and liking it
  asks for, the traits it asks of the one doing it (only the Good Brighten a Day, only a
  Schmoozer Flatters; the Grumpy aren't asked to but won't do some unasked) and of the one it's
  done to, the topics it's offered under (traits': the Evil may Mastermind a Plot, and others
  Accuse them of Being Evil; moodlets': a Sim just fired is asked about work; a pregnancy, a new
  job, a new home) and its suite's own clips; those sharing a phrase gathered on a menu of their
  own, long menus paged with "More ›". A Mooch moochs money from other households (the game's
  sums, as often doubled or quadrupled) and snacks; a cook can Ask for a Recipe of a cook from
  another household, and learns one from the books at their level. Sims chat with the friendly and funny ones of their own
  accord, as their traits favour. Children have theirs too, in the children's own clips: the
  game's child-to-child, child-to-grown-up and grown-up-to-child versions where it has them
  for the social (a hug, a gossip, cheering up), else its generic talk, with the listener
  nodding along or wincing as it goes down (the grown-up in it talking down to them).
- **Conversation topics**, as the game's (its `ActiveTopic`s, for as long as its table has them
  last): what's happened to a Sim lately is talked about, by them or by others with them,
  opening the game's socials under it. A birthday (Announce Birthday; Birthday Congratulations
  from others), a fire (Talk About Fire, for all who were there), a burglar seen, a promotion,
  an engagement, a wedding, a new outfit (Compliment or Mock Appearance), a meal cooked
  (Compliment Cooking; Thank for Cooking from those at the meal, to the cook), cleaning up
  (thanked for it), a day at work (Enthuse About Work; Worry About Work after a bad one), a
  skill come on (Boast About Culinary Prowess...), a swim, money running low, a party thrown
  (complimented or mocked while it's on, boasted or complained of after), a visit (the guests
  Compliment the Home), a novel finished (Praise Written Book), a pregnancy (Announce
  Pregnancy), dirty dishes about (complained of; a mess too, by the Neat); what a Sim's doing
  while they do it and twenty minutes after (Ask: Good Show? at the TV, Compliment Music,
  Ask About Game); grown-ups with children, the household's own, those in the same line of
  work; a Sim's Charisma (its greetings, by their level: Amusing and Flirty, Funny, Hilarious,
  Seductive, Hot); the other's line of work (Enthuse about Science, Joke About the Police,
  Complain About Politicians...). Most are said once between two Sims while they last. Sims bring them up of their own
  accord now and then, as their free will has it (thanking the cook after dinner,
  congratulating someone on their birthday): something mean only from the mean or to someone disliked, something romantic only
  where there's romance.
- **Art**: paintings and sculptures are viewed ("View", for fun): the Sim looks the piece over
  and then shows what they make of it with the game's reactions, by its price and their taste
  (Snobs and the Artistic love fine art, Snobs sniff at cheap art, those who can't stand art
  hate it all and never look of their own accord).
- **Laundry** (Generations), as the game's `LaundryManager` has it, with its clips: where the
  laundry's done, Sims taking off their day's clothes indoors leave them in a pile (or in a
  hamper in the room; slobs drop them anyway, often), two a day each at most (a slob's four).
  Piles are picked up by the Neat at once, others in time; once there's a load, someone does
  the laundry (gathering the piles first): half an hour in the washer, ten minutes in the dryer
  or half an hour on the line. For twelve hours after, whoever dresses is in Fresh Clothing
  (Very Fresh, line-dried or from fine machines); a swimsuit or the bath takes it off, smelling
  does too. Kept in saves.
- **The future's kitchen**: a house with only the Food Synthesizer (Into the Future) eats from
  it, a plate made at the touch of its screen; apartments' trash goes down the trash chute.
- **Puddles**, as the game's: a broken shower, tub, sink or toilet sprays water and spills a
  puddle every quarter hour (four at most, running to the nearest free tile), and a Sim who
  couldn't make it to the bathroom leaves one. They dry up (two hours outdoors, eight on a
  floor) or are mopped up with the game's mop clips (five minutes): Neat Sims do it on their
  own, slobs never. Kept in saves.
- **Unmade beds**: a bed slept, napped or lazed in is left unmade, its covers thrown back, until
  someone makes it ("Make Bed", from beside it, with the game's clips: the covers pulled up,
  smoothed and patted; Neat Sims do it on their own). An unmade bed is got into as it is. Kept
  in saves, as what a fire charred is.
- **Double beds** sleep two, a side each, as the game's couples do: the second Sim gets in on
  the far side, with the game's animations played mirrored (the rig's two sides are each
  other's mirror images, so each bone plays its partner's track, reflected). Sims share a bed
  on their own with their partner, fiancé(e) or spouse. The bed's half of the clip is mirrored
  with it (each of its bones played as its partner across the bed's middle is), each sleeper
  turning back the covers on their own side.
- **The workout bench** (The Exercise Queen): "Pump Iron", for teens and up, in athletic wear:
  on to the bench, then the game's bench presses, leg extensions and pec flys in turn, building
  Athletic.
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
  hail pelts; precipitation is clipped below indoor ceilings using triangle-aware shelter
  masks for the home and visited lot, including rotated buildings and raised floors.
  Fog closes in; heavy rain brings lightning, the sky flashing and the game's
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
  Each season has its holiday, as the game's: Love Day, Leisure Day, Spooky Day and Snowflake
  Day, two days before the season ends; word of it the day before, and on the day at five in
  the morning its sting, the game's own notice and its moodlet for everyone (Spring Fling,
  Summer Fun, Spooky Time, Holiday Cheer), no school, and work a paid day off (and on
  Snowflake Day, three times in four, a holiday bonus of days' pay, more for good work, in the
  game's own words); on Snowflake
  Day Sims take more warmly to every social, and on Love Day to romance. After heavy
  snow (within twelve hours, with the ground's snow at least medium) it's a snow day: the
  game's notice and sting, and no school. The season
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
- **Homework**: children and teens come home from school (Monday to Friday) with homework,
   and sit down at a table to do it (the game's homework animations and base completion rate;
   faster for Bookworms, Workaholics and Multi-Taskers, slower for Perfectionists); done, it
   lifts their grades; left undone, it drags them down. A teen or grown-up
  can Help with Homework (a Friendly social, offered while there's homework): it's done, with
  better grades, the helper learns a little Logic, and the hours count towards Teacher
   Extraordinaire (whose teachers get through it twice as fast). A grown-up can tell a child or
   teen with homework to "Do Homework" (the game's own social, which family-minded parents use
   unasked): one of the household, or who likes them well enough, sets about it there and then.
   Grades and unfinished homework
   survive saves, including partial homework progress and school visits in progress;
   cancelled homework can be resumed, with its percentage shown in the school tooltip.
   Becoming a young adult clears school
   grades and remaining homework.
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
  and relatives, out to cousins, can't romance one another. The Simology panel's Family Tree
  button puts up the game's own family tree dialog: the Sim's grandparents and parents above
  (each couple over their child), brothers and sisters beside them and their spouse at their
  side, children and grandchildren below, in the game's thumbs with their portraits and the
  game's connecting lines, the ellipsis on those with more family out of view; a click on
  anyone puts them at the middle.
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
- **Fire**: a poor cook can set the stove alight, at the game's odds (the food burnt, a third
  of the time for a novice and never for a Natural Cook, and one in twenty of those alight); the flames flicker and light
  the room, spread to what's beside them and ruin it (charred, the model's own Burnt look,
  until it's mended), and Sims nearby panic with the game's
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
  away and bring them back. A household with a car of its own takes it out on its trips and
  to work, as the game's Sims drive themselves: it leaves the driveway and drives off with
  them, and comes back when they do. Clicked, the car offers what the game's does: "Go to Work"
  when it's nearly time, and "Go to Community Lot...".
- **Collecting and fishing**: the world builders' spawners leave gems, metals and space rocks
  about the community lots (from the game's own tables, with their odds and prices); Sims
  crouch to collect them, catch the butterflies flitting about by day and the beetles in the
  grass (each kind in its colour, caught with the game's catching animations), and fish at the lots' fishing spots with the game's casting and
  reeling animations, rod in hand, catching what each spot holds by Fishing skill. Finds go in
  the finder's inventory, and the Collection Journal (J) is the game's own dialog: its
  statistics, metals, gems, insects and fish tabs, and the game's table of each kind's finds
  (picture and name, greyed until found; rarity; how many the household holds; what one
  fetches).
- **Vending machines** (Late Night): *Buy Soda* or *Buy Candy* (the healthy one has both),
  with the game's clips, the coins in and the can or the bar out of the tray, and had there
  and then (a little hunger, as the game's), §5 away from home, free at home. One in ten sticks
  in the machine (a glare at it, and nothing to pay); *Shake Machine* or *Slam Machine* may
  shake it loose, or one for free, or topple the machine onto them, at the game's odds (the
  slam riskier). Catalogue objects
  come in their first swatch, as the game sells them (some, the vending machines, have no
  picture of their own without one).
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
  picnic basket gives a picnic on the ground off paper plates; a picnic table seats four on
  its benches (the game's four places, getting in and out with its own animations), sat at or
  eaten at like a dining table; a buffet table serves a plate
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
- **Brushing teeth** at a sink (the game's brushTeeth animation, toothbrush in hand) leaves the
  game's *Minty Breath* moodlet for four hours, as its `Sink.BrushTeeth` does.
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
  the one most of the town's houses have), and roofs over the rooms in the game's shapes (hip,
  gable with its gable ends, shed, mansard; chosen on the Roofs tab) in any
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
  stations on stereos: turned on and off as the game's are (a Sim presses the button), kept
  playing until turned off (dancing to one turns it on), and "Change to..." any of the game's
  stations there and then.
- **Careers** from the game's own career tables: all eleven base-game careers (Professional
  Sports included) with every level's title, pay, hours and workdays, and the part-time jobs
  (bookstore, grocery, spa, mausoleum) for teens. Careers branch where the game's do
  (Criminal into Thief or Evil, Music into Electric Rock or Symphonic, Law Enforcement into
  Special Agent or Forensic Analyst): the promotion there asks which path to take. Jobs are
  found as the game's are: "Find a Job" in the paper (three openings a day, the game's
  tuning) or "Find Job" on the computer (five), the same all day, in the game's Available
  Jobs dialog: the career and the job, its pay, hours and workdays (today marked), what the
  listing says, the career's icon and the Sim's face, arrows through the openings, Accept
  Career or Cancel, the game standing still meanwhile.
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
  it), with submenus ("Have Breakfast...") for long lists, opened where their option was
  clicked with the game's return button in the middle, named for them, back to the menu
  before. A menu that would go off the screen is moved onto it, the pointer with it, as the
  game's are. The interaction queue at the top left and the
  notices at the top right are the game's own too: each queued action in its frame with the
  object's picture or the other Sim's face, the running one's progress bar, a cross to cancel;
  each notice a speech card with the Sim's face or a system card, closed with its cross.
- **The game's own main menu**: the game's `GameEntryMainMenu` over its picture of town (cropped
  to cover any screen shape), with the logo. With saved games: the saves along a strip in
  the game's frames (arrows and the wheel to scroll), the newest chosen; the chosen save's
  name, household, game time and date; Play Now. Each save keeps its own pictures, taken as
  it's saved (the lot as the player was looking at it, and the family's faces side by side),
  shown on the strip, beside the save and on its loading screen. New Game (or no saves yet): the town from
  the game's drop-down list of the home towns installed (the newest pack's chosen first, as
  the game does), each with its own picture and description read from its world file; Play
  Now. The round button opens the game's popup menu: Options and Quit. A newer interface
  layout than the one converted is converted again while the menu waits. Loading a town shows
  the game's loading screen: the town's own loading picture (Twinbrook's, Bridgeport's...),
  "Welcome to" the town or the saved household's name, the game's tips (its own and the
  packs') changing every ten seconds, and the green load bar; a first run's conversion is
  written under it. Options (from the menu or the puck's "…" button, which opens the game's
  popup menu: Save, Save As, Edit Town, Options, Quit to Main Menu, Save and Quit, Quit) is
  the game's Options dialog: its tabs with their icons; Sound (the voices', effects', music's
  and ambience's volumes on the game's plumbob sliders, each with its Mute box); Game Options
  (free will off, low or high; aging on or off; the lifespan, short to epic, with the Sim days
  a life lasts and each life stage's days); Graphics (lighting and shadows); OK keeps the
  changes, Cancel drops them, Restore Defaults puts a page back. The game's own dialogs ask
  its questions: an opportunity offered (the Sim's face, the opportunity and "for <Sim>", its
  description on the game's card, objective and reward, OK or cancel), a teen's lifetime
  wish (the wishes' icons along the chooser's slots, the chosen one's name and what it asks),
  and the lifetime rewards shop (the game's table of rewards with their icons and costs, those
  out of reach shaded, the points to spend, Purchase and Close).
- **The game's own Create a Sim**: the Sim stands in the game's own dressing room (its CAS room
  model: the window and its view, curtains, the dressing mirror, the plant and the rug on the
  wooden floor), framed by the game's puck (turn and zoom, the household's Sims along the
  skewer by their ages' figures, Add a Sim, the "…" menu, cancel and accept) and its character
  sheet (Basics, Hair, Face, Clothing, Personality, Randomize), each opening the game's panel:
  Basics (names typed in place, the random-name dice, gender, the supernatural type menu, age
  buttons, the skin tone slider over a ramp of the tones, weight and fitness); Hair (the
  styles in the game's item cells three across, scrolled to the one worn, with the item
  grid's scroll bar; the hair colours' swatches; facial hair for grown men); Face (head and
  ears, eyes, nose and mouth, each with the game's picture of it, the hotspots of its parts and
  their sliders; the eyes' colours; eye shadow and lipstick); Clothing (the outfits down the
  side, tops, bottoms, outfits and shoes across the top, a row for each item with the game's
  own picture of each of its colourways, turned by the row's arrows); and Personality (the
  traits in their sockets and the game's traits picker by its groups, the six lifetime wishes
  that suit them, favourite food, music and colour from the game's grids, the three voices).
  A household of more than one is accepted through the game's Sims and Relationships screen:
  the family tree, and a Sim dragged onto another asks what they are to each other.
  Add a Sim opens the game's add menu: a new Sim, or Play with Genetics (`CASGenetics`): a
  father and mother from the household, the child's gender and age, Create Baby (a skin tone
  between the parents', one's hair colour and one's eyes, a face between theirs) and Accept, the
  child joining as their son or daughter (and their other children's sibling).
- **The game's own Play Flow**: a new household chooses its home as in the game: the speech
  bubble's instructions ("Choose a place to live!", then "Let's play!" with Play, Cancel back to
  Create a Sim or to choosing again), the play flow puck (turn the town, options), the bin of the
  homes for sale with each lot's picture, and a click on one (or on its lot on the map) putting up
  the game's Purchase Lot dialog: its picture, name, size and description, furnished or
  unfurnished at their prices (the unfurnished house keeps only its doors, windows, stairs and
  fireplaces), the household's funds (the game's §20,000 and more by each Sim's age) and Purchase,
  greyed when it's out of reach; the price comes out of the funds on moving in.
- **The game's own Edit Town**: the game's Edit Town puck (Return to Game, turning the town,
  options, the select and bulldoze tools), its library bin along the bottom (the household bin, and
  the town's houses to put a copy of on an empty lot, in the game's bin cells), lots picked on the
  map putting up the game's info panel (the family portrait, name, funds and story; the lot's
  picture, name, size and description; Evict, Switch To, Change Lot Type, or Place for what's in
  the bin), and the game's own dialogs to confirm: Confirm Household Eviction, "Are you sure you
  want to change your active household?", the bulldoze warning, and Change Lot Type's residential
  or community choice with its kinds of community lot.
- **Family inventory**: in Buy mode an object picked up off the lot goes into the household's
  family inventory by the game's Family Inventory button (or a click on its grid); the button
  shows the inventory in place of the catalogue, in the game's cells, each object taken out to
  put down free (back in if it isn't), with Sell All. It's kept with the game and goes along when
  the household moves or another is played, and Edit Town's Move Out With Home packs a household's
  furniture into it.
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
  (right-click an offered or promised wish to dismiss it; both lists, their rewards and
  the next-offer timer survive saves)
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
  the cell phone opens the phone, the green book the collection journal); Relationships (a
  card for everyone the Sim knows: their portrait, the relationship bar out from the middle,
  the state's icon from stranger to spouse, a relative's, co-worker's or visitor's mark; the
  last one talked to first; all, family, friends, co-workers or visitors; sideways with the
  wheel, a click takes the camera to them); Opportunities (the career, skill and special
  slots with their icons or the game's empty-slot pictures and words, the chosen one's task,
  time left and description; a right click cancels one); Lifetime Rewards (the happiness to
  spend, the rewards bought with their icons and meanings, and the button to buy more).
  Buttons light up, press and grey out with the game's own pictures. The Sim's own menu has their phone too.
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
  wallpaper and the other wall/floor groups), with All restoring the complete list. The
  diagram's doors, windows, arches, columns and fireplaces open the game's Construction
  catalogue, and its trees, shrubs, flowers and rocks the Gardening catalogue (their tabs and
  expanding grids, sorted by the objects' own build categories); trees, shrubs and flowers are
  planted from their species' billboards, bought, moved and sold like any object. The
  diagram's gate opens the gates alone (single and double), each set into a fence as a door
  into a wall, in the fence runs it replaces (on the ground or a deck), Sims walking through it.
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
  master, music, effects, voice and ambient levels, aging and life span (Short, Medium,
  Normal, Long and Epic, using the original stage-length scaling), free
  will, and the original per-preset elder mortality rates (reduced for Marathon Runners,
  with survival progress retained across saves). Other controls include
  shadows and the frame-rate counter, kept in `settings.json`.

## Parity status

Full parity is still a work in progress. Remaining milestones include the original-layout
complete Build Mode interface; Create a Sim's pets; complete object placement and routing compatibility; the rest of the expansions'
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
`TOPICS=Birthday,Talk About Fire` gives every Sim those conversation topics (`Thank For
Cooking` about the selected Sim); `WEATHER=rain@30` has the rain begin half an hour in;
`QUEUE_LOG=<s>` also names what keeps an idle Sim's free will back.

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
`HALF_MEAL_TEST=1` creates a platter and individual plate using an original recipe, reduces
the platter to four servings and advances eating past the half-eaten threshold, then verifies
both full meshes are replaced by half-depleted meshes exactly once. Capture after
12 seconds; the log reports `half platter PASS`.
Run that probe with `--save-at 9` and an isolated `SIMS3_SAVES` directory, then use
`HALF_MEAL_TEST=restore --load 0` (the variable set in the environment) to check restored
servings and half-depleted geometry, plus a pizza's source model and remaining slices;
the log reports `saved platter PASS`.
`MEAL_BUDGET_TEST=1` checks cooking and birthday-cake requests with an empty household
budget: neither may create food or debt. Capture after 12 seconds for `meal budget PASS`.
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
