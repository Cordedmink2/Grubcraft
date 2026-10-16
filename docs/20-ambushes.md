# 20 · Ambushes

Hiding regiments in woods and tall grass, spotting them, and the surprise when a hidden force strikes. Mario asked for ambush mechanics to be explored in full and then built. This document is the whole design: the battle rules (built first), how they stay readable and fair, the counterplay, what each faction and the AI do with them, how they grow into fog of war, the campaign and the living world, and the build plan.

## Summary

- **Woods and tall grass hide regiments.** A regiment with most of its block in a wood is hidden from the enemy until an enemy regiment comes within 4 tiles (10 if it is on the move). Tall grass hides a regiment standing still, spotted within 3 tiles. Everywhere else, regiments are seen, as today.
- **Fighting or shooting gives you away.** A regiment in melee is seen by everyone; one that looses a volley stays seen for 5 seconds. Once seen, a regiment stays seen for 3 seconds after it slips back out of sight. Allies share what they see.
- **You can't target what you can't see.** Attack orders need a target your side can see; archers only shoot what their side sees; a regiment chasing an enemy that vanishes marches to where it was last seen. Hidden archers hold their fire until you order them to shoot, so they don't give themselves away.
- **Springing an ambush.** A regiment that hits an enemy in melee whose side hadn't seen it until less than 4 seconds before springs an ambush: the victim loses 20 morale at once and is *caught off guard* for 6 seconds (its blows deal 70% damage), and its friends within 16 tiles lose 5 morale. An ordered volley from hiding costs its target 10 morale. Each hidden regiment springs one ambush.
- **Hidden regiments don't hold the field.** A Field Battle is won by holding the field with no enemy in sight still standing, so an army whose visible regiments have all broken has lost, however many it still has hiding. An army with no enemy left standing at all holds it wherever it stands.
- **The AI plays fair and sets ambushes.** Computer players see only what their side sees. The Veteran and General can choose the **Ambush** doctrine when a wood lies off a flank between the armies: the line holds while up to three regiments (its fast troops, or the ends of its line in an army without any) slip into the trees, wait, and strike the enemy's flank once it is locked in melee or walks past.
- **On screen:** woods and tall grass are drawn on the generated maps, enemies you can't see aren't drawn, your hidden regiments carry a moss-green flag above the treetops, and an **Ambush!** alert, drum and horn sound when one is sprung.
- **Woods slow and shield** (with the terrain rules in [05 · Combat](05-combat.md#terrain)): regiments in a wood march at 85% pace (cavalry and monsters 70%), and the trees catch 30% of the arrows shot into or out of it.
- **Later:** unit traits (scouts, stealthy, tall, detectors), fog of war with line of sight and dead ground behind ridges (M3), Brood burrowing, and ambush stances and ambush battles on the campaign.

## What makes an ambush

Every ambush in history and in games has the same five parts. Each needs a rule in Grubcraft:

| Part | Question | Grubcraft's answer |
|---|---|---|
| **Somewhere to hide** | Where can a regiment be and not be seen? | Cover on the map: woods and tall grass now; later dead ground behind ridges, fog banks and tunnels |
| **A way to be found** | When does the enemy see it? | Spotting distances by cover and movement; fighting and shooting reveal; allies share sight |
| **A reason to walk into it** | Why would the enemy come close? | The ground: woods beside fords, passes and the open fields where lines meet; bait, a feint, a line that pins them |
| **A payoff** | What does springing it win? | Morale: a big shock to the victim and its friends, and a few seconds caught off guard, on top of the flank or rear hit an ambush usually lands |
| **Counterplay** | What can the victim do about it? | Look before you walk: scouts, keeping distance from woods, a slow advance, spare regiments to turn and meet it; later detectors and reveal abilities |

Three famous ambushes show the shape, and one of them is close to the Ember Clans' own world:

- **Lake Trasimene (217 BC):** Hannibal hid his army in the hills above a lakeside road in the morning mist and fell on the Roman column as it marched past. Cover plus a road the enemy had to take.
- **The Teutoburg Forest (9 AD):** Germanic tribes destroyed three Roman legions strung out in column through dense forest. A marching column can't form a line in the trees.
- **Okehazama (1560):** Oda Nobunaga's small army, hidden by a thunderstorm, struck Imagawa Yoshimoto's resting camp in a narrow valley and broke an army many times its size. Surprise turned numbers upside down because the victims broke before they could fight.

And the games Grubcraft learns from:

- **Total War (Shogun 2 onward):** regiments standing in woods or tall grass are hidden until enemies come close; on the campaign map an army in Ambush stance can trap a marching army into an ambush battle where the defender starts in column. Grubcraft takes the battle rule now and the campaign stance later.
- **Mount & Blade:** no hiding in battle, but bandits and lords lurk on the world map. Useful for the living world, not for battles.
- **Warcraft III and StarCraft:** fog of war, high ground and burrowing make information a weapon. Grubcraft's Brood Burrowers and Drone-Moths come from here.

## The battle rules

These are the rules in the simulation. They are the same for people and computer players, and every number is in `crates/gc_sim/src/vision.rs`.

### Cover

The map carries one cover value per tile:

| Cover | Hides | Spotted within (edge to edge) |
|---|---|---|
| **Open** | Nothing: seen by everyone | Always |
| **Tall grass** | A regiment standing still | 3 tiles |
| **Wood** | Any regiment, standing or marching | 4 tiles standing, 10 on the move |

A regiment counts as in cover when at least four of five points of its block (its centre and its four corners) are in that cover. So a long thin line sticking out of a wood is seen, and a deep block fits into a small copse better than a wide one. "Edge to edge" means the gap between the two blocks, so a big regiment isn't seen sooner because it is big.

Woods hide marching regiments (at a shorter distance) because trees block the view; grass only hides those who keep still and low. Fast troops are better ambushers: a regiment charging out of a wood is spotted at 10 tiles, and covering that in under 4 seconds keeps the surprise (see [Springing an ambush](#springing-an-ambush)), which cavalry and Skitterlings do and line infantry only just does from closer.

### Seen and hidden

Every tick, for each regiment and each other party, the simulation decides whether that party sees it:

- **Its own side always sees it**, and allies share sight.
- **In the open, it is seen** by everyone.
- **In melee, it is seen** by everyone: fighting is loud.
- **After loosing a volley, it is seen** by everyone for 5 seconds: a cloud of arrows rising out of a wood gives the archers away.
- **In cover, it is seen** by a party when any of that party's regiments (or its allies') is within the spotting distance above.
- **Once seen, it stays seen for 3 seconds** after the last reason ends. Watchers see it slip into the trees and fade, rather than flicker at the edge of a distance.
- **Regiments placed in cover start hidden** (a deployment in a wood is an ambush from the first second); regiments placed in the open start seen.

Any regiment can spot, including routers: running for your life doesn't make you blind.

### What hiding changes

- **Attack orders need a target your side can see.** You can't click what isn't drawn, and the simulation rejects an order naming a hidden regiment, so a computer player can't cheat by guessing.
- **Archers only shoot at what their side can see.**
- **A chase ends at the last place the target was seen.** A regiment ordered to attack an enemy that slips out of sight marches to where it was last seen and stops there.
- **Hidden archers hold their fire** at enemies who can't see them, until you order them to shoot (or the enemy spots them). Without this rule archers in a wood would loose at the first enemy in range and give the ambush away. Melee regiments never charge on their own anyway; you order the charge.
- **Champions come out only between regiments that see each other** ([05 · Champions before battle](05-combat.md#champions-before-battle)), and a regiment hiding in cover never sends one out, since he would give it away.
- **Unseen enemies don't frighten anyone.** A regiment in melee loses morale when nearby enemies outnumber nearby friends; enemies hidden from its side don't count until they show themselves. A routing regiment runs from the enemies its side can see, and rallies once none has been near for a while, so its flight doesn't give away a hidden regiment either.
- **Hidden regiments don't hold the field.** A Field Battle ends when one side has held the field for 10 seconds: it has regiments standing in sight, and no enemy it can see is still standing. Regiments hidden in cover don't count for either side, so an army whose regiments in sight have all broken has lost, however many it still has hiding (spring the ambush while there is a battle to spring it on), and a beaten army's survivors rallying in a wood don't drag the battle on. Two armies both wholly out of each other's sight are still facing each other. With no enemy left standing anywhere, the winner holds the field even if its whole army stands in a wood. (Since review fix R5 the simulation itself rules on this every tick, `gc_sim::verdict`, so the game, the golden replays, the AI test harnesses and every machine in a network match agree on who won; the ruling is part of the state hash.)

### Springing an ambush

A regiment **springs an ambush** when it makes contact in melee with an enemy whose side first saw it less than 4 seconds before (or never saw it at all). Then:

| Effect | Number | Why |
|---|---|---|
| The victim loses morale at once | 20 points | Morale is how battles are won here; a fresh line regiment has 45 to 70 points, so an ambush alone doesn't break it, but an ambush into the flank of a regiment already fighting usually does |
| The victim is **caught off guard** | 6 seconds, blows deal 70% damage | Soldiers facing the wrong way, half of them turning: the ambusher wins the first exchanges |
| Every friend of the victim within 16 tiles loses morale | 5 points | "Ambush!" runs down the line, like a rout's shock (8 points) but smaller |

The ambusher's own charge bonus applies as usual when it hits at speed, and most ambushes land on a flank or the rear, where blows already deal 140% or 180% and shake morale every second. Several ambushers striking the same regiment each count, but **each hidden regiment springs one ambush**: once it has, the victim's side has it fully in view, so breaking off and charging back in (or loosing a second volley) is no surprise.

**A volley from hiding**: an archer regiment ordered to shoot, whose target's side hadn't seen it for 4 seconds, costs the regiment it looses at 10 morale (the one it actually shoots, if it switched to another in range). Arrows don't catch anyone off guard.

The 4-second window decides how close an ambusher must hide: line infantry at 3 tiles a second must start within about 12 tiles of the victim (and from 10 tiles a marching regiment in a wood is spotted, so the window is tight); cavalry and Skitterlings at 4 to 4.5 tiles a second can strike from further back.

### Reading it on screen

The [UI principles](08-ui-ux.md#ui-principles) say nothing important is hidden. Ambushes hide *choices*, never *rules*:

- **The map shows where anything could hide.** Woods are drawn as trees and tall grass as tufts, so every player knows which ground could hold an enemy, the same way they know which ground is high.
- **Enemies you can't see aren't drawn**, and enemies your side spots appear where they stand. (The sound of their fighting isn't played either: horns, charges, clashes and arrow twangs from regiments you can't see are silent. Arrows landing are still heard, as they land where you can see.)
- **Your own hidden regiments carry a moss-green flag** above the treetops (and above the block in the Commander View), so you always know whether the enemy can see them.
- **"Ambush!"**: when an ambush is sprung, by you or on you, a message names who caught whom, and a war drum and a horn sound.
- **Orders follow sight**: right-clicking where a hidden enemy stands is a move, not an attack.
- **The sandbox** follows the side you command, so `Tab` shows each side's view; `V` reveals every regiment for testing (not in Field Battles or network matches, and starting one switches it off).

Planned with the M3 interface: a tooltip on any regiment showing whether it is hidden and the distance it would be spotted at; a faint "last seen" ghost where an enemy vanished; the hidden marker on the army tray.

## Counterplay

An ambush must be answerable or it is just a trap. What the victim can do:

| Answer | How it works now | Later |
|---|---|---|
| **Look first** | Send a fast regiment along the edge of a wood: within 4 tiles anything in it is revealed for everyone on your side | Scout units see further and spot hidden regiments at double distance |
| **Keep your distance** | March more than 10 tiles from woods and grass and nothing in them can surprise you from outside the 4-second window | The AI's Marshal avoids routes beside woods when it believes the enemy has reserves it hasn't seen |
| **Advance slowly** | A slow advance gives ambushers time to be seen before contact; a line that is already facing the wood takes the hit on its front | |
| **Keep a reserve** | A reserve behind the line can turn and catch an ambusher in its own flank once it is committed and seen | |
| **Make it come out** | Hidden archers can't shoot without revealing themselves; a hidden melee regiment has to charge to do anything | Burning a wood with fire pots or rockets (Ember Clans) flushes it |
| **Detectors and reveal** | | Drone-Moths' *Antennae* and the Onmyōji's *Divination* reveal hidden and burrowed regiments in a radius ([17](17-unit-rosters.md)) |
| **High ground** | | With line of sight (M3), high ground sees over grass and into the edge of woods from further away |

## Factions and units

Every faction can hide in woods. Unit traits that change the numbers come with the unit schema that carries them (the `flags` and `vision` fields proposed in [17 · Unit Rosters](17-unit-rosters.md#schema-proposals)):

| Trait | Effect | Who |
|---|---|---|
| **Scout** | Spots hidden regiments at double distance | Ember Clans War Kite (*High Watch*), Choir Glass Striders, Concord outriders |
| **Stealthy** | Spotted at half distance; hides in grass while walking | Brood Skitterlings and Burrowers, Grubfolk hunters' companies |
| **Tall** | Can't hide in grass; spotted at 1.5× distance in woods | Cavalry, monsters (Broodmother Bulk, Colossus Maw), siege engines |
| **Detector** | Reveals hidden and burrowed regiments within a radius | Brood Drone-Moths (*Antennae*, 10 tiles) |
| **Burrows** | Hidden anywhere while tunnelling; surfacing next to an enemy springs an ambush, and its blows count as rear hits for 3 s (*Ambush* passive) | Brood Burrowers, the Devourer's *Burrow Charge*, Gutworm creeps |

Faction leanings, so ambushes feel different by army:

- **Brood** is the ambush faction (its AI doctrine leans toward Ambush, [18](18-ai.md#captain-battle-tactics)): fast swarms that strike from woods inside the 4-second window, Burrowers that hide anywhere, Drone-Moths that find the other side's ambushes.
- **Ember Clans** hold the line well, so they bait: a steady line pins the enemy while Yari or mounted samurai wait in a wood; fire pots and rockets can burn an ambush out.
- **Choir** has few, precious regiments and Phase Shift to put them anywhere a Resonator stands; a Resonator inside a wood is a hidden door.
- **Carrion Concord** undead never rout, which makes them the least afraid of ambushes (they still take the damage); their living crossbows are the most afraid.

## The AI

Computer players follow the same rules ([18 · AI](18-ai.md#spymaster-perception-and-belief)): **perception is filtered**, so a mind sees only the regiments its side can see, and it can't name a hidden regiment in an order because the simulation would refuse it.

### Setting an ambush

The **Ambush** doctrine: *a hidden force strikes the flank once the lines are locked.* It is Hammer and anvil with the hammers hidden. An army with no fast troops (the Ember Clans' spears and bows, the Choir, the Concord as the rosters stand) hides the outer regiments of its line instead: a third of its line regiments, at most three, when it has at least four. Infantry has to hide closer to strike inside the surprise window, but the hiding places are close to the line anyway.

1. **Is there somewhere to hide?** While the armies are still at least 35 tiles apart and nobody has come to blows, the Captain looks for a hiding place for each of its hammers (up to three): a spot where the whole block (all five points) fits in a wood, or in tall grass, off one end of the line, between the line and the enemy and no more than halfway to it, at least 20 tiles from any enemy it can see, and reachable at least 1.5 times sooner than any enemy could get there. It tries places 2 tiles apart and prefers woods, a short march, and a spot about 8 tiles past the end of the line and 10 in front of it: beside where the enemy will be when it meets the line.
2. **Choosing it.** The doctrine scores as Hammer and anvil does (or, with no fast troops, 0.4 plus 0.3 for cunning), plus a bonus when there is somewhere to hide: more for cunning and patient personalities, less for headlong ones, a little more for a wood than for grass and when expecting to lose; then each personality's liking for it (`ambush` in `data/ai/personalities.ron`: the Deep Hive and the Dissonance like it most, the Long Ledger least). The Veteran and General can choose it; the Recruit can't.
3. **Slipping in.** The ambushers march into the trees while the enemy is far away, then stand still, facing where the enemy will come from. The rest of the army holds: while anyone lies in wait it has 25 seconds more patience in a stand-off, doesn't march out to meet an enemy coming on, and doesn't advance just to bring its archers into range.
4. **Springing it.** An ambusher strikes when an enemy regiment is locked in melee within 35 tiles of it, when an enemy comes within 12 tiles of it (edge to edge), or once it has been spotted with an enemy within 20. It picks its target the way a hammer does once the enemy line is locked: engaged enemies, exposed flanks and wavering regiments first, and swings round to the flank if it isn't already there.
5. **After it.** A sprung ambusher is a hammer for the rest of the battle, and the doctrine stays Ambush while any of the ambushers is still in the fight. If the battle moves away (the line and every enemy in sight more than 45 tiles from the hiding place), it gives up and rejoins as a hammer.
6. **Once a battle.** Nobody slips away into hiding once anyone has come to blows.

### Walking into one

Now: the AI reacts to what appears. A regiment ambushed on the flank fights back where it stands; wings turn to face flankers; the reserve plugs the gap; wavering regiments are pulled back. It also **remembers** an enemy regiment it has lost sight of for 60 seconds, where it last saw it, and won't decide the enemy is beaten while remembered regiments may still be standing. Without that memory a general who saw two enemy regiments slip into a wood thought the enemy had shrunk, decided it was "finishing them off" and threw everything forward; in the balance table that alone swung the Brood against the Choir by 15 points. (Counting remembered regiments in the battle estimate too was tried and left out: it made the Brood and the Choir too cautious against the Ember Clans' archers, who then won 70% of their battles.) (A regiment hidden from the very start was never seen, so it isn't remembered either.)

Later (with the Spymaster's belief, AI-2 and AI-3):
- It remembers every enemy regiment it has seen for the whole battle, guesses where it went, and notices when part of the enemy army has gone missing, which is what an ambush looks like from outside.
- With missing enemies and woods near its route, cautious personalities probe the woods with a fast regiment, keep their distance, or advance more slowly; reckless ones march on.
- The Warlord's rehearsal (built, AI-2) can only rehearse what it believes: its scratch worlds hold only the regiments it sees, and every commander in them sees only what its side would, so a hidden reserve or an ambush still surprises it ([18](18-ai.md#rehearsal-thinking-ahead-with-the-real-simulation)). Where there is cover it rehearses laying an ambush of its own too.

## Fog of war and line of sight (M3)

Woods and grass are the first step of fog of war. When the vision grid of [10 · Architecture](10-architecture.md#fog-of-war) arrives with Conquest:

- **Sight ranges.** Each regiment sees out to its unit's `vision` (12 tiles for most, more for scouts and air). In Conquest anything outside every friendly regiment's sight is unseen, not only things in cover. In Field Battle the default stays "light fog": regiments in the open are seen, as now.
- **Line of sight and dead ground.** Sight is blocked by ridges and rises, so a regiment behind a rib ridge or on a reverse slope is unseen until an enemy crests it: the oldest ambush in the book, and Grubcraft's maps are full of ribs.
- **Cover stays a rule on top.** A regiment in sight but in cover is still hidden until spotted at the distances above; sight just decides who is close enough to look.
- **Gut Fog** (map setting, [04](04-economy.md)) shrinks sight and spotting in lowlands while it lies.
- **Explored ground** is remembered; enemies are not.

## Ambushes on the campaign and in the living world

On the [campaign](19-campaign.md), ambushes are a host's choice of where to wait:

- **A posture, Lie in wait,** joins Attack, Cautious and Parley. A host lying in wait in a region with woods or broken ground stays hidden on the campaign map (unless a Pathfinder, an allied host or a Friendly village is watching that region) and attacks any hostile host that enters.
- **The ambush battle.** When it springs, the battle map is made from the region with the road through it; the defender deploys in **march column** along the road (no redeployment), and the ambusher deploys hidden in the cover beside it. The battle rules above do the rest: the defender's first job is to turn its column into a line before the hidden regiments hit it.
- **The chance is shown.** Whether the ambush is spotted before it springs is a number both sides see when it matters, from the region's cover, the hunter's scouts and Pathfinder, and the size of the waiting host (bigger hosts hide worse). No hidden dice, as the campaign's [design rules](19-campaign.md#design-rules) require.
- **Encounter cards** can lead to small ambush battles: bandits on the Gut road, a lost caravan that is bait, the Unbannered ronin who sell their services as ambushers.
- **Titan-spawn** that ambush are creeps with the Burrows trait (Gutworms in seeps and bogs, [07](07-world-and-maps.md)), waking when an enemy walks over them.
- **The ledger remembers.** Ambushing a party you were at peace with (a broken ceasefire) is a betrayal memory in the [relations ledger](18-ai.md#the-ledger), and word spreads.

## Architecture

### Simulation (`gc_sim`)

- **The cover layer** is part of the map: `Ground::cover`, one byte per tile (0 open, 1 tall grass, 2 wood), read through `Terrain::cover`. Empty means a map with no cover, and then vision does nothing and costs nothing: every regiment is seen, nothing is ever a surprise, and the state hash is exactly what it was before cover existed (all the golden hashes stand).
- **`vision.rs`** keeps, per regiment slot and per party (eight parties, the same as the combat grid's owner slots): how many ticks the party has had it in view (counting up to the 4-second surprise window and stopping there), how long a lost sighting still lingers, how long a volley keeps it revealed, how long it is caught off guard, and whether it was in melee last tick. A regiment is hidden or seen from the moment it is placed (in the open, everyone sees it; in cover, only its own side), and a new regiment in a slot is noticed by its generation, so `Regiments` doesn't change.
- **Order within a tick:** commands (an Attack must name a target the issuer's side sees), contacts, movement, melee and volleys (archers pick seen targets; hidden archers hold fire; caught-off-guard blows deal 70%; combat lists each volley loosed, shooter and target), morale (only seen enemies count toward being outnumbered, or toward a router's flight and rally), then **vision**: timers count down; the volleys loosed and the new melee contacts are checked for surprise against what each side saw before this tick, and the shocks applied; then each regiment's cover and movement give its spotting distance, those in cover look for spotters with the combat grid's neighbour search, and sightings and lingering are updated; chases after targets that went out of sight become marches to the last place seen.
- **Cost:** regiments in the open cost a few comparisons; only regiments in cover search for spotters. No allocation in the tick (the no-allocation test runs a map with cover).
- **Determinism:** integers only, index order, all of it hashed while the map has cover.
- **API:** `World::sees(party, regiment)`, `World::hidden(regiment)` (no enemy sees it), `World::caught_off_guard(regiment)`, and `World::ambushes()`, the ambushes sprung this tick (ambusher, victim), for the game's alert.

### The game

- **Map generation** (`terrain_gen.rs`, `Heightmap::cover`) lays woods and tall grass from the integer heights and the seed with integer rules (hashed value noise), so every machine that agrees on the heights agrees on the cover, and the LAN lobby's map check hashes the cover too. Woods grow in clumps on dry, gentle ground, at least 34 tiles from the start plateaus' centres and 10 from the map's edge; tall grass rings them and fills a few meadows. On the 256-tile maps about one tile in nine is wood and one in twelve tall grass (8 to 13% wood and 6 to 10% grass over the seeds the test checks).
- **Drawing** (`scenery.rs`): a tree on about half the wood tiles (a trunk and two stacked canopy boxes, placed, sized and shaded by a hash of the tile) and three low tufts on each tall-grass tile, built once when the map loads in 16-tile chunks and drawn nearest the camera first, at most 24,000 boxes a frame. The Commander View leaves the grass out.
- **The viewer** is the side you command: unseen enemies aren't drawn or picked, your hidden regiments (and your allies') get the flag, and `World::ambushes()` drives the alert.
- **Holding the field** (`gc_sim::verdict`) counts only regiments the other side can see, while any enemy still stands.

### The AI (`gc_ai`)

- `Perception::capture_for(world, party)` copies only what the party sees, marks the party's own hidden regiments (`Seen::hidden`), and remembers the enemies it lost sight of in the last 60 seconds as last seen (`Perception::remembered()`); each `Mind` keeps its own such look on maps with cover (the shared `view` stays whole).
- `gc_ai::Terrain::with_cover` carries the cover layer; `captain/ambush.rs` finds hiding places and decides when to spring; the Captain's `Ambush` doctrine and `Ambusher` role use them. The AI lens draws ambushers in moss green, and marks only regiments the player can see, so it gives no hidden regiment away.

### Networking

No new commands and no new wire format. Cover is computed from the heights and seed the lobby already compares, the lobby's map hash covers it as well, and everything that changes the battle is in the hashed simulation state. Replays (format 3) store the cover layer; format 2 replays still load, as maps without cover.

## Build plan

| Step | What | Who | Status |
|---|---|---|---|
| **A · Hiding and surprise** | Cover layer; seen and hidden, spotting, lingering, shared sight; targeting and hold-fire rules; springing an ambush; woods and grass on the generated maps; trees, tufts, hidden marker and the Ambush! alert; AI fair sight and the Ambush doctrine; tests | This thread, Opus (sim rules are Opus work by the [staffing plan](15-staffing-plan.md)) | Built (see below) |
| **B · Traits and terrain** | Unit schema with scout, stealthy, tall and detector flags and a `vision` stat; forest slow-down and arrows into trees (with the terrain rules); last-seen ghosts and tooltips | Opus for the schema and sim (forest slow-down and arrows into trees: the Terrain effects thread, which owns the combat and terrain rules); the unit data thread for numbers; Sonnet for the UI | Forest slow-down and arrows into trees built (2026-10-16); the rest next |
| **C · Fog of war** | Sight ranges, line of sight and dead ground, the explored layer, minimap fog, Gut Fog; Brood burrowing and reveal abilities | M3 economy and sim core thread (Opus), Conquest UI (Sonnet) | With M3 |
| **D · Campaign ambushes** | Lie in wait posture, ambush battles with column deployment, the shown spotting chance, ambush encounter cards | Campaign track C-2 and C-3 | With the campaign |

**Model strength.** Step A is mostly deterministic simulation rules and AI tactics, the two places the staffing plan reserves for Opus, so it is built by Opus in one thread (rendering and UI parts are small enough not to need a separate Sonnet thread). Step B's data numbers and step C's interface can go to Sonnet, with Haiku for test drills, as the staffing plan describes.

### Built so far

Step A is built:

- **Simulation:** `Ground::cover` and `Terrain::cover`; `vision.rs` with every rule in [The battle rules](#the-battle-rules); attack orders, archers' targets, chases and morale's outnumbering follow sight; one ambush per hidden regiment. Tests: `gc_sim/tests/ambush.rs` (19 cases, from "a regiment in a wood is hidden until an enemy comes close" to "an ambush breaks a line that would have held"); the no-allocation test ticks a map with cover. On a map without cover nothing changes: every golden hash and golden replay stands unchanged. Cost: the 6,000-soldier grand battle replayed over a dense woods-and-grass layer ticks in 88 µs at the median and 410 µs at the 99th percentile (58 and 352 µs without cover; the budget is 2,000), still with no allocation in the tick.
- **The game:** woods and tall grass on every generated map; trees and tufts; unseen enemies not drawn or picked; the moss-green flag; the Ambush! alert with drum and horn; `V` in the sandbox; hidden regiments don't hold the field. Replays store the cover (format 3) and the LAN map check hashes it.
- **The AI:** fair sight on maps with cover, a 60-second memory of enemies out of sight, and the Ambush doctrine for the Veteran and General, with fast troops or, in armies without any, the ends of the line. Tests: `gc_ai/tests/ambush.rs` (a cunning General hides its cavalry in a wood off its flank and springs it on an enemy that comes on; an army of spears and bows hides the ends of its line; minds never order attacks on what they can't see and remember what they lost sight of; no ambushes without cover).
- **A golden replay with woods:** `replays/field-woods-1v1.gcr`, Brood against Choir with both Generals free to lay ambushes (six sprung), so CI checks the vision rules play out identically on Mac, Windows and Linux.
- **How often the AI ambushes:** in 144 Veteran battles (every pair of factions, 24 each) the generals laid 208 ambushes, sending 502 regiments into hiding; 157 of those sprang, and 322 were called off when the battle moved away from them (usually because their own army advanced after waiting out its patience), which leaves room for a smarter plan later. Before ambushers could be line infantry, no live army could set one: only the Brood has fast troops, and an army of nothing but fast troops never chose Hammer and anvil.
- **Balance with woods:** two Veteran AIs a side, 192 battles a pair (seeds 2001 to 2192, after the weapons and formations of PR #35), the first faction's wins, with the same seeds without woods in brackets: Clans v Brood 60% (59), Clans v Choir 62% (42), Clans v Concord 52% (53), Brood v Choir 57% (45), Brood v Concord 52% (54), Choir v Concord 47% (56). Without woods every pair is within 42% to 59%; with them two are outside (Clans v Brood just, Clans v Choir by three points). **Woods cost the Choir 9 to 20 points in each of its pairs.** It isn't the AI's fair sight (with every general seeing everything the Choir does no better: 70%, 52%, 52%), and only partly the Ambush doctrine (without it: 55%, 54%, 45%); without line-infantry ambushers the table moves elsewhere instead (Clans v Brood 63%, Brood v Choir 40%). Since then pushing through friends and lines that push have moved the table much further than the woods did; the [Field Battle balance](17-unit-rosters.md#field-battle-balance) passes that followed (Bone Pikes defence 52, Skitterling morale 63, Shardling morale 57) bring every pair to 49% to 54% on maps with woods. Woods now help the Clans against the Choir: on bare maps the Choir wins 78% of that pair, with woods 49%. The AI's counterplay (probing woods, keeping its distance) is still to come.

Built since (2026-10-16): forest slow-down and arrows into trees. Not yet: unit traits, last-seen ghosts and tooltips (step B); fog of war (step C); campaign ambushes (step D).

## Open questions

- **A lobby switch** for hidden regiments in Field Battle ("Cover: on / off")? Recommended on by default with no switch until someone asks: it is a terrain rule like high ground.
- **Should ambush shock scale with the ambusher's size?** A tiny regiment springing an ambush on a big one costs it the same 20 points. Recommended to keep it flat until the arena says otherwise: simple and readable.
- **Should spotting need the spotter to face the wood?** Real watchers look forward. Recommended no for now; facing already matters a lot in combat, and a sentry rule is hard to read.
