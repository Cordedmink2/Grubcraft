# 17 · Unit Rosters

Every regiment, worker, summon and hero for all four factions, with first-pass stats and abilities, ready for Conquest (M3) and the Choir and Concord milestone (M5). [03 · World and Factions](03-factions.md) gives each faction's fantasy, economy and structures; this document gives the numbers.

The numbers live as data in `data/rosters/*.ron`, one file per faction, and the tables here are a readable copy of them. Where the two disagree, the data files win. A test (`tools/gc_bake/tests/rosters.rs`) checks every roster entry against the same rules the baker applies to `data/units.ron`, so each stat block can be moved into the game unchanged.

**Built so far:** `gc_bake` bakes every roster building and every trainable unit (not the heroes) into two pack tables, `data/buildings` and `data/conquest_units`, which the game checks when it loads its data; Conquest arrives in steps, so nothing in play uses them yet. A Conquest unit's record is its `stats` block baked exactly as a `units.ron` entry, plus its cost, supply, tier, class, flags, train time, the building that trains it or the unit it evolves from, and how many a player starts with (`starting_units`, from [04 · Economy](04-economy.md#starting-state-conquest)). Field Battles still read `data/units.ron`, which holds five of these regiments (Yari Ashigaru, Yumi Ashigaru, Skitterlings, Shardlings, Bone Pikes); their roster entries copy its numbers as of the [Field Battle balance pass](#field-battle-balance), and `units.ron` stays the live copy for those five until the rosters replace it. The same test fails if a live regiment's stats (everything but Field Battle points) differ from its roster entry, so tuning one file without the other is caught. Costs, supply, tiers, abilities, heroes and every class rule below need the Conquest systems of M3 before they do anything; [Schema proposals](#schema-proposals) lists what the engine needs to add.

## How to read the tables

- **T**: tech tier (1 to 3). **Cost**: Grub / Marrow. **Sup**: supply. **Time**: seconds to train (for Brood evolutions, the cocoon time).
- **From**: where it is trained, what it evolves from (Brood), or what summons it.
- **Models × HP**: soldiers at full strength and hit points each. **Files**: soldiers per rank when raised.
- **Speed**: tiles per second. **Turn**: degrees per second. **Spacing**: tiles between soldiers across / front to back (for one-model units, its footprint).
- **Atk / Def / Dmg**: melee attack, melee defence, damage per landed blow. A blow lands with chance 50 + attack − defence percent (10 to 90). Each front-rank soldier strikes once a second (spears and pikes add half their second rank; see [Weapon flags and mass](#weapon-flags-and-mass)).
- **Chg**: extra melee damage, percent, when a charge hits home (fades over 3 s).
- **Mor**: morale when fresh, 1 to 100. "—" means it never routs.
- **Ranged**: range in tiles, damage per hit, seconds between volleys, percent of shots that hit a block of normal density.
- **New** marks a regiment this roster adds to the faction lists in 03 to fill a gap.

Abilities are listed as **Name** · kind · cooldown · range · radius · duration, then what it does, then in brackets the effect primitives it would be built from (see [10 · Architecture](10-architecture.md#abilities-as-data)). Kinds: *passive* (always on), *aura* (always on, affects others nearby), *toggle* (switched on and off), *active* (cast, then cools down), *autocast* (fires by itself when ready), *order* (a command with no cooldown, such as building).

## Classes and counters

Every unit has one **class**. Class drives soft counters (the [counters table](05-combat.md#counters-soft-rock-paper-scissors) in 05) and the supply rule.

| Class | What it is | Supply |
|---|---|---|
| Worker | Gathers and builds | 1 |
| Light | Fast, fragile infantry and swarms | 3 |
| Spear | Spear and pike lines, anti-cavalry | 3 to 4 |
| Heavy | Armoured and elite infantry | 4 to 5 |
| Ranged | Archers, crossbows, beams, acid | 3 to 4 |
| Cavalry | Riders and fast shock units | 5 |
| Air | Flying regiments | 3 |
| Artillery | Long-range area fire | 4 |
| Siege | Built to break structures | 4 |
| Support | Economy and aura units (Reclaimer, Caravan, Conductor) | 2 to 3 |
| Monster | One huge model, causes terror | 6 |
| Hero | Levels 1 to 10, four abilities | 5 |

Summoned units use no supply.

**Counter multipliers** (proposed): damage a regiment of the row class deals to the column class. Unlisted pairs are ×1.0. Every value sits between 0.7 and 1.5, so the "wrong" army can still win from a good position.

| Attacker ↓ / Target → | Light | Spear | Heavy | Ranged | Cavalry | Air | Artillery / Siege | Monster | Worker |
|---|---|---|---|---|---|---|---|---|---|
| Light | | | 0.8 | 1.2 | | | 1.3 | | 1.2 |
| Spear | | | 0.8 | | 1.5 | | | 1.2 | |
| Heavy | 1.4 | 1.2 | | | | | | 0.8 | |
| Ranged | 1.2 | 1.2 | 1.2 | | 0.8 | | | 1.3 | |
| Cavalry | | 0.7 | | 1.4 | | | 1.5 | | 1.3 |
| Air | | | | 1.2 | | | 1.5 | | 1.5 |
| Artillery | 1.3 | 1.3 | 1.3 | | 0.7 | | | | |
| Monster | 1.4 | 1.2 | 1.2 | 1.2 | | | | | |

Heroes take and deal ×1.0 against everything. Siege units deal ×3.0 to Fortified structures (a separate rule from these counters).

## Shared rules

- **Heroes** never rout. They cost 200 Grub and 50 Marrow, take 40 s to hire at the faction's hero building, and use 5 supply. Each has three basic abilities with three ranks (written 1/2/3 below) and an ultimate at level 6. Hero stats grow every level by the amounts listed.
- **Hero leadership** (proposed): friendly regiments within 16 tiles of any hero recover 1 extra morale a second and lose 25% less from casualties. A hero dying costs friendly regiments within 16 tiles 15 morale.
- **Workers** and Support units are not counted as regiments for the "6 to 8 regiment types" rule in 03.
- Every Brood regiment and worker also costs one larva.

---

## 1 · The Ember Clans

Disciplined spear lines, elite samurai, shock cavalry, fire from above. **Formation discipline** (signature): regiments standing still in tight formation get +15% melee defence.

### Regiments

| Unit | T | Class | Cost | Sup | Time | From | Models × HP | Files | Speed | Turn | Spacing | Atk / Def / Dmg | Chg | Mor | Ranged |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Hands | 1 | Worker | 50 / 0 | 1 | 12 | Clan Hall | 1 × 20 | 1 | 3.0 | 180 | 1.0 / 1.0 | 5 / 5 / 2 | 0 | 20 | |
| Yari Ashigaru | 1 | Spear | 120 / 0 | 4 | 22 | Dojo | 28 × 8 | 8 | 3.0 | 60 | 0.75 / 0.85 | 25 / 45 / 2 | 20 | 60 | |
| Yumi Ashigaru | 1 | Ranged | 130 / 0 | 3 | 22 | Dojo | 16 × 8 | 8 | 3.0 | 70 | 0.8 / 1.0 | 15 / 20 / 2 | 5 | 50 | 30, 4, 4.0 s, 32% |
| Battering Ram | 1 | Siege | 100 / 0 | 4 | 25 | Ember Workshop | 1 × 300 | 1 | 2.0 | 45 | 1.5 / 3.0 | 20 / 10 / 40 | 0 | 50 | |
| Katana Samurai | 2 | Heavy | 160 / 40 | 5 | 30 | Dojo | 16 × 14 | 8 | 3.0 | 75 | 0.8 / 0.9 | 55 / 45 / 5 | 30 | 80 | |
| Mounted Samurai | 2 | Cavalry | 180 / 40 | 5 | 32 | Stables | 8 × 30 | 4 | 6.5 | 140 | 1.4 / 2.0 | 45 / 30 / 6 | 80 | 70 | |
| Yabusame Riders (new) | 2 | Cavalry | 170 / 40 | 5 | 30 | Stables | 8 × 22 | 4 | 6.0 | 140 | 1.4 / 2.0 | 30 / 25 / 3 | 20 | 60 | 24, 4, 3.5 s, 30% |
| Ember Rocket Cart | 2 | Artillery | 150 / 60 | 4 | 35 | Ember Workshop | 4 × 25 | 4 | 2.0 | 40 | 2.5 / 2.5 | 5 / 10 / 2 | 0 | 45 | 60, 12, 10.0 s, 20% |
| War Kite | 2 | Air | 140 / 40 | 3 | 28 | Kite Loft | 4 × 18 | 4 | 7.0 | 120 | 2.0 / 2.0 | 20 / 15 / 2 | 0 | 50 | 6, 5, 6.0 s, 50% |
| Oni-Bound | 3 | Monster | 250 / 150 | 6 | 50 | Ancestor Shrine | 1 × 520 | 1 | 2.75 | 90 | 3.0 / 3.0 | 60 / 40 / 32 | 40 | 90 | |

**Summoned**

| Unit | Class | From | Models × HP | Files | Speed | Turn | Spacing | Atk / Def / Dmg | Chg | Mor | Ranged |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Spectral Samurai | Heavy | Ancestral Return | 16 × 12 | 8 | 3.25 | 80 | 0.8 / 0.9 | 55 / 45 / 5 | 30 | — | |
| Fire Spirit | Ranged | Fire Spirit (Onmyōji) | 1 × 120 | 1 | 4.0 | 180 | 1.0 / 1.0 | 30 / 30 / 6 | 0 | — | 18, 10, 2.5 s, 60% |

### Abilities

- **Hands**
  - **Gather** · passive — Carries 10 Grub or 5 Marrow per trip to a Clan Hall or Granary. [Gather]
  - **Build** · order — Builds and repairs Clan structures (repair 8 hit points a second). [Construct]
- **Yari Ashigaru** (existing in `units.ron`)
  - **Brace** · toggle — Holds position: cannot move, charges into its front get no charge bonus, +10 melee defence to the front. [ModifyStat] *Built so far (2026-10-16): the Brace order (Z) for every regiment with long weapons, as in [05 · Brace and Fall back](05-combat.md#brace-and-fall-back): a charge into its front is turned back on the charger; the +10 front defence is not built.*
  - **Formation Discipline** · passive — +15% melee defence while standing still in tight formation. [ModifyStat]
- **Yumi Ashigaru** (existing)
  - **Fire Arrows** · toggle — Volleys set structures burning (3 damage a second for 5 s) and deal +50% to structures; reload is 25% slower while on. [ModifyStat, ApplyStatus]
  - **Formation Discipline** · passive — As above. [ModifyStat]
  - *Flags: anti-air.*
- **Battering Ram**
  - **Siege Engine** · passive — Its blows only harm structures (×3.0 against Fortified). [ModifyStat]
  - **Covered Roof** · passive — Takes 50% less damage from ranged volleys. [ModifyStat]
  - *Flags: siege only.*
- **Katana Samurai**
  - **Kiai** · active · 45 s · radius 6 — A battle shout: enemy regiments in contact or within 6 tiles lose 12 morale; the samurai gain 5. [AreaPulse, ModifyMorale]
  - **Formation Discipline** · passive — As above. [ModifyStat]
- **Mounted Samurai**
  - **Ride Down** · passive — +50% damage to routing regiments. [ModifyStat]
  - **Wedge** · toggle — Wedge formation: +30 charge bonus, −10 melee defence. [ModifyStat]
- **Yabusame Riders** (new)
  - **Parthian Shot** · passive — Can loose volleys while moving, at 75% accuracy; skirmishes by default. [ModifyStat]
- **Ember Rocket Cart**
  - **Rocket Salvo** · passive — Each volley bursts over a 3-tile radius and can hit friends. [Damage, AreaPulse]
  - **Screaming Fire** · passive — Regiments hit lose 4 extra morale per salvo. [ModifyMorale]
  - *Flags: friendly fire. Cannot fire at targets closer than 10 tiles (proposed min range).*
- **War Kite**
  - **Fire Pots** · passive — Its drops set structures burning (3 damage a second for 5 s). [ApplyStatus]
  - **High Watch** · passive — Sees 18 tiles instead of the usual 12. [Reveal]
  - *Flags: flying, anti-air.*
- **Oni-Bound**
  - **Terror** · aura · radius 10 — Enemy regiments within 10 tiles lose 1 morale a second; a regiment it charges loses 10. [ModifyMorale]
  - **Bound Chains** · passive — With no Clan hero within 20 tiles for 20 s, it breaks its chains and attacks the nearest regiment of any side until a hero comes back within 10 tiles. [Control]
  - *Flags: terror.*
- **Spectral Samurai**
  - **Spectral** · passive — Lasts 60 s; never routs. [Summon]
- **Fire Spirit**
  - **Spirit Flame** · passive — Lasts 30 to 50 s (by Onmyōji rank); its bolts set structures burning. [ApplyStatus]

### Honor Edicts (meter 0 to 100)

| Edict | Honor | T | Cooldown | Effect |
|---|---|---|---|---|
| Rice Tax | 25 | 1 | 60 s | Instantly gain 300 Grub. [Economy] |
| Forced March | 20 | 1 | 45 s | Selected regiments +30% speed for 30 s. [ModifyStat] |
| Banner of the Ancestors | 35 | 2 | 90 s | Regiments within 12 tiles of the target point cannot rout for 20 s. [ApplyStatus] |
| Ancestral Return | 80 | 3 | 180 s | Summon a Spectral Samurai regiment at a Shrine or hero for 60 s. [Summon] |

*Built (2026-10-17, faction depth stage 5a):* Rice Tax, Forced March and Banner of the Ancestors. They are baked from the roster's `meter.abilities` into `data/powers` like the Resonance ones, and each built one carries its `built` value in `data/rosters/ember_clans.ron`: `Grant(300)`, `March(30)` or `Steadfast`. Forced March lasts 30 s (600 ticks) and Banner of the Ancestors 20 s (400 ticks). Each needs a finished Ancestor Shrine; a Banner that would steady none of the caster's regiments is refused, and a routing regiment loses its march. Ancestral Return is built in stage 5b, below.

*Built (2026-10-17, faction depth stage 5b):* Ancestral Return (`built: Some(Summon("Spectral Samurai"))`, baked as `POWER_SUMMON` with the unit's place in the Conquest unit table as its argument, which loading checks like a building index (the unit must be one nothing trains or evolves into, of the power's own faction); the baker also checks the unit is a `Summoned` one of the faction's own ("X is not a summoned unit"), and POWER_SCHEMA is 3). Its 60 s (1200 tick) duration is the Spectral Samurai's life. It is cast on one of your finished Shrines (any finished building of yours with the Honor flag) and puts a full regiment beside it for 80 Honor; it takes no supply, never routs and is gone when its time is up, nobody left locked with it. A hero as the place to cast it waits for heroes (stage 9). **Formation Discipline** (the passive on the Yari Ashigaru, Yumi Ashigaru and Katana Samurai) is built too: the baker gives any roster unit with an ability of that name the discipline weapon bit (`WEAPON_DISCIPLINE`, 8; UNIT_SCHEMA 7, CONQUEST_UNIT_SCHEMA 4), and `data/units.ron` says `discipline: true` on the live copies (a test checks they agree). See [03 · Signature rule](03-factions.md#signature-rule-formation-discipline) and [05 · Weapons and formations](05-combat.md#weapons-and-formations).

### Heroes

| Hero | Role | HP (+/lvl) | Speed | Turn | Spacing | Atk / Def / Dmg (+/lvl) | Chg | Ranged (dmg +/lvl) |
|---|---|---|---|---|---|---|---|---|
| The Daimyo | Commander, mounted | 340 (+36) | 5.5 | 180 | 1.4 / 2.0 | 55 / 50 / 18 (+2) | 40 | |
| The Ronin | Duelist | 300 (+32) | 4.25 | 240 | 1.0 / 1.0 | 65 / 45 / 22 (+3) | 20 | |
| The Onmyōji | Mystic | 260 (+26) | 3.5 | 200 | 1.0 / 1.0 | 35 / 35 / 10 (+1) | 0 | 22, 14 (+2), 2.5 s, 70% |

- **The Daimyo** (hired at the Ancestor Shrine)
  - **Rally** · active · 40 s · radius 12 — Routing regiments in the area rally at once; all regiments there gain 15/25/35 morale. [ModifyMorale]
  - **Command Aura** · aura · radius 12 — Friendly regiments +5/10/15 melee defence. [ModifyStat]
  - **Challenge** · active · 60 s · range 8 — Locks an enemy hero in a duel for 6/8/10 s; nobody else can hit either of them. The winner's side gains 20 Honor. [ApplyStatus, Control]
  - **Shogun's Banner** · ultimate · 120 s · 20 s — Every Clan regiment on the map: morale tops up by 25, +20% speed, cannot rout. [ModifyMorale, ModifyStat, ApplyStatus]
- **The Ronin**
  - **Iaijutsu Dash** · active · 14 s · range 10 — Blinks through a line of enemies, dealing 60/100/140 damage spread across the models crossed. [Teleport, Damage]
  - **Wind Cut** · active · 10 s · range 6 — 90° cone, 40/70/100 damage to each regiment in it. [Damage]
  - **Wanderer** · passive — Regains 2/4/6 hit points a second with no friendly regiment within 12 tiles. [Heal]
  - **Thousand Cuts** · ultimate · 90 s · radius 4 · 3 s — Untouchable while it strikes 8 times for 45 damage among enemies within 4 tiles. [Damage, ApplyStatus]
- **The Onmyōji**
  - **Fire Spirit** · active · 40 s — Summons a Fire Spirit for 30/40/50 s. [Summon]
  - **Ward Talisman** · active · 35 s · range 20 · radius 5 — A ward that destroys enemy volleys landing inside it, for 8/10/12 s. [ApplyStatus]
  - **Divination** · active · 30 s · 10 s — Reveals a radius of 12/16/20 tiles anywhere on the explored map, including burrowed and hidden units. [Reveal]
  - **Kitsune Storm** · ultimate · 120 s · range 24 · radius 8 · 10 s — Fox-fire storm: 20 damage a second to every enemy regiment inside, which also loses 3 morale a second. [Damage, ModifyMorale, AreaPulse]

---

## 2 · The Brood

A swarm that changes shape mid-battle and feeds on the dead. **Metamorphosis** (signature): a tier-1 regiment cocoons for 15 s on the battlefield (it cannot move or fight, and takes +25% damage) and emerges as one of its two tier-2 forms, paying the evolution cost below. Survivors carry over in proportion (a half-strength Skitterlings regiment becomes a half-strength Spitters regiment), and veterancy is kept. Evolving needs a Chrysalis Chamber. **Swarm instinct**: Brood regiments lose 30% less morale from casualties but 50% more when a nearby hero dies.

### Regiments

| Unit | T | Class | Cost | Sup | Time | From | Models × HP | Files | Speed | Turn | Spacing | Atk / Def / Dmg | Chg | Mor | Ranged |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Gnawer | 1 | Worker | 50 / 0 | 1 | 12 | Hive | 1 × 18 | 1 | 3.25 | 180 | 1.0 / 1.0 | 5 / 5 / 2 | 0 | 15 | |
| Skitterlings | 1 | Light | 100 / 0 | 3 | 18 | Hive | 32 × 6 | 8 | 4.0 | 120 | 0.8 / 0.8 | 50 / 26 / 3 | 40 | 64 | |
| Chitin Guard | 1 | Heavy | 130 / 0 | 4 | 22 | Hive | 12 × 16 | 6 | 2.75 | 70 | 0.9 / 1.0 | 30 / 55 / 4 | 15 | 60 | |
| Reclaimer | 1 | Support | 75 / 0 | 2 | 18 | Hive | 1 × 60 | 1 | 2.5 | 120 | 1.5 / 1.5 | 10 / 20 / 3 | 0 | 40 | |
| Spitters | 2 | Ranged | 60 / 20 | 3 | 15 | Skitterlings | 16 × 7 | 8 | 3.5 | 100 | 0.8 / 0.9 | 20 / 15 / 2 | 5 | 45 | 22, 3, 3.0 s, 35% |
| Burrowers | 2 | Light | 70 / 20 | 3 | 15 | Skitterlings | 16 × 9 | 8 | 4.0 | 110 | 0.8 / 0.8 | 45 / 25 / 3 | 50 | 50 | |
| Mantid Reapers | 2 | Spear | 80 / 30 | 4 | 15 | Chitin Guard | 12 × 15 | 6 | 3.25 | 90 | 1.0 / 1.1 | 55 / 35 / 6 | 25 | 65 | |
| Shellbacks | 2 | Heavy | 90 / 30 | 5 | 15 | Chitin Guard | 8 × 30 | 4 | 1.5 | 40 | 1.3 / 1.3 | 25 / 70 / 4 | 0 | 75 | 18, 3, 3.0 s, 40% |
| Drone-Moths | 2 | Air | 120 / 30 | 3 | 25 | Moth Roost | 6 × 14 | 3 | 7.5 | 150 | 1.6 / 1.6 | 20 / 15 / 2 | 0 | 40 | 5, 4, 8.0 s, 50% |
| Broodmother Bulk | 2 | Artillery | 160 / 60 | 4 | 35 | Gorge | 1 × 380 | 1 | 1.75 | 45 | 3.0 / 3.0 | 15 / 25 / 8 | 0 | 55 | 45, 10, 12.0 s, 30% |
| Colossus Maw | 3 | Monster | 260 / 140 | 6 | 55 | Gorge | 1 × 600 | 1 | 2.5 | 70 | 3.5 / 3.5 | 55 / 35 / 28 | 50 | 95 | |

Brood regiments are hatched from larvae at a Hive or Hatchery Pit; "Hive" above means either.

**Summoned**

| Unit | Class | From | Models × HP | Files | Speed | Turn | Spacing | Atk / Def / Dmg | Chg | Mor | Ranged |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Spawnlings | Light | Larva Sac, Spawn Brood, Swarm Tide | 8 × 5 | 4 | 4.0 | 120 | 0.8 / 0.8 | 35 / 15 / 2 | 30 | — | |

### Abilities

- **Gnawer**
  - **Burrow In** · order — Burrows into a seep (up to 5 per seep) and delivers Grub through the vein network; burrowed Gnawers are hidden and cannot be harmed except by Titan Events. [Burrow, Gather]
  - **Grow Structure** · order — The Gnawer is consumed to grow a Brood structure. [Construct]
- **Skitterlings** (existing)
  - **Swarm Instinct** · passive — Brood regiments lose 30% less morale from casualties but 50% more when a nearby hero dies. [ModifyMorale]
  - **Metamorphosis** · order — Evolves into Spitters or Burrowers. [Transform]
- **Chitin Guard**
  - **Hunker** · toggle — +20 melee defence, −50% speed. [ModifyStat]
  - **Metamorphosis** · order — Evolves into Mantid Reapers or Shellbacks. [Transform]
- **Reclaimer**
  - **Digest** · autocast — Eats a corpse every 0.5 s within 2 tiles, 2 Grub each. [Economy, RaiseCorpses]
  - **Mending Ooze** · aura · radius 6 — Friendly regiments out of combat regain 1 hit point per model every 5 s. [Heal]
- **Spitters**
  - **Acid** · passive — Regiments hit lose 10 melee defence for 8 s (stacks twice). [ApplyStatus]
  - *Flags: anti-air.*
- **Burrowers**
  - **Tunnel** · toggle — Burrowed: hidden, crosses cliffs and rib-ridges, speed 2.5, cannot fight. [Burrow]
  - **Ambush** · passive — For 3 s after surfacing, its blows count as rear hits. [ModifyStat]
  - *Flags: burrows.*
- **Mantid Reapers**
  - **Scything Arms** · passive — +30% damage to Monsters (on top of the Spear class bonus against Cavalry). [ModifyStat]
- **Shellbacks**
  - **Root** · toggle — Cannot move; +25 melee defence; spine range +6. [ModifyStat]
  - **Spines** · passive — Its volleys can only target air regiments. [ModifyStat]
  - *Flags: anti-air.*
- **Drone-Moths**
  - **Spore Bomb** · passive — Regiments hit get −15% ranged accuracy for 6 s. [ApplyStatus]
  - **Antennae** · aura · radius 10 — Detects burrowed and hidden units. [Reveal]
  - *Flags: flying, detector.*
- **Broodmother Bulk**
  - **Larva Sac** · passive — Each sac bursts over a 2-tile radius (can hit friends) and hatches a Spawnlings regiment at the impact point for 20 s. [SpawnVolley, Summon, AreaPulse]
  - *Flags: friendly fire.*
- **Colossus Maw**
  - **Devour** · active · 20 s — Swallows 3 models of a non-monster regiment it is fighting, healing 40 hit points per model. [Damage, Heal]
  - **Terror** · aura · radius 10 — Enemy regiments within 10 tiles lose 1 morale a second; a regiment it charges loses 10. [ModifyMorale]
  - *Flags: terror.*
- **Spawnlings**
  - **Short-lived** · passive — Lasts 20 to 30 s; never routs. [Summon]

### Larvae (meter)

Larvae are not spent on abilities. Each Hive and Hatchery Pit holds up to 3 larvae and grows one every 12 s; every Gnawer and regiment hatches from one. The meter shows larvae ready across all hives (the data file caps it at 15 for display).

*Built (2026-10-17, faction depth stage 1):* every Brood unit, workers included, takes a larva when it is queued, and training is refused with "No larvae are ready." while none is. The top bar shows larvae ready and room, and a Hive's card shows its larvae as n/3. In `data/rosters/brood.ron` a building that holds larvae has the `Larvae` flag, a unit that hatches from one has `Hatches`, and the Gnawer, used up to grow buildings, has `Consumed`; the simulation reads only these flags, never the faction. Since stage 2 the Hive and Pustule have `Veins` (they spread root-veins, as far as their `field` in tiles) and the Gnawer has `Burrows` (a worker with it stays at its node and sends loads home through the veins). Since stage 3 the Choir's Spire, Resonator and Siphon power a field as far as their `field`, the Resonator and Siphon have `tunes` (the seeps or veins they work and how far), the Choir's other buildings have `NeedsPower`, and the Tuner has `BuildsOnly` (it builds and mends but never gathers). Since stage 4 the Resonator and Siphon have `links: 14` (two link when their centres are that close, and three mutually linked make a chord).

### Heroes

| Hero | Role | HP (+/lvl) | Speed | Turn | Spacing | Atk / Def / Dmg (+/lvl) | Chg | Ranged (dmg +/lvl) |
|---|---|---|---|---|---|---|---|---|
| The Queen-in-Waiting | Mother, support | 300 (+30) | 3.5 | 160 | 1.6 / 1.6 | 40 / 45 / 14 (+2) | 0 | |
| The Devourer | Bruiser | 420 (+45) | 4.0 | 180 | 1.6 / 1.6 | 60 / 45 / 24 (+3) | 30 | |
| The Whisperer | Controller | 280 (+28) | 3.5 | 200 | 1.0 / 1.0 | 35 / 40 / 12 (+1) | 0 | |

- **The Queen-in-Waiting** (hired at the Queen's Chamber)
  - **Spawn Brood** · active · 40 s — Hatches 1/2/3 Spawnlings regiments beside her for 30 s. [Summon]
  - **Royal Jelly** · active · 25 s · range 10 — Heals a friendly regiment 60/100/140 hit points, spread across its models. [Heal]
  - **Pheromone Trail** · active · 30 s · radius 10 · 8 s — Friendly regiments +15/25/35% speed. [ModifyStat]
  - **Swarm Tide** · ultimate · 120 s · radius 20 · 15 s — Brood regiments in range +20 melee attack and lose no morale from casualties; hatches 4 Spawnlings regiments. [ModifyStat, ModifyMorale, Summon]
- **The Devourer**
  - **Gorge** · active · 12 s — Eats one model of a regiment it is fighting and heals 80/120/160. [Damage, Heal]
  - **Grow** · passive — +4/6/8 hit points and slightly larger per model killed, up to 30 kills. [ModifyStat]
  - **Burrow Charge** · active · 20 s · range 16 · radius 3 — Tunnels to a point and erupts for 60/90/120 damage, counting as a charge. [Burrow, Teleport, Damage]
  - **Maw of the Titan** · ultimate · 120 s · range 16 · radius 5 · 3 s — The ground opens: kills up to 12 non-monster models in the area and deals 200 damage to monsters and heroes. [Damage]
- **The Whisperer**
  - **Mind-Spore** · active · 60 s · range 12 — Takes control of a creep regiment or an enemy regiment of at most 6/10/14 models for 20/25/30 s. [Control]
  - **Confusion Cloud** · active · 30 s · range 16 · radius 5 — Enemy regiments inside ignore orders and attack the nearest regiment of any side for 4/5/6 s. [Control, ApplyStatus]
  - **Hive Sight** · passive — Every Pustule reveals a radius of 10/14/18 tiles. [Reveal]
  - **Puppet Army** · ultimate · 150 s · radius 20 · 20 s — Every routing enemy regiment in range fights for you. [Control]

---

## 3 · The Choir

Few, costly crystal soldiers who blink across the map and bend light through each other. **Refraction** (signature): a beam volley that passes through an allied Crystal regiment splits into two volleys of 75% damage aimed at the two nearest enemies past it. All Choir units are Crystal.

### Regiments

| Unit | T | Class | Cost | Sup | Time | From | Models × HP | Files | Speed | Turn | Spacing | Atk / Def / Dmg | Chg | Mor | Ranged |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Tuner | 1 | Worker | 60 / 0 | 1 | 14 | Spire | 1 × 30 | 1 | 3.0 | 180 | 1.0 / 1.0 | 5 / 10 / 2 | 0 | 30 | |
| Shardlings | 1 | Light | 110 / 0 | 3 | 20 | Forge of Facets | 14 × 12 | 6 | 3.25 | 80 | 0.6 / 0.9 | 40 / 40 / 4 | 30 | 56 | |
| Prism Lancers | 1 | Ranged | 140 / 0 | 4 | 24 | Forge of Facets | 10 × 8 | 5 | 3.0 | 70 | 0.9 / 1.0 | 15 / 25 / 2 | 0 | 55 | 26, 6, 4.5 s, 45% |
| Harmonic Walkers | 2 | Heavy | 170 / 50 | 5 | 32 | Forge of Facets | 6 × 32 | 3 | 2.75 | 50 | 1.6 / 1.8 | 40 / 50 / 7 | 20 | 70 | |
| Glass Striders (new) | 2 | Cavalry | 175 / 45 | 5 | 32 | Forge of Facets | 6 × 26 | 3 | 7.0 | 150 | 1.4 / 2.0 | 45 / 30 / 6 | 70 | 60 | |
| Skyglass | 2 | Air | 130 / 40 | 3 | 26 | Skyglass Loom | 4 × 16 | 4 | 8.0 | 160 | 1.8 / 1.8 | 25 / 20 / 3 | 0 | 50 | 14, 5, 2.5 s, 50% |
| Conductor | 2 | Support | 120 / 60 | 3 | 30 | Spire | 1 × 140 | 1 | 3.0 | 120 | 1.5 / 1.5 | 20 / 30 / 4 | 0 | 60 | |
| Monolith | 3 | Monster | 280 / 160 | 6 | 60 | Forge of Facets | 1 × 650 | 1 | 1.75 | 30 | 4.0 / 4.0 | 35 / 40 / 20 | 0 | 100 | 32, 40, 8.0 s, 70% |

The Choir has no summoned units.

### Abilities

- **Tuner**
  - **Construct** · order — Builds Choir structures inside a Resonator's field and repairs them (10 hit points a second). Tuners do not gather. [Construct]
- **Shardlings** (existing)
  - **Shatter** · passive — Each model that dies bursts for 3 damage to enemies within 1.5 tiles. [Damage, AreaPulse]
- **Prism Lancers**
  - **Refraction** · passive — A beam volley that passes through an allied Crystal regiment splits into two volleys of 75% damage aimed at the two nearest enemies past it. [SpawnVolley]
  - *Flags: direct fire (needs line of sight), anti-air.*
- **Harmonic Walkers**
  - **Sonic Pulse** · active · 25 s · range 8 — Cone in front: enemy regiments are knocked back 3 tiles, lose 6 morale and suffer −20 melee defence for 5 s while they re-form. [Knockback, ModifyMorale, ApplyStatus]
- **Glass Striders** (new)
  - **Phase Charge** · active · 30 s · range 10 — Blinks straight into contact with an enemy regiment, counting as a full-speed charge. [Teleport]
- **Skyglass**
  - **Glint** · passive — +50% damage to air regiments. [ModifyStat]
  - *Flags: flying, anti-air, direct fire.*
- **Conductor**
  - **Mobile Resonator** · toggle — Anchors in 3 s; while anchored it cannot move and counts as a Resonator (links chords, powers structures, tunes a seep within 4 tiles). [Construct, Economy]
  - **Tuning Fork** · aura · radius 8 — Crystal regiments regain 1 hit point per model every 4 s. [Heal]
- **Monolith**
  - **Sonic Beam** · passive — ×3.0 damage to structures. [ModifyStat]
  - **Terror** · aura · radius 10 — Enemy regiments within 10 tiles lose 1 morale a second; a regiment it charges loses 10. [ModifyMorale]
  - *Flags: terror, direct fire.*

### Resonance abilities (meter 0 to 100)

| Ability | Resonance | T | Cooldown | Effect |
|---|---|---|---|---|
| Overclock | 20 | 1 | 20 s | A structure or regiment works, reloads and strikes 50% faster for 20 s. [ModifyStat] |
| Phase Shift | 60 | 2 | 90 s | Teleport selected regiments to any of your Resonators. [Teleport] |
| Silence the Chord | 80 | 3 | 150 s | Enemy abilities and faction meters within 12 tiles of a point freeze for 15 s. [ApplyStatus] |

*Built (2026-10-17, faction depth stage 4b):* Overclock and Phase Shift. The meter abilities are baked into a powers table (`data/powers`, from each roster's `meter.abilities`); one the game has built says what the simulation does with its `built` field (`Haste(percent)`, faster for its duration; `Shift("Building")`, the building regiments step to; and from stage 5a `Grant(grub)`, `March(percent)` or `Steadfast`), and the rest are listed but not offered. **Overclock** (20 Resonance, tier 1, 20 s cooldown): one of your buildings trains, upgrades, is built and tunes 50% faster for 20 s, or one of your regiments reloads, strikes (regiments and buildings) and builds 50% faster, its extra blows falling halfway between its own; using it again on the same target restarts it. **Phase Shift** (60 Resonance, tier 2, 90 s cooldown): your chosen regiments (not routing ones) step straight out of any fight (their foes let them go) to one of your finished Resonators, round it in up to five rings of eight; every regiment shifted to the same Resonator within a second of the payment rides on it, so a big order the network carries over a few turns is still one use. Cooldowns and hastes last as long at every tempo. Silence the Chord is not built yet. On screen a Choir player has a button for each (keys F2 and F3) under the top bar, showing its cost or what holds it back (tier, Resonance, cooldown, or no regiments selected for a shift); pressing one aims it, a left click on a valid target of yours uses it, and Esc or a right click puts it away. Hasted buildings and regiments pulse with a cyan outline.

### Heroes

| Hero | Role | HP (+/lvl) | Speed | Turn | Spacing | Atk / Def / Dmg (+/lvl) | Chg | Ranged (dmg +/lvl) |
|---|---|---|---|---|---|---|---|---|
| The Maestro | Economy, support | 300 (+30) | 3.5 | 180 | 1.2 / 1.2 | 40 / 45 / 14 (+2) | 0 | |
| The Refractor | Damage | 320 (+32) | 3.75 | 200 | 1.2 / 1.2 | 45 / 40 / 10 (+1) | 0 | 26, 18 (+2), 2.0 s, 80% |
| The Hollow Bell | Control | 360 (+38) | 3.5 | 160 | 1.4 / 1.4 | 45 / 50 / 16 (+2) | 0 | |

- **The Maestro** (hired at the Sanctum of Echoes)
  - **Crescendo** · active · 35 s · radius 12 · 10 s — Friendly regiments reload 15/25/35% faster and gain +10/15/20 melee attack. [ModifyStat]
  - **Retune** · active · 60/45/30 s · range 20 — Moves one of your Resonators instantly to a point in range. [Teleport]
  - **Harmony** · passive · radius 20 — Chords near the Maestro produce +10/20/30% Grub. [Economy]
  - **Symphony** · ultimate · 180 s · 30 s — All your chords +100% Grub, and Resonance charges twice as fast. [Economy]
- **The Refractor**
  - **Split Beam** · active · 18 s — Its next 3 volleys split to 2/3/4 targets. [SpawnVolley]
  - **Mirror Shield** · active · 30 s · range 12 · 6 s — A friendly regiment reflects 30/45/60% of incoming volleys back at the shooters. [ApplyStatus, SpawnVolley]
  - **Lens** · active · 40 s · range 12 — A friendly non-Crystal regiment refracts beams for 15/20/25 s. [ApplyStatus]
  - **Prismatic Lance** · ultimate · 120 s · range 40 — A 2-tile-wide line: 250 damage split across everything in it, ×1.5 against structures. [Damage]
  - *Flags: direct fire, anti-air.*
- **The Hollow Bell**
  - **Toll** · active · 20 s · range 10 — Stuns a regiment (no moving or striking) for 1.5/2.5/3.5 s. [Stun]
  - **Silence** · active · 30 s · range 14 · radius 6 — Abilities and auras inside switch off for 4/6/8 s. [ApplyStatus]
  - **Echo Step** · active · 15 s — Blinks 10/14/18 tiles. [Teleport]
  - **Deafening Peal** · ultimate · 150 s · radius 14 — Every enemy regiment in range loses 30 morale at once. [ModifyMorale, AreaPulse]

---

## 4 · The Carrion Concord

Merchant-necromancers: cheap undead that never break, beside living specialists that do. **The dead don't rout** (signature): undead regiments ignore morale. Instead they **crumble**: more than 24 tiles from a Reliquary they lose one model every 10 s, and they can only be reinforced with Tithe, never Grub.

### Regiments

| Unit | T | Class | Cost | Sup | Time | From | Models × HP | Files | Speed | Turn | Spacing | Atk / Def / Dmg | Chg | Mor | Ranged |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Ledger Clerk | 1 | Worker | 50 / 0 | 1 | 12 | Counting House | 1 × 20 | 1 | 3.0 | 180 | 1.0 / 1.0 | 5 / 5 / 2 | 0 | 25 | |
| Caravan | 1 | Support | 75 / 0 | 2 | 20 | Counting House | 1 × 220 | 1 | 2.25 | 45 | 2.5 / 3.5 | 0 / 10 / 1 | 0 | 30 | |
| Bone Pikes | 1 | Spear | 90 / 0 | 3 | 18 | Ossuary | 19 × 7 | 10 | 2.75 | 45 | 0.7 / 0.9 | 28 / 52 / 2 | 14 | — | |
| Crossbow Auditors | 1 | Ranged | 140 / 0 | 4 | 24 | Guildhall | 12 × 9 | 6 | 3.0 | 70 | 0.8 / 1.0 | 20 / 25 / 2 | 0 | 55 | 26, 7, 6.0 s, 40% |
| Gravewardens (new) | 2 | Heavy | 170 / 40 | 5 | 30 | Ossuary | 12 × 18 | 6 | 2.5 | 55 | 0.9 / 1.0 | 45 / 50 / 6 | 15 | — | |
| Debt Collectors | 2 | Cavalry | 190 / 50 | 5 | 34 | Guildhall | 6 × 34 | 3 | 6.0 | 120 | 1.5 / 2.1 | 45 / 40 / 7 | 70 | 65 | |
| Lantern Wraiths | 2 | Air | 140 / 40 | 3 | 28 | Lantern Spire | 4 × 16 | 4 | 6.5 | 140 | 1.8 / 1.8 | 30 / 25 / 3 | 0 | — | 8, 3, 3.0 s, 40% |
| Mausoleum Engine | 3 | Siege | 240 / 120 | 4 | 50 | Ossuary | 1 × 500 | 1 | 1.5 | 30 | 3.5 / 4.0 | 20 / 40 / 30 | 0 | — | 40, 14, 9.0 s, 30% |
| The Insolvent | 3 | Monster | 260 / 150 | 6 | 55 | Ossuary | 1 × 640 | 1 | 2.5 | 60 | 3.5 / 3.5 | 50 / 35 / 30 | 40 | — | |

**Summoned**

| Unit | Class | From | Models × HP | Files | Speed | Turn | Spacing | Atk / Def / Dmg | Chg | Mor | Ranged |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Bound Dead | Light | Raise the Bound, Indenture, Raise Dead, Interment, Mass Grave | 16 × 7 | 8 | 2.5 | 50 | 0.6 / 0.9 | 20 / 30 / 2 | 5 | — | |
| Grubfolk Militia | Spear | Hire Militia | 16 × 8 | 8 | 3.0 | 70 | 0.75 / 0.9 | 25 / 35 / 3 | 10 | 45 | |

Grubfolk Militia are neutral hirelings; until the data has a neutral faction they are listed under the Concord.

### Abilities

- **Ledger Clerk**
  - **Mine** · passive — Carries 5 Marrow per trip to a Counting House. [Gather]
  - **Build** · order — Builds and repairs Concord structures (8 hit points a second). [Construct]
- **Caravan**
  - **Trade Run** · autocast — Shuttles between the Counting House and a trade post; each delivery pays 40 Grub plus 1.2 per tile of route (full rate at a hamlet or an ally, half at a Waystation). [Economy]
  - **Strongbox** · passive — When destroyed it drops half its cargo as a Grub pile anyone can collect. [Economy]
- **Bone Pikes** (existing)
  - **Undead** · passive — Never routs; crumbles away from Reliquaries. [ModifyMorale]
  - **Pike Wall** · toggle — As Brace (see Yari Ashigaru). [ModifyStat]
  - *Flags: undead.*
- **Crossbow Auditors**
  - **Audit** · passive — Bolts pierce armour: +30% damage to Heavy and Cavalry. [ModifyStat]
  - *Flags: direct fire, anti-air.*
- **Gravewardens** (new)
  - **Unfeeling** · passive — Flank and rear hits only deal ×1.2 and ×1.4 damage (not ×1.4 and ×1.8). [ModifyStat]
  - **Undead** · passive — As above. [ModifyMorale]
  - *Flags: undead.*
- **Debt Collectors**
  - **Repossess** · passive — Every 2 damage dealt to a structure steals 1 Grub. [Economy]
  - *Living riders on undead horses: they rout.*
- **Lantern Wraiths**
  - **Dread Light** · aura · radius 8 — Enemy regiments lose 1.5 morale a second. [ModifyMorale]
  - **Undead** · passive — As above. [ModifyMorale]
  - *Flags: flying, undead.*
- **Mausoleum Engine**
  - **Interment** · autocast · 15 s · radius 10 — Raises up to 8 corpses nearby as a Bound Dead regiment for 30 s. [RaiseCorpses, Summon]
  - **Siege Engine** · passive — ×3.0 damage to Fortified structures. [ModifyStat]
  - *Flags: undead.*
- **The Insolvent**
  - **Stitched Together** · passive — Regains 10 hit points for each corpse it walks over. [Heal, RaiseCorpses]
  - **Terror** · aura · radius 10 — Enemy regiments within 10 tiles lose 1 morale a second; a regiment it charges loses 10. [ModifyMorale]
  - *Flags: undead, terror.*
- **Bound Dead**
  - **Undead** · passive — Never routs; temporary versions last 30 to 90 s, Indentured ones crumble instead. [Summon]
- **Grubfolk Militia**
  - **Hireling** · passive — Leaves after 120 s, or at once if the hamlet that supplied it is razed. [Summon]

### Tithe contracts (meter 0 to 250)

| Contract | Tithe | T | Cooldown | Effect |
|---|---|---|---|---|
| Hire Militia | 30 | 1 | 30 s | Hire a Grubfolk Militia regiment from a friendly hamlet for 120 s. [Summon] |
| Raise the Bound | 40 | 1 | 30 s | Turn corpses within 8 tiles of a point into a Bound Dead regiment for 90 s. [RaiseCorpses, Summon] |
| Foreclose | 60 | 2 | 120 s | Take control of a neutral creep camp. [Control] |
| Indenture | 100 | 2 | 60 s | Raise a permanent Bound Dead regiment from corpses; it uses no supply but crumbles away from Reliquaries. [RaiseCorpses, Summon] |

### Heroes

| Hero | Role | HP (+/lvl) | Speed | Turn | Spacing | Atk / Def / Dmg (+/lvl) | Chg | Ranged (dmg +/lvl) |
|---|---|---|---|---|---|---|---|---|
| The Magnate | Economy | 300 (+30) | 3.5 | 160 | 1.4 / 1.4 | 35 / 40 / 12 (+1) | 0 | |
| The Undertaker | Necromancy | 320 (+32) | 3.25 | 180 | 1.0 / 1.0 | 40 / 40 / 14 (+2) | 0 | |
| The Usurer | Duelist, debuff | 340 (+36) | 4.0 | 220 | 1.0 / 1.0 | 55 / 45 / 20 (+3) | 0 | |

- **The Magnate** (hired at the Vault of Names)
  - **Hostile Takeover** · active · 180 s · range 8 — A hamlet becomes your exclusive trade post for 120/180/240 s. [Economy, Control]
  - **Bribe** · active · 60 s · range 10 — Pays 100/75/50 Grub to pacify a creep camp for 60/90/120 s. [Control]
  - **Compound Interest** · passive — Earns 1/2/3% of banked Grub every minute, at most 15/30/45. [Economy]
  - **Market Crash** · ultimate · 180 s · 30 s — Every enemy's income −50%. [Economy]
- **The Undertaker**
  - **Raise Dead** · active · 40 s · radius 8 — Raises a Bound Dead regiment of 6/10/14 models from nearby corpses for 45 s. [RaiseCorpses, Summon]
  - **Corpse Explosion** · active · 20 s · radius 10 — Up to 4/6/8 corpses burst for 30 damage within 2 tiles each. [Damage, AreaPulse]
  - **Last Rites** · active · 30 s · range 10 — An undead regiment heals 80/130/180 and regains 2/3/4 lost models. [Heal]
  - **Mass Grave** · ultimate · 150 s · radius 10 · 15 s — Every model that dies in the area rises at once as Bound Dead on your side. [RaiseCorpses, Summon]
- **The Usurer**
  - **Leech Debt** · active · 15 s · range 8 · 3 s — Drains 15/25/35 hit points a second from a target into the Usurer. [Damage, Heal]
  - **Lien** · active · 20 s · range 12 · 10 s — A marked regiment takes +15/20/25% damage. [ApplyStatus]
  - **Collateral** · active · 25/20/15 s · range 10 — Steals one beneficial effect from an enemy regiment or hero. [ApplyStatus]
  - **Foreclosure** · ultimate · 120 s · range 10 — An enemy regiment below half strength instantly loses its weakest third of models. [Damage]

---

## Balance notes

A few figures to sanity-check the first pass (front rank fighting a defence-40 enemy head on, in Line, no charge, steady morale; spears and pikes count half their second rank):

| Unit | Total HP | Melee damage a second | Grub-equivalent cost* | HP per 100 Grub |
|---|---|---|---|---|
| Yari Ashigaru | 224 | 8.9 | 120 | 187 |
| Skitterlings | 192 | 14.4 | 100 | 192 |
| Shardlings | 168 | 12.0 | 110 | 153 |
| Bone Pikes | 133 | 11.0 | 90 | 148 |
| Katana Samurai | 224 | 26.0 | 240 | 93 |
| Chitin Guard | 192 | 9.6 | 130 | 148 |
| Gravewardens | 216 | 19.8 | 250 | 86 |
| Harmonic Walkers | 192 | 10.5 | 270 | 71 |
| Mounted Samurai | 240 | 13.2 | 260 | 92 |
| Oni-Bound | 520 | 22.4 | 550 | 95 |

\* Marrow counted at 2 Grub each.

What these show, and what playtesting should watch:

- Tier-1 regiments trade efficiency for numbers: they are the cheapest hit points in the game. The Skitterlings are a big, fast swarm that hits hard on the charge but has little defence; the Shardlings are few and tough; the Bone Pikes have the fewest hit points but the best defence and never break. The Choir's high cost per hit point is deliberate (few, precious units) and leans on Refraction and Phase Shift to pay off.
- Elites hit much harder per regiment (Katana Samurai deal almost three times what a Yari line does) at about the same damage per Grub. You pay for concentrated punch and steadier morale, not for efficiency.
- A monster holds its own against about two tier-1 regiments at once (the Oni-Bound against two Yari lines is close to even, before Terror). Focused ranged fire and heroes are the intended answer.
- Every number here is a starting point for the M3 and M5 balance passes, not a final value.

### Field Battle balance

A Field Battle army is ten regiments from the faction's roster (see [06 · Game modes](06-game-modes.md)), so today the five live regiments are balanced per regiment, not per Grub; costs were left alone for the M3 pass. With the first-pass stats the Ember Clans won every matchup against the Brood and the Choir, so the October 2026 pass changed:

| Unit | Before | After |
|---|---|---|
| Yari Ashigaru | 24 × 10 HP | 28 × 8 HP (less total HP) |
| Skitterlings | 24 × 6, 6 files, 40 / 20 / 2, Mor 45 | 32 × 6, 8 files, 50 / 25 / 3, Mor 57 (a bigger, harder-hitting swarm) |
| Shardlings | 12 × 9, spacing 0.7, 35 / 35 / 4, Mor 55 | 14 × 12, spacing 0.55, 40 / 40 / 4, Mor 60 (tougher; the tighter block takes more arrows) |
| Bone Pikes | 20 × 8, speed 2.5, spacing 0.55, Def 50 | 19 × 7, speed 2.75, spacing 0.7, Def 55 (fewer hit points, harder to hit, looser block, a little quicker) |
| Yumi Ashigaru | unchanged | |

Two Veteran AIs, one per faction, on the generated map with every rule in play (high ground and fords, [05 · Combat](05-combat.md); woods and ambushes, [20 · Ambushes](20-ambushes.md); pushing through friends; lines that push; deployment; champions allowed, the lobby default), 384 battles a pair (seeds 1 to 192 and 2001 to 2192, ends swapped every battle); the first faction's wins, after the [third retune](#third-retune) below:

| | Brood | Choir | Carrion Concord |
|---|---|---|---|
| **Ember Clans** | 50% | 52% | 50% |
| **Brood** | | 44% | 52% |
| **Choir** | | | 49.5% |

**Weapon room retune (2026-10-15).** Reach gave the spear regiments half their second rank as extra blows, so the Yari Ashigaru went from 30 / 45 / 3 to 27 / 45 / 2 and the Bone Pikes from 25 / 55 / 3 to 28 / 54 / 2. Before the retune the table read 50%, 47%, 61% / 45%, 53% / 53%, and after it 59%, 42%, 53% / 45%, 54% / 56%. On seeds 1 to 96 it now reads 60%, 51%, 52% / 36%, 52% / 54% (the Brood against the Choir was 36% there before the retune too). These knobs are steep too: a Bone Pikes defence of 53 or 55 instead of 54 moved the Concord's matchups by 10 to 20 points.

Seeds 1 to 96 gave every pair between 48% and 57% except the Brood against the Choir (36%). Once the terrain rules arrived the Concord fell behind the Clans (66%), so the Bone Pikes got a little faster (2.6 to 2.75). One Yumi fewer evens the Clans and the Concord further, but then a Veteran in the Clans mirror on one test seed beats the army in front of it and never marches on the survivors who rallied and scattered (an AI gap, not balance), so the Yumi stayed at 16 until the Concord retune below raised it to 17 (that test still passes). Results move in big steps: one Bone Pike more or less per regiment swings Concord matchups by 20 to 30 points, because it changes which AI expects to win and so which side attacks. Once the generated maps had woods and tall grass ([20 · Ambushes](20-ambushes.md#built-so-far), 2026-10-15) the same seeds gave Clans v Brood 60%, Clans v Choir 62%, Clans v Concord 52%, Brood v Choir 57%, Brood v Concord 52% and Choir v Concord 47% (with the weapons of PR #35), before the passes below. Re-run the table after any change to these stats or to combat, on both sets of seeds: `AI_BATTLES=192 AI_FROM=2001 cargo test -p gc_match --release faction_table -- --ignored --nocapture`, then again with `AI_FROM=1` (`AI_PAIR=Brood` or `AI_PAIR=Brood,Choir` narrows it, `AI_CHAMPIONS=off` leaves out the champions, `AI_COVER=off` plays on bare maps).

**Champions (2026-10-15).** The same 192 battles a pair on today's rules (woods and tall grass, [20 · Ambushes](20-ambushes.md); friends pushing through each other; the battle AI's latest update), without champions and then with them allowed ([05 · Champions before battle](05-combat.md#champions-before-battle); the lobby default): Ember Clans against the Brood 58% then 54%, against the Choir 52% then 51%, against the Concord 38% then 37%; the Brood against the Choir 33% then 35%, against the Concord 32% then 33%; the Choir against the Concord 40% then 41%. Champions come out in about half the battles and move no pair by more than 4 points. The Concord now wins 60% to 68% of its battles against everyone, and the Brood a third against the Choir, with or without champions: that came with the rule changes before the champions, so the table is due another balance pass.

<a id="lines-that-push-retune"></a>**Lines that push retune (2026-10-15).** [Lines that push](05-combat.md#lines-that-push) widened the gap at first (Ember Clans against the Brood 62%, the Choir 46%, the Concord 23%; the Brood against the Choir 29%, the Concord 20%; the Choir against the Concord 31%, on 96 battles a pair): the shove goes to whoever already wins the exchange, which the hard-to-hit Bone Pikes usually do, and being pinned costs the living heart while the dead never rout. So the Bone Pikes went from defence 54 to 52 and the Skitterlings from morale 57 to 63 (the swarm holds a little longer before it breaks). Before the deployment phase and the army builder arrived on main that gave every pair 46% to 54%; the table above is the same run on today's main (armies deploy first): every pair between 42% and 57% without champions, and with them Ember Clans against the Brood 47%, the Choir 50%, the Concord 51%; the Brood against the Choir 42%, the Concord 54%; the Choir against the Concord 56%. The Choir is now a little ahead (the Brood wins 42% against it) and is the one to watch. Both knobs are steep: a Bone Pikes defence of 51 gave the Concord's opponents 52% to 64% and 53 gave them 39% to 55%, and Skitterlings morale 65 left the Clans 40% against the Brood while 62 gave them 62%.

<a id="concord-retune"></a>**Concord retune (2026-10-16).** After charges needed a run-up ([05 · Combat](05-combat.md), PR #82) the Concord won 55% to 60% against everyone (the table read Clans 52.5% / 53% / 42.5%, Brood 45.5% / 45%, Choir 39.5%, averaging both seed sets). Three small moves gave the table above: Bone Pikes defence 52 to 51, Yari Ashigaru attack 27 to 26 and one more Yumi Ashigaru model (16 to 17); every pair now sits between 45.5% and 51.5% (champions allowed, 384 battles a pair, each set of 192 between 39% and 61%). Tried and dropped: Skitterlings defence 26 (Brood v Concord 56.5%), Bone Pikes attack 29 (Choir v Concord 40%), Shardlings morale 56 (Clans v Choir 54%, Choir v Concord 42.5%) and Skitterlings morale 64 (no better than 63). Units and map are stored inside replays, so the goldens did not move. The Brood against the Choir (45.5%) and the Clans against the Concord (46%) are the two to watch.

<a id="second-concord-retune"></a>**Second retune (2026-10-16).** Sticky melee contact ([#98](05-combat.md)) left the Brood at 34% to 45% against everyone, and the stronger General AI of stage 3 (#112) then put the Clans and the Choir ahead: on main the table read Clans 56.5 / 53 / 58, Brood v Choir 32.5, Brood v Concord 51, Choir v Concord 61.5. The pass moved Skitterlings defence 25 to 26 and morale 63 to 64, Shardlings morale 57 to 56, Bone Pikes defence 51 to 52 and charge bonus 10 to 14, Yari Ashigaru attack 26 to 25, Yumi Ashigaru models 17 to 16 and accuracy 30 to 31 (champions allowed, 384 battles a pair, every pair between 44% and 54.5%; each set of 192 between 42% and 56%). Steep knobs this time: Bone Pikes attack 27 instead of 28 gave the Concord 60% to 66% against the Brood and the Choir, Yumi accuracy 33 put the Clans 58% against the Choir, and Yumi models 17 moved the Clans against the Concord from 43% to 42.5% (noise). The Brood against the Choir (44%) and the Clans against the Concord (46%) are the two to watch.

<a id="choir-retune"></a>**Choir retune (2026-10-15).** On 384 battles a pair (seeds 1 to 192 and 2001 to 2192, champions allowed) the Choir won all three of its pairs after the lines that push retune: the Clans won 45% against it, the Brood 42%, and it beat the Concord 55%. So the Shardlings went from morale 60 to 57 (they break a little sooner), which gives the table above: every pair 49% to 54%, and each set of 192 between 44% and 56%. Without champions (seeds 2001 to 2192) it reads 51%, 54%, 49% / 49%, 57% / 52%. A Shardling attack of 39 instead left the Clans 42% against the Choir on seeds 1 to 192, and a defence of 39 left the Choir 43% against the Concord there. Every generated map has woods (5% to 23% of the ground on these 384 maps, 10% to 30% counting tall grass). On bare maps (`AI_COVER=off`, seeds 2001 to 2192) the table reads 54%, 22%, 48% / 43%, 44% / 57%: in the open the Choir beats the Clans' archers easily, so hand-made maps with little cover will want their own look.

<a id="third-retune"></a>**Third retune (2026-10-17).** Re-measured on main after the Brace and Fall back change (#108). Nothing had moved since the table of the second retune above (Clans 50 / 52 / 46, Brood 44 / 52, Choir against the Concord 54.5, averaging seeds 1 to 192 and 2001 to 2192, champions allowed). The Choir was ahead of the Brood (56%) and the Concord (54.5%), the Clans were behind the Concord (46%), and the woods penalty found in #38 is long gone (the Choir is level in woods). Two moves were made: Yumi Ashigaru accuracy 31 to 32 and Shardlings file spacing 0.55 to 0.6. The result is Clans 50 / 52 / 50, Brood against the Choir 44, Brood against the Concord 52 and Choir against the Concord 49.5, each set of 192 within about 6 points of that. A third set (seeds 5001 to 5192) read 53 / 53 / 40 / 42 / 52 / 46, so the Brood against the Choir (about 43%) and the Clans against the Concord (about 46% over three sets) are the two to watch; per-set noise is 4 to 6 points. On bare maps (`AI_COVER=off`), before then after: Clans against the Brood 67 to 71, Clans against the Choir 32 to 36, Clans against the Concord 60 to 65, Brood against the Choir 32 to 37, Brood against the Concord 48 to 48, Choir against the Concord 66 to 57. The Choir still wins 57% to 64% on bare ground, and the Clans win 65% to 71% against the Brood and the Concord. That is cover-dependent (archers and the narrow Shardling block): woods are the balance point because every generated map has them, and hand-made maps with little cover need their own look. No stat can even out both kinds of map. Tried and dropped (woods, both seed sets): Shardlings morale 55 (Choir v Concord 49 but Brood v Choir stayed 44); Skitterlings defence 27 (Brood v Choir 47 but Clans v Brood 44.5 and Brood v Concord 55 to 57); Skitterlings charge bonus 44 and Bone Pikes charge bonus 15 (no change); Shardlings spacing 0.5 (the Choir won more: Brood v Choir 40, Choir v Concord 58); Shardlings charge bonus 24 (Choir v Concord 46.6, Brood v Choir the same); Shardlings defence 39 (Brood v Choir 47.7 but Choir v Concord 43, and with morale 57 Brood v Choir fell back to 44.8). Units are stored in replays, so the goldens did not move.

### Weapon flags and mass

Since unit schema 7 (2026-10-17) a stats block can also say `discipline: true` (the Clans' formation discipline: 15% more of its own melee defence standing still in Tight order or a shield wall); a roster unit gets it from its "Formation Discipline" ability instead. Since unit schema 6 (2026-10-15) every stats block in `data/units.ron` needs `points`, what the regiment costs from a Field Battle army's budget (400 for all five today; roster entries without it are read as 400). Since unit schema 5 (October 2026) each stats block can carry the [weapons and formations](05-combat.md#weapons-and-formations) flags: `reach: true` (spears and pikes: the second rank fights at half strength, and they strike as lines meet), `swing_room: true` (big swinging weapons: cramped in Tight order and a shield wall, no defence lost in Loose), `shield: true` (can form a shield wall), and `mass: Light` or `mass: Heavy` (Medium when left out; light and heavy decide who gives ground when [lines push](05-combat.md#lines-that-push)). Anything not written is off.

| Flag | Regiments |
|---|---|
| Reach | Yari Ashigaru, Bone Pikes, Grubfolk Militia |
| Swing room | Katana Samurai, Oni-Bound, Spectral Samurai, Mantid Reapers, Colossus Maw, Shardlings, The Insolvent |
| Shield | Chitin Guard, Shellbacks, Gravewardens |
| Heavy | Battering Ram, Mounted Samurai, Yabusame Riders, Ember Rocket Cart, Oni-Bound, Broodmother Bulk, Colossus Maw, Harmonic Walkers, Glass Striders, Monolith, Caravan, Debt Collectors, Mausoleum Engine, The Insolvent |
| Light | War Kite, Fire Spirit, Gnawer, Skitterlings, Spitters, Burrowers, Drone-Moths, Spawnlings, Skyglass, Lantern Wraiths, Bound Dead |

Spears pay for their second rank: the Yari Ashigaru and Bone Pikes deal 2 damage a blow instead of 3, with attack tuned so the Field Battle armies stay close to even (below). A spear block's punch now depends on its depth: it fades as the second rank thins, and a spear block hit in the flank fights with its plain front.

---

## Schema proposals

What the engine thread would need to add for these rosters to work in the game. Proposals only; `data/units.ron` and the engine crates are unchanged by this work.

1. **Move units into roster files.** `gc_bake` already bakes each trainable unit's `stats` block into the Conquest unit table. What is left is to build the Field Battle unit table from the rosters too (Field Battle points would move into the roster entries) and retire `units.ron`.
2. **Unit record additions** (a new `UNIT_SCHEMA`; schema 5 already added `mass` and the `reach`, `swing_room` and `shield` weapon flags): `class` (u8, the classes above), `tier` (u8), `flags` (u16 bitset: flying, anti-air, crystal, undead, burrows, siege-only, friendly fire, direct fire, detector, terror), `supply` (u8), `cost_grub` and `cost_marrow` (u16), `train_ticks` (u16), `vision` (tiles, Fx), and `abilities` (a range into an ability table).
3. **Counter table**: a 12 × 12 table of percent multipliers keyed by class, baked from data, applied to melee and missile damage beside the facing multiplier.
4. **Ranged additions**: `min_range` (artillery), `splash_radius` (rockets, sacs), `direct_fire` (line of sight), and a structure-damage multiplier.
5. **Heroes**: a hero table (base stats as a unit record, per-level hp and damage growth, four ability ids, ultimate flag) and the leadership rule above.
6. **Ability table**: one record per ability with kind, cooldown, range, radius, duration and a short list of effect primitives with parameters. The primitives this roster uses beyond those named in 10 · Architecture are **ModifyStat**, **Reveal**, **Stun**, **Knockback**, **Control**, **Economy**, **Construct**, **Gather** and **Burrow**.
7. **Metamorphosis**: a `Transform` that keeps the regiment's handle, veterancy and its proportion of surviving models, with a 15 s cocoon state.
8. **Summons**: units marked summoned use no supply and carry a lifetime.
9. **Neutral faction**: a fifth `Faction` value for Grubfolk and creeps, so Grubfolk Militia can leave the Concord roster.
10. **Monsters with one model** already work with today's combat (one blow a second, damage spills across models). They may want a "sweep" rule later (one blow hitting several soldiers) if one-blow-a-second reads too slow on screen.
