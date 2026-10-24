# 21 · Buildings

Every faction's buildings with first-pass numbers for Conquest (M3): cost, build time, hit points, armour, size and supply. [03 · World and Factions](03-factions.md) says what each building is for and [04 · Economy](04-economy.md) sets the resources, supply and tier costs these numbers sit on. The canonical values live in the `buildings` list of each `data/rosters/*.ron` file, not in this document. `tools/gc_bake/tests/rosters.rs` checks them against the rules below and against the units that train in each building.

`gc_bake` bakes these buildings into the pack's building table (`data/buildings`, with the starting count of each). **Built (2026-10-16, M3 step 1):** the simulation has buildings: footprints that block walking, hit points, armour and the damage table below, and regiments ordered to attack a building close in and knock it down. The sandbox lets you put buildings down and knock them over (`K`, see [06 · Game Modes](06-game-modes.md)). Construction, training, supply and towers follow in later steps ([15 · Staffing Plan](15-staffing-plan.md#m3--first-harvest-the-first-agent-team)). **Built (2026-10-16, M3 step 3):** workers lay foundations (`Command::Build`, paid for when laid) that give nothing until they are finished, with up to four builders at once (the first at full speed, each further one at half). A building queues up to five regiments, which wait for supply before training. Calling off a building or an upgrade refunds 75%. Main buildings upgrade on their own, with no builders, and keep working meanwhile. An upgrade keeps its building's footprint (the bake and the simulation refuse one that doesn't). A unit's tier is checked when it is queued, so a regiment queued before its owner's keep falls still trains. A regiment comes out of the south side, or another side if that is blocked (the front turns with the building since fortifications stage 2). The lobby and HUD for these come in step 4.

**Built (2026-10-17, buildings brought to life):** all 46 Conquest buildings of the four factions have 3D models with visible construction stages (main halls and upgrades in PR #149, the other 33 after). The game picks the look from the simulation's percent built, and the simulation is unchanged. A building with no model is still drawn as the old placeholder box, with its hit bar. Looks, upgrades, workers, packs and budgets are in [Models and construction stages](#models-and-construction-stages) below.

**Planned (approved 2026-10-22, not built):** walls for every faction, working gates, towers that fire, field works, collapses and walled towns are designed in [26 · Fortifications and Bases](26-fortifications.md). Towers shoot since stage 1, and since stage 2 the Ember Gate lets its own side and allies through (Auto, Locked or Open, see [26](26-fortifications.md#gates)).

## Shared rules

- **Times** are seconds at Standard tempo. The lobby's tempo setting scales them like research ([04 · Economy](04-economy.md#tempo-setting)).
- **Size** is the footprint in tiles, across and deep. Buildings are 2×2 to 5×5. Walls are one tile thick.
- **Ground** must be clear: a building can't rise on scrub, woods or thicket. Laying one there is allowed; the worker pays, clears the footprint first, then lays the foundation (see [31 · Changing land](32-changing-land.md)).
- **Supply** is what a building adds to the cap. Only the buildings that docs/04 lists add any. For a main building's upgrades it is the total for that tier: each tier adds 10. Every faction starts on 12; the Choir's Spire gives 4 and its two starting Resonators give the rest.
- **Tier** is the main-building tier needed to place the building. A building's tier is never above the tier of the units it trains, so nothing has to wait for a building to train a unit its tier already allows.
- **Tier 1 costs no Marrow.** This holds for buildings as it does for units, so a player who loses their Marrow vein can still rebuild a base.
- **Construction.** A building appears at 10% of its hit points and fills up as it is built. Hands, Tuners and Ledger Clerks stay with it until it is finished. Cancelling refunds 75% of the cost. A destroyed building refunds nothing.
- **Repair** is free. Hands and Ledger Clerks repair 8 hit points a second, Tuners 10 ([17 · Unit Rosters](17-unit-rosters.md)). **Built in step 3:** every builder mends at 8 hit points a second, and the Tuners' 10 is not built yet.
- **Tier upgrades** of the main building are listed as their own rows with the cost and time from [04 · Economy](04-economy.md#tech-tiers). The building keeps working while it upgrades.

### Faction rules

- **Ember Clans:** the sturdiest bases. The halls, towers and walls are Fortified, and so is the Outpost Hall used to expand. **Built (2026-10-17, faction depth stage 5a):** a finished Ancestor Shrine (flag Honor) earns its owner a point of Honor every 30 seconds, if their meter is Honor (see [03 · Faction meter: Honor](03-factions.md#faction-meter-honor-0-to-100)).
- **Brood:** every structure is grown by using up the Gnawer that starts it, so its real price is the listed Grub plus a worker (50 Grub and a larva). Upgrading the Hive uses no Gnawer. All Brood structures are Living. Nothing repairs them, but they regrow 2 hit points a second while on root-veins. **Built (2026-10-17, faction depth stage 1):** the Gnawer is used up at the foundation, and the building then grows on its own at one builder's speed; other Gnawers can't help it or mend it. **Built (2026-10-17, faction depth stage 2):** a finished Living building standing within reach of its owner's linked root-veins regrows 2 hit points a second.
- **Choir:** every building except the Spire, Resonators and Siphons needs power. It must stand inside the field of a Spire (8 tiles) or a Resonator or Siphon (6 tiles). An unpowered building stops training, researching and shooting until power returns. **Built (2026-10-17, faction depth stage 3):** a Choir building that needs power can only be laid where one of its owner's finished fields reaches its footprint ("It must stand inside a field of power."), and while none does its training queue waits. The field's reach is the `field` of the building in `data/rosters/choir.ron` (Spire tiers 8, Resonator and Siphon 6), and the buildings that need power have the `NeedsPower` flag. Researching is not built yet. **Built (2026-10-24):** an unpowered Choir tower doesn't shoot (see [26 · Fortifications](26-fortifications.md#build-plan), stage 1). **Built (stage 4):** finished Resonators and Siphons whose centres are within 14 tiles link (their `links`), and three mutually linked make a chord that speeds the seeps inside it and charges Resonance (see [03 · Factions](03-factions.md)).
- **Carrion Concord:** the Counting House and Tollgate are Fortified, and everything else is Structure, including the Waystation. The Reliquary (75 Grub, 2×2) is small and cheap, made to be planted beside other players' fights.

## Models and construction stages

Each building has four models, and the game shows one of them from the building's percent built (`StructureView::built_percent`):

| Look | Shown at | Model |
|---|---|---|
| Foundation | 0 to 24% built | `<Name> #1` |
| Frame with scaffolding | 25 to 59% | `<Name> #2` |
| Nearly done | 60 to 99% | `<Name> #3` |
| Finished | 100% | `<Name>` |

- **Upgrades** of a main building have only `#3` and the finished look. For the first half of an upgrade the game shows the old tier, and for the second half the upgrade's `#3`.
- **Workers** on a building going up, or being mended, turn to face it and play their attack clips as work. An upgrade has no builders, so three of that faction's workers (Ember Hands, Concord Ledger Clerks, Choir Tuners or Brood Gnawers) are drawn working at the walls. This is drawing only.
- **Packs.** Each faction's buildings are baked into `assets/buildings/<faction>.pak` from `assets-src/buildings/<faction>.ron`. GitHub refuses files over 100 MB and the soldier pack is 80 MB, so they are separate. The build script joins the soldier pack and every building pack into one pack in the build directory, which the executable embeds. CI checks each building pack against its manifest, as it does the soldier pack. The Ember Clans' buildings are split over `ember.pak` (halls and town) and `ember-defences.pak` (shrine, forge, tower and walls). `fortifications.pak` (from `assets-src/buildings/fortifications.ron`) holds the faction gates, starting with the Carrion Concord's Iron Gate and the Brood's Maw Gate (1×3, same size and look slots as the Ember Gate; scripts `concord/iron_gate.py` and `brood/maw_gate.py`). A finished look may reach 0.45 tiles past its footprint and a construction look's scaffolding 0.6 (the `building_models_dress_buildings` test).
- **Budgets** (finished look, full detail, in triangles): main buildings and upgrades 18,000; outposts 14,000; 3×3 to 5×5 buildings 10,000; 2×2 towers and small pieces 6,000; walls and gates 2,500. Each has three smaller levels of detail. All four looks of a building share one 512 × 512 material, baked at 1024.
- **How each faction builds:**
  - **Ember Clans:** raw-timber pole scaffolding, and stone and log piles.
  - **Carrion Concord:** bone-and-iron scaffolding, coffins and ledger crates.
  - **Shard Choir:** a floating pewter lattice, with hovering shards singing the building into shape.
  - **Brood:** no scaffolding. The building is grown: a root-vein web and resin mound, then a chitin ribcage in a membrane cocoon, then a hardening shell.
- **Status.** The first model is the Ember Clans' Clan Hall. The other main buildings and their upgrades follow in this batch, and the other 46 buildings in later batches. `--showroom buildings` stands every look in a row (`--showroom-clip idle` holds one clip).

## Damage against buildings

Buildings have one of three armour types (see [05 · Combat](05-combat.md#structures-in-combat)). A blow against a building always lands, because buildings don't parry. Then the attacker's damage is multiplied:

| Attacker | Structure | Fortified | Living |
|---|---|---|---|
| Regiments | ×1.0 | ×0.5 | ×1.0 |
| Heroes | ×1.0 | ×1.0 | ×1.0 |
| Siege (Battering Ram, Mausoleum Engine) | ×2.0 | ×3.0 | ×2.0 |
| Artillery (rocket carts, Broodmother Bulk) | ×1.5 | ×1.0 | ×1.5 |

Abilities that already name structures (Fire Arrows, Fire Pots, Repossess, Sonic Beam, Prismatic Lance) apply on top of this table.

Towers shoot like a few archers on a roof. Each volley fires the listed number of shots, and every shot hits. Every tower outranges every regiment except artillery, siege and monsters (the longest is the Yumi Ashigaru at 30 tiles), so archers can't pick one off from beyond its reach.

**What the numbers give, on a quick count:**
- One Battering Ram (40 a blow, ×3.0) brings down a Clan Hall in about 25 s.
- A regiment of Yari Ashigaru strikes with its front rank and, being spearmen, half its second rank: about 12 blows of 2 a second. It burns a Farmstead in about 25 s. Against a Clan Hall it does half damage, so it would need about 4 minutes. Infantry raids should go for workers, housing and caravans, and siege breaks bases.
- A Watchtower kills a lone Yari regiment in about 45 s (about 55 s since volleys were slowed to 85%; towers fire from 2026-10-24). The regiment needs about 80 s to topple the tower, so one tower holds off one regiment. Three regiments take it in under 30 s and lose a little over half a regiment.

**Opening at Standard tempo:**
- The Clans start with 300 Grub and 5 Hands. A Farmstead (80) goes down at about 0:30 and a Dojo (150) at about 1:30.
- Tier 2 (300 Grub, 100 Marrow, 60 s) lands around minute 5 to 6, matching the pacing target in 04.
- The tier-2 production buildings cost 40 to 60 Marrow each. A tier-2 army therefore competes with the next tier and with elite units for the same scarce Marrow.

**Sieges on screen** (drawing and sound only; `gc_game/src/siege.rs`; dust, smoke, fire, sparks and small chips are particles from the shared system in [30](30-cinematic-lighting.md#built-stage-3), the shots, big chunks, stuck arrows and rubble heaps are boxes). The simulation tells every blow on a building as `Event::Struck` (who struck it), and its fall as `Event::Fallen`. The game draws them so it is plain when troops are at a building:
- **At the wall.** Melee regiments attacking a building step up to its walls (the model's walls, not the footprint's edge) and swing at it; the front rank swings, the ranks behind wait. A ram draws back between blows and swings in to land each one. Archers turn to the building and shoot.
- **Shots.** Archers' volleys fly to the wall and break on it. Siege engines (Battering Ram class, Mausoleum Engine) throw a stone; artillery (rocket carts, Broodmother Bulk) throws a burning shot that trails smoke. Each shot is one the simulation struck, landing a moment after the bar drops.
- **Hits.** Each blow shakes the building and its bar flashes white. What flies off where it landed depends on what the building is made of: timber throws splinters, stone chips and sparks, the Brood's chitin sprays yellow-green ichor, the Choir's crystal throws glittering shards, and the Concord's bone chips and lets a ghost-light slip out. Every blow raises a little dust, a heavy one (ram, stone, rocket) a cloud, and knocks a few real chunks off that lie where they fall. Arrows that hit timber, chitin or bone stay in the wall for 40 s.
- **Damage.** A finished building below 70% of its hit points smokes from the roof, with more columns as it gets worse; below 35% it burns, with flames and drifting embers. A building still going up doesn't, since it has few hit points yet. Smoke and fire are only made for buildings the camera sees.
- **Collapse.** A building brought down sinks into the ground in about 2.4 s in a cloud of dust that rolls along its whole length (a wall piece's too) and flying chunks, each faction's in its own way: Ember Clan timbers burn and throw embers, stone drops in a heavy cloud with sparks, Concord bones scatter and ghost-lights escape, Choir crystal shatters in glitter, and Brood shells burst and spray ichor. It leaves a rubble heap that smoulders for 25 s and lies for 90 s. Rubble does not yet slow walkers (that would be a simulation rule).
- **Sounds.** Blades on walls (`WallHit`), arrows thudding in, siege engines firing (`SiegeLaunch`) and landing (`SiegeImpact`), and a building coming down (`Collapse`), which carries far.
- **Alerts.** One of your buildings struck out of view raises the same edge badge as a regiment under attack.
- **Try it:** `grubcraft --siege` stages a siege in the sandbox: Ember Clans blades and rocket carts at a Clan Hall, archers at a Farmstead, a Battering Ram at a Watchtower and a Mausoleum Engine at a Tenement.

**Sight in fog of war** (built in Conquest step 5b, `Role::sight` in `gc_sim::structures`): a main building sees 32 tiles round its walls, an outpost 28, a tower 36 (past its 32-tile reach), a wall 8, and every other building 20. Regiments see 32.

**Not designed yet:** research costs and times, and Grubfolk trade prices.

## Ember Clans

| Building | Tier | Cost (Grub / Marrow) | Build time | Hit points | Armour | Size | Supply | What it does |
|---|---|---|---|---|---|---|---|---|
| Clan Hall | 1 | 400 / 0 | 120 s | 3,000 | Fortified | 5×5 | +12 | Main building. Trains Hands, takes Grub and Marrow, and reinforces regiments nearby. |
| Castle Keep (upgrades Clan Hall) | 2 | 300 / 100 | 60 s | 3,800 | Fortified | 5×5 | +22 | Tier 2: samurai, cavalry, rocket carts, War Kites and a second hero. |
| Daimyo's Citadel (upgrades Castle Keep) | 3 | 600 / 250 | 90 s | 4,600 | Fortified | 5×5 | +32 | Tier 3: monsters, top upgrades and a third hero. |
| Outpost Hall | 1 | 250 / 0 | 80 s | 2,000 | Fortified | 4×4 | +6 | A cheaper hall for expansions: takes Grub and Marrow and reinforces, but trains nothing and cannot be upgraded. |
| Farmstead | 1 | 80 / 0 | 25 s | 600 | Structure | 3×3 | +10 | Housing. |
| Granary | 1 | 100 / 0 | 30 s | 800 | Structure | 3×3 |  | A Grub drop-off to build beside seeps, so Hands walk less. Researches the Clans' economy upgrades. |
| Dojo | 1 | 150 / 0 | 50 s | 1,400 | Structure | 4×4 |  | Trains spearmen, archers and samurai. Reinforces regiments nearby. |
| Stables | 2 | 180 / 40 | 60 s | 1,400 | Structure | 5×4 |  | Trains cavalry. Reinforces regiments nearby. |
| Ember Workshop | 1 | 160 / 0 | 55 s | 1,400 | Structure | 4×4 |  | Builds battering rams and, at tier 2, rocket carts. |
| Kite Loft | 2 | 160 / 60 | 60 s | 1,100 | Structure | 3×3 |  | Builds War Kites. |
| Ancestor Shrine | 1 | 150 / 0 | 50 s | 1,200 | Structure | 3×3 |  | Hires and revives heroes and holds the Honor Edicts. Binds the Oni at tier 3. |
| Forge | 1 | 175 / 0 | 50 s | 1,200 | Structure | 3×3 |  | Researches weapon and armour upgrades. |
| Watchtower | 1 | 125 / 0 | 40 s | 1,000 | Fortified | 2×2 |  | Two archers that shoot at ground and air. Range 32, 2 × 5 damage every 2 s. |
| Palisade | 1 | 10 / 0 a tile | 4 s a tile | 250 a tile | Fortified | 1×1, drawn as a line | Line | A wall you draw at any angle, priced and built a tile at a time ([26](26-fortifications.md#drawn-walls)). Blocks movement; Fire Arrows burn it. |
| Gate | 1 | 60 / 0 | 20 s | 1,000 | Fortified | 1×3 | Gate | A wall segment that opens for your own and allied regiments. Laid on your Palisade, it replaces that stretch. |

## Brood

| Building | Tier | Cost (Grub / Marrow) | Build time | Hit points | Armour | Size | Supply | What it does |
|---|---|---|---|---|---|---|---|---|
| Hive | 1 | 300 / 0 | 100 s | 2,600 | Living | 5×5 | +12 | Main building. Holds 3 larvae and spawns one every 12 s; every regiment hatches from a larva. Root-veins spread 15 tiles from its edge (built: enough to reach its base's seeps and vein on every map). |
| Great Hive (upgrades Hive) | 2 | 300 / 100 | 60 s | 3,400 | Living | 5×5 | +22 | Tier 2: evolutions, Drone-Moths, the Broodmother Bulk and a second hero. |
| Throne Hive (upgrades Great Hive) | 3 | 600 / 250 | 90 s | 4,200 | Living | 5×5 | +32 | Tier 3: monsters, top upgrades and a third hero. |
| Pustule | 1 | 40 / 0 | 20 s | 400 | Living | 2×2 | +8 | Carries the root-veins 8 tiles further; kill it and every seep beyond stops delivering. |
| Hatchery Pit | 1 | 120 / 0 | 40 s | 900 | Living | 3×3 |  | Holds 3 more larvae and spawns its own every 12 s. |
| Chrysalis Chamber | 2 | 150 / 50 | 50 s | 1,100 | Living | 3×3 |  | Unlocks evolutions: tier-1 regiments can cocoon into their tier-2 forms anywhere on the field. |
| Moth Roost | 2 | 150 / 40 | 50 s | 1,000 | Living | 3×3 |  | Lets larvae hatch into Drone-Moths. |
| Gorge | 2 | 180 / 60 | 60 s | 1,300 | Living | 4×4 |  | Lets larvae hatch into Broodmother Bulks and, at tier 3, the Colossus Maw. |
| Spine Mound | 1 | 100 / 0 | 35 s | 900 | Living | 2×2 |  | Flings spines at ground and air. Range 30, 1 × 10 damage every 2 s. |
| Queen's Chamber | 1 | 140 / 0 | 45 s | 1,100 | Living | 3×3 |  | Hatches and revives heroes. |
| Resin Ridge | 1 | 8 / 0 a tile | 5 s a tile | 200 a tile | Living | 1×1, drawn as a line | Line, Grown | Grown by the Hive along its root-veins, every tile at once, with no Gnawer ([26](26-fortifications.md#drawn-walls)). Heals on the veins and withers off them. |
| Maw Gate | 1 | 55 / 0 | 20 s | 900 | Living | 1×3 | Gate | Chitin jaws that part for your own and allied regiments. Laid on your Resin Ridge, it replaces that stretch. |

## Choir

| Building | Tier | Cost (Grub / Marrow) | Build time | Hit points | Armour | Size | Supply | What it does |
|---|---|---|---|---|---|---|---|---|
| Spire | 1 | 400 / 0 | 110 s | 2,800 | Fortified | 4×4 | +4 | Main building. Builds Tuners and Conductors and powers buildings within 8 tiles. |
| Cathedral (upgrades Spire) | 2 | 300 / 100 | 60 s | 3,600 | Fortified | 4×4 | +14 | Tier 2: Harmonic Walkers, Glass Striders, Skyglass, the Conductor and a second hero. |
| Grand Organ (upgrades Cathedral) | 3 | 600 / 250 | 90 s | 4,400 | Fortified | 4×4 | +24 | Tier 3: monsters, top upgrades and a third hero. |
| Resonator | 1 | 150 / 0 | 25 s | 500 | Structure | 2×2 | +4 | Tunes a seep within 4 tiles to make Grub with no workers, links with Resonators within 14 tiles into chords, and powers buildings within 6 tiles. |
| Siphon | 1 | 175 / 0 | 30 s | 500 | Structure | 2×2 | +4 | A Resonator built against a Marrow vein (within 3 tiles of its edge): about 50 Marrow a minute. Links chords and powers buildings like a Resonator. |
| Forge of Facets | 1 | 160 / 0 | 50 s | 1,300 | Structure | 4×4 |  | Builds every Choir regiment that walks. |
| Harmonics Lab | 1 | 175 / 0 | 50 s | 1,100 | Structure | 3×3 |  | Researches upgrades. |
| Skyglass Loom | 2 | 160 / 50 | 55 s | 1,100 | Structure | 3×3 |  | Builds Skyglass. |
| Bell Tower | 1 | 150 / 0 | 40 s | 900 | Fortified | 2×2 |  | A beam that hits ground and air. Every 20 s it tolls, silencing enemy abilities within 10 tiles for 5 s. Range 30, 1 × 10 damage every 2 s. |
| Sanctum of Echoes | 1 | 150 / 0 | 50 s | 1,100 | Structure | 3×3 |  | Builds and restores heroes. |
| Light Curtain | 1 | 10 / 0 a tile | 3 s a tile | 180 a tile | Fortified | 1×1, drawn as a line | Line, Pass, Needs power | A wall of light drawn inside your power ([26](26-fortifications.md#drawn-walls)). Your own troops walk through it; it drops for everyone without power. |

## Carrion Concord

| Building | Tier | Cost (Grub / Marrow) | Build time | Hit points | Armour | Size | Supply | What it does |
|---|---|---|---|---|---|---|---|---|
| Counting House | 1 | 400 / 0 | 120 s | 2,800 | Fortified | 5×5 | +12 | Main building. Trains Clerks and Caravans. Caravan routes start here, and Clerks bring Marrow here. |
| Exchange (upgrades Counting House) | 2 | 300 / 100 | 60 s | 3,600 | Fortified | 5×5 | +22 | Tier 2: Gravewardens, Debt Collectors, Lantern Wraiths and a second hero. |
| Grand Bourse (upgrades Exchange) | 3 | 600 / 250 | 90 s | 4,400 | Fortified | 5×5 | +32 | Tier 3: monsters, top upgrades and a third hero. |
| Waystation | 1 | 200 / 0 | 60 s | 1,200 | Structure | 4×4 |  | Your own trade post for caravan routes (half rate), and a Marrow drop-off for expansions. |
| Tenement | 1 | 80 / 0 | 25 s | 650 | Structure | 3×3 | +10 | Housing. |
| Reliquary | 1 | 75 / 0 | 20 s | 400 | Structure | 2×2 |  | Every model that dies within 12 tiles, on any side, pays 1 Tithe. Can be built anywhere you have vision. |
| Ossuary | 1 | 150 / 0 | 50 s | 1,400 | Structure | 4×4 |  | Raises the undead: Bone Pikes, Gravewardens, and at tier 3 the Mausoleum Engine and the Insolvent. |
| Guildhall | 1 | 160 / 0 | 50 s | 1,400 | Structure | 4×4 |  | Trains living infantry and cavalry. Living regiments reinforce near it. |
| Lantern Spire | 2 | 160 / 50 | 55 s | 1,000 | Structure | 3×3 |  | Trains Lantern Wraiths. |
| Tollgate | 1 | 140 / 0 | 40 s | 1,000 | Fortified | 2×2 |  | Two crossbows that shoot at ground and air. Every 4 damage it deals earns 1 Grub. Range 30, 2 × 5 damage every 2 s. |
| Vault of Names | 1 | 150 / 0 | 50 s | 1,200 | Structure | 3×3 |  | Hires and revives heroes. |
| Bone Fence | 1 | 12 / 0 a tile | 4 s a tile | 300 a tile | Fortified | 1×1, drawn as a line | Line | Bone stakes and chain, drawn at any angle ([26](26-fortifications.md#drawn-walls)). Blocks movement. |
| Iron Gate | 1 | 70 / 0 | 22 s | 1,200 | Fortified | 1×3 | Gate | Iron bars between bone posts that open for your own and allied regiments. Laid on your Bone Fence, it replaces that stretch. |
