# 26 · Fortifications and Bases

Walls, gates, towers, field works, sieges you can see, and bases that grow into towns, for all four factions. This is the plan Mario approved on 2026-10-22. **Nothing in this document is built yet.** The work is on hold while the project is paused and starts with stage 1 when Mario resumes it. The concept page with sketches is [Grubcraft Fortifications](https://claude.ai/artifact/Vk448ydPoRbbMx3bykGhFa), and its sources are in the project files under `design/fortifications/`.

[21 · Buildings](21-buildings.md) holds the building stats this plan builds on, and [05 · Combat](05-combat.md#structures-in-combat) holds the damage rules. Numbers here are first-pass and move in balance passes, like everywhere else in these docs.

## The aim

Bases should feel like places worth holding. You draw a wall the way you draw an army's line, and it snaps to towers and cliffs. Gates let your own troops through and keep the enemy out. Archers stand on the walls, and towers shoot. When a building falls you watch it come down. Every faction gets all of this in its own way, with one rule each that makes its walls different. Mario's brief: succinct, not overwhelming, but it must work, and it must be easy to build big fortifications.

## Where the game stands (2026-10-22, main at `6a476a5`)

- **Walls:** only the Ember Clans have walls: the Palisade (1×2) and the Gate (1×3), laid one click at a time. Concord, Choir and Brood have no wall buildings.
- **Gates:** a gate is a plain blocking box. `Nav::set_built` blocks a footprint for every side, and the walking grid has no idea of a side, so a gate shuts out its owner's troops too.
- **Towers:** the Watchtower, Tollgate, Bell Tower and Spine Mound carry a `TowerAttack` (range, damage, reload, shots) in `gc_sim::structures`, but no code fires it. Towers do not shoot.
- **Placing:** no rotation (a footprint is anchored at its north-west tile and has no facing), no dragging a line of wall, no snapping.
- **Destruction:** a building brought to nothing is removed at once and its ground freed. `Event::Fallen` is emitted but nothing reads it. There is no damage look, collapse or rubble, and `gc_render` has no particle system (no smoke, dust or fire).
- **Attacking buildings:** regiments ordered to attack a building close in and deal damage (`Command::AttackStructure`), but they are drawn as a plain block. No clip plays and nothing shows on the building.
- **Ground:** heights are fixed after the map loads. Building footprints are the only thing that changes the walking grid in a match (`nav_reblock` updates it in place, at two cells a tile).

## Drawn walls

- **Click and drag** to draw a wall at any angle, and click again to bend it. The length and price show as you draw.
- **Snapping:** a wall end snaps to wall ends, towers, gates and cliff edges within a tile and a half. Shift locks the angle to 15° steps. Otherwise the angle is free.
- **Ring tool:** click a few points round a base and it closes the loop itself, leans on cliffs, and offers a tower every so often and a gate where the workers' path crosses.
- **Blueprints:** a whole plan goes down as a ghost and is paid piece by piece as workers reach it, so a city wall can be planned early and rise over time.
- **Turning:** every other building turns in quarter turns with `R` or the scroll wheel while it is placed.

**How it works.** A wall is a chain of posts at half-tile precision, split into pieces of up to 6 tiles. Each piece is its own structure with its own hit points, so a breach is local. Each piece is stamped into the walking grid as a line one tile thick, so a diagonal wall is smooth on screen and a fine staircase in the grid. Buildings get a facing (0 to 3) that turns the footprint and the model and sets which side units come out of.

## Gates

- A gate dropped onto any wall replaces that stretch. It is at least 3 tiles wide, so a regiment folds into a column to pass (`Regiments::squeeze` already does this in tight ground).
- **Auto** (the default) opens as the owner's or an ally's troops arrive and shuts behind them. **Locked** is shut to everyone. **Open** is held open for a sally.
- An Auto gate won't shut on its own soldiers, so a rout back through the gate can carry the pursuers in with it.
- Enemy routes treat a shut gate as wall.
- At wall level 2 a gate becomes a gatehouse with room for archers above.

**How it works.** Gate cells become *doors* in the walking grid. A shut door is open ground to its owner's team and wall to everyone else, and an open door is open ground for all. Only door cells pay for the extra check, through a small side table, and the portal graph marks portals that cross a door with the teams that may use them. Route search is the most speed-sensitive code in the game (drill p99 is close to its 2 ms budget, see [12 · Performance](12-performance.md)), so this stage is measured first and may need a speed-up before it lands.

## Walls for every faction, with levels

| Level | What it is | Hit points (first pass) | Price (first pass) |
|---|---|---|---|
| 1 | A cheap, quick barrier. It stops walking and little else. | base | about 10 Grub a tile |
| 2 | A real wall with a walkway soldiers can stand on. | about 2.5 × level 1 | 15 Grub a tile more |
| 3 | Battlements and the faction's twist. Needs tier 2. | about 4 × level 1 | 20 Grub and 5 Marrow a tile more |

Double-clicking a wall selects the whole connected line, so it can be upgraded in one order. Workers walk along it upgrading piece by piece, and the wall stays up while they work.

| Faction | Level 1 | Level 2 | Level 3 | Gate | The twist |
|---|---|---|---|---|---|
| Ember Clans | Palisade (sharpened logs, burns) | Stone Wall (sloped stone base, plaster, tiled cap) | Castle Wall (loopholes, corner turrets) | Gate, then Yagura Gatehouse | Castle Walls drop stones on anyone hacking at their foot, and archers on them shoot faster through the loopholes. |
| Carrion Concord | Bone Fence (bone stakes and chain) | Ossuary Wall (stacked bone, iron bands) | Mausoleum Wall (lanterns, niches) | Iron Gate, then Gate of Debts | Every model, on any side, that dies at the foot of a Mausoleum Wall mends it a little. Sieges against the Concord feed the wall. |
| Shard Choir | Light Curtain (the player builds pylons; curtains form between them) | Glass Wall (solid crystal) | Singing Wall (slows whoever strikes it) | none | Choir troops pass straight through their own walls, so the Choir builds no gates. A wall without power drops, so cutting the power opens the base. |
| Brood | Resin Ridge (grows only along root-veins) | Chitin Wall (overlapping plates) | Thorn Bulwark (spines hurt melee attackers) | Maw Gate | Brood walls are grown, not built: the player marks the line and the Hive grows it along its veins with no workers. They heal on veins and die off when the veins are cut. |

The Choir's walls use the same door cells as gates, held open for the Choir's team at all times. Mario can still overrule the Choir pass-through rule (see [Calls for Mario](#calls-for-mario)).

## Manning the walls

- A regiment right-clicked onto a level 2 or 3 wall climbs at the nearest tower, gatehouse or stair and spreads along the walkway, facing out. The wall is its battle line, so one regiment holds about 15 to 25 tiles.
- **Height:** shooters on a wall reach 20% further. **Cover:** arrows at them do half damage. Melee can't reach them from the ground.
- **Ladders:** infantry attacking a manned wall can climb, slowly, and fight at a disadvantage on top. It is the cheap way in without siege. Spearmen and swordsmen on the wall are what stops climbers.
- When the stretch they stand on breaks, they come down with it, taking damage and disorder.

**How it works.** A manned regiment leaves the walking grid and is held by its wall pieces, with its soldiers in slots along them. Route search never has to know about wall tops.

## Towers

Three kinds, all of which fire:

- **Arrow towers** are the four towers in the game today, firing at last and each keeping its quirk. The Tollgate earns Grub, the Bell Tower silences abilities, the Spine Mound hits fliers, and the Ember Watchtower gets fire arrows that burn wood at level 2.
- **Bolt towers** fire one heavy shot that runs down a file of soldiers: the answer to monsters, cavalry and siege engines.
- **Blast towers** lob splash damage into packed blocks and at enemy siege, but can't hit anything close to them.

Towers snap into walls as joints and are where troops climb up. Each tower has two levels. Towers fire first at siege engines and anything hitting the owner's buildings, then at the nearest enemy, and a right-click focuses one target. Shots use the same volleys and arrows as regiments.

| Faction | Arrow (today's tower) | Bolt (new) | Blast (new) |
|---|---|---|---|
| Ember Clans | Watchtower: fire arrows at level 2 | Ballista Yagura | Hōroku Tower: fire-pots that leave burning ground |
| Carrion Concord | Tollgate: earns Grub | Collector's Bolt: a model it kills pays 1 Tithe | Corpse Catapult: a model killed by its hit rises as a Bone Pike |
| Shard Choir | Bell Tower: silences abilities | Prism Lance: a beam through the whole line | Chord Obelisk: a sound burst that disorders blocks |
| Brood | Spine Mound: hits fliers | Harpoon Maw: drags its target a few steps toward it | Acid Bloater: leaves an acid pool for a few seconds |

## Field works

- **Stakes:** a dragged line of sharpened stakes. Cavalry and monsters charging into it lose the charge and take hits. Infantry picks through slowly.
- **Plant stakes** is a regiment order: spearmen and archers hammer a stake line in front of themselves in 10 seconds, so fortification is a Field Battle tool too.
- **Barricades:** waist-high carts and logs, built in seconds. They stop walking but not arrows, and troops behind them take less from arrows and fight better across them.
- **One hidden trap per faction**, unseen by the enemy until sprung: the Ember spike pit, Concord grave-hands that hold a regiment in place, a Choir shard mine that stuns, and a Brood burrow nest that bursts out Skitterlings.

## Sieges you can see

- **Attackers spread along the building's face**, each soldier at a spot of their own, hacking, chopping or throwing torches, instead of a block standing beside it. Arrows stick in timber. A ram crew swings on a beat and the building shudders with each blow.
- **Every hit shows**, by material: splinters from wood, chips and dust from stone, shards from bone, sparks and a ring from crystal, ichor from chitin.
- **Damage looks:** cracks and smoke below two thirds of its hit points, fire and holes below one third.
- **Collapse:** each building's model is cut into chunks when it is baked. When it falls it shakes, sags and breaks apart in a cloud of dust. Ember timbers burn and fall in, Concord bones scatter and ghost-lights escape, Choir crystal shatters with a falling chord, Brood shells crack and deflate.
- **Rubble stays** as slow ground for a few minutes. A fallen wall piece leaves a breach, and a smashed gate becomes an open gap.

**How it works.** Almost all of this is drawing only (`gc_game` and `gc_render`, reading `Event::Fallen` and building hit points), so it can't upset the lockstep. It needs a small particle system and pre-cut models. Rubble is the one rule in the simulation: a slow-ground stamp on the walking grid that fades.

## Towns

- When walls and cliffs fully close round a player's main building, the base becomes a **Walled Town**. Buildings inside take a quarter less damage from ordinary regiments, and the owner's regiments fighting inside stand firmer and reinforce faster.
- The game checks enclosure only when a wall goes up or comes down, by flooding out from the main building across the walking grid with shut gates counted as wall.
- Towns look lived in, all as drawing only: paths worn by workers' real traffic, lanterns, chimney smoke, carts by the stores.
- The computer players learn all of this. They wall their bases with the ring tool, put towers on chokepoints, man walls when attacked, plant stakes against cavalry, and bring rams and ladders against gates and the weakest stretch ([18 · AI](18-ai.md)).

## Build plan

Eight stages, one pull request each. Each stage is playable on its own and lands with its docs, golden replays and computer-player support.

1. **Towers fire.** The four existing towers shoot, with their quirks, through the existing volleys. Target priorities and focus fire. The computer players respect tower range.
2. **Gates that open, buildings that turn.** Door cells in the walking grid with Auto, Locked and Open, measured against the route-search budget. Quarter turns for every building.
3. **Drawn walls for all four factions.** Drag, snap, any angle, ring tool and blueprints. Level 1 walls and gates for the Concord, Choir and Brood, Choir pass-through and Brood walls grown along veins.
4. **Wall levels and manning the walls.** Levels 2 and 3, walkways, height and cover, ladders, gatehouses and the four faction twists.
5. **Bolt and Blast towers.** Eight new towers with their quirks, and tower levels.
6. **Field works.** Stakes, barricades, the Plant stakes order and the four hidden traps.
7. **Sieges you can see.** Particle system, hit effects, attackers along the face, damage looks, collapses, rubble and breaches. This stage is drawing work and can run in a second thread alongside the others.
8. **Walled towns and smarter sieges.** The town bonus, town life, and computer players that wall, man walls and assault properly.

**Who builds it** (per [15 · Staffing Plan](15-staffing-plan.md) and the project rules): the simulation stages are led by Opus, with Sonnet helpers for the placing tool, interface and drawing. The wall, tower and collapse art is 3D modelling, which runs on Sonnet 5.5 alone (Mario, 2026-10-23). Haiku helpers do the doc updates.

**Costs to watch:** door checks in route search (stage 2), tower targeting and manned walls in every tick (stages 1 and 4), particles and collapse chunks against the 6 GB budget (stage 7). Every stage that changes the rules bumps `SIM_VERSION` and re-records the golden replays.

## Calls for Mario

Each has a recommendation and can change while the stages are built:

- **Choir walls let Choir troops walk through, so the Choir builds no gates.** Recommended: keep. It fits the Choir's teleporting armies, and it gives them a weakness: no power, no wall.
- **Walled towns get a defence bonus.** Recommended: keep, small. It is the reason to wall the whole base and not just the front.
- **Regiments plant their own stakes.** Recommended: keep, for spearmen and archers only.
