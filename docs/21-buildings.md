# 21 · Buildings

Every faction's buildings with first-pass numbers for Conquest (M3): cost, build time, hit points, armour, size and supply. [03 · World and Factions](03-factions.md) says what each building is for and [04 · Economy](04-economy.md) sets the resources, supply and tier costs these numbers sit on. The canonical values live in the `buildings` list of each `data/rosters/*.ron` file, not in this document. `tools/gc_bake/tests/rosters.rs` checks them against the rules below and against the units that train in each building.

None of this is in the game yet. Construction arrives with M3 thread A ([15 · Staffing Plan](15-staffing-plan.md#m3--first-harvest-the-first-agent-team)), built on these numbers.

## Shared rules

- **Times** are seconds at Standard tempo. The lobby's tempo setting scales them like research ([04 · Economy](04-economy.md#tempo-setting)).
- **Size** is the footprint in tiles, across and deep. Buildings are 2×2 to 5×5. Walls are one tile thick.
- **Supply** is what a building adds to the cap. Only the buildings that docs/04 lists add any. For a main building's upgrades it is the total for that tier: each tier adds 10. Every faction starts on 12; the Choir's Spire gives 4 and its two starting Resonators give the rest.
- **Tier** is the main-building tier needed to place the building. A building's tier is never above the tier of the units it trains, so nothing has to wait for a building to train a unit its tier already allows.
- **Tier 1 costs no Marrow.** This holds for buildings as it does for units, so a player who loses their Marrow vein can still rebuild a base.
- **Construction.** A building appears at 10% of its hit points and fills up as it is built. Hands, Tuners and Ledger Clerks stay with it until it is finished. Cancelling refunds 75% of the cost. A destroyed building refunds nothing.
- **Repair** is free. Hands and Ledger Clerks repair 8 hit points a second, Tuners 10 ([17 · Unit Rosters](17-unit-rosters.md)).
- **Tier upgrades** of the main building are listed as their own rows with the cost and time from [04 · Economy](04-economy.md#tech-tiers). The building keeps working while it upgrades.

### Faction rules

- **Ember Clans:** the sturdiest bases. The halls, towers and walls are Fortified, and so is the Outpost Hall used to expand.
- **Brood:** every structure is grown by using up the Gnawer that starts it, so its real price is the listed Grub plus a worker (50 Grub and a larva). Upgrading the Hive uses no Gnawer. All Brood structures are Living. Nothing repairs them, but they regrow 2 hit points a second while on root-veins.
- **Choir:** every building except the Spire, Resonators and Siphons needs power. It must stand inside the field of a Spire (8 tiles) or a Resonator or Siphon (6 tiles). An unpowered building stops training, researching and shooting until power returns.
- **Carrion Concord:** the Counting House and Tollgate are Fortified, and everything else is Structure, including the Waystation. The Reliquary (75 Grub, 2×2) is small and cheap, made to be planted beside other players' fights.

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
- A Watchtower kills a lone Yari regiment in about 45 s. The regiment needs about 80 s to topple the tower, so one tower holds off one regiment. Three regiments take it in under 30 s and lose a little over half a regiment.

**Opening at Standard tempo:**
- The Clans start with 300 Grub and 5 Hands. A Farmstead (80) goes down at about 0:30 and a Dojo (150) at about 1:30.
- Tier 2 (300 Grub, 100 Marrow, 60 s) lands around minute 5 to 6, matching the pacing target in 04.
- The tier-2 production buildings cost 40 to 60 Marrow each. A tier-2 army therefore competes with the next tier and with elite units for the same scarce Marrow.

**Not designed yet:** research costs and times, Grubfolk trade prices, and sight ranges for buildings.

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
| Palisade | 1 | 20 / 0 | 8 s | 500 | Fortified | 1×2 |  | One wall segment. Blocks movement; Fire Arrows burn it. |
| Gate | 1 | 60 / 0 | 20 s | 1,000 | Fortified | 1×3 |  | A wall segment that opens for your own and allied regiments. |

## Brood

| Building | Tier | Cost (Grub / Marrow) | Build time | Hit points | Armour | Size | Supply | What it does |
|---|---|---|---|---|---|---|---|---|
| Hive | 1 | 300 / 0 | 100 s | 2,600 | Living | 5×5 | +12 | Main building. Holds 3 larvae and spawns one every 12 s; every regiment hatches from a larva. Root-veins spread 10 tiles around it. |
| Great Hive (upgrades Hive) | 2 | 300 / 100 | 60 s | 3,400 | Living | 5×5 | +22 | Tier 2: evolutions, Drone-Moths, the Broodmother Bulk and a second hero. |
| Throne Hive (upgrades Great Hive) | 3 | 600 / 250 | 90 s | 4,200 | Living | 5×5 | +32 | Tier 3: monsters, top upgrades and a third hero. |
| Pustule | 1 | 40 / 0 | 20 s | 400 | Living | 2×2 | +8 | Carries the root-veins 8 tiles further; kill it and every seep beyond stops delivering. |
| Hatchery Pit | 1 | 120 / 0 | 40 s | 900 | Living | 3×3 |  | Holds 3 more larvae and spawns its own every 12 s. |
| Chrysalis Chamber | 2 | 150 / 50 | 50 s | 1,100 | Living | 3×3 |  | Unlocks evolutions: tier-1 regiments can cocoon into their tier-2 forms anywhere on the field. |
| Moth Roost | 2 | 150 / 40 | 50 s | 1,000 | Living | 3×3 |  | Lets larvae hatch into Drone-Moths. |
| Gorge | 2 | 180 / 60 | 60 s | 1,300 | Living | 4×4 |  | Lets larvae hatch into Broodmother Bulks and, at tier 3, the Colossus Maw. |
| Spine Mound | 1 | 100 / 0 | 35 s | 900 | Living | 2×2 |  | Flings spines at ground and air. Range 30, 1 × 10 damage every 2 s. |
| Queen's Chamber | 1 | 140 / 0 | 45 s | 1,100 | Living | 3×3 |  | Hatches and revives heroes. |

## Choir

| Building | Tier | Cost (Grub / Marrow) | Build time | Hit points | Armour | Size | Supply | What it does |
|---|---|---|---|---|---|---|---|---|
| Spire | 1 | 400 / 0 | 110 s | 2,800 | Fortified | 4×4 | +4 | Main building. Builds Tuners and Conductors and powers buildings within 8 tiles. |
| Cathedral (upgrades Spire) | 2 | 300 / 100 | 60 s | 3,600 | Fortified | 4×4 | +14 | Tier 2: Harmonic Walkers, Glass Striders, Skyglass, the Conductor and a second hero. |
| Grand Organ (upgrades Cathedral) | 3 | 600 / 250 | 90 s | 4,400 | Fortified | 4×4 | +24 | Tier 3: monsters, top upgrades and a third hero. |
| Resonator | 1 | 150 / 0 | 25 s | 500 | Structure | 2×2 | +4 | Tunes a seep within 4 tiles to make Grub with no workers, links with Resonators within 14 tiles into chords, and powers buildings within 6 tiles. |
| Siphon | 1 | 175 / 0 | 30 s | 500 | Structure | 2×2 | +4 | A Resonator built on a Marrow vein: about 50 Marrow a minute. Links chords and powers buildings like a Resonator. |
| Forge of Facets | 1 | 160 / 0 | 50 s | 1,300 | Structure | 4×4 |  | Builds every Choir regiment that walks. |
| Harmonics Lab | 1 | 175 / 0 | 50 s | 1,100 | Structure | 3×3 |  | Researches upgrades. |
| Skyglass Loom | 2 | 160 / 50 | 55 s | 1,100 | Structure | 3×3 |  | Builds Skyglass. |
| Bell Tower | 1 | 150 / 0 | 40 s | 900 | Fortified | 2×2 |  | A beam that hits ground and air. Every 20 s it tolls, silencing enemy abilities within 10 tiles for 5 s. Range 30, 1 × 10 damage every 2 s. |
| Sanctum of Echoes | 1 | 150 / 0 | 50 s | 1,100 | Structure | 3×3 |  | Builds and restores heroes. |

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
