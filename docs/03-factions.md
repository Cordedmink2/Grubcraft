# 03 · World and Factions

## The world: the Reach

Nobody remembers the sky before the **Titan** fell. It came down burning, broke the old world, and died. Its body is the land now: ridges of fossilised rib, plains of compacted hide, forests rooted in its fur, rivers running through old veins. At the centre of every map, somewhere under the soil, its heart still beats once a day.

As the Titan slowly rots and regrows it weeps **Grub**: a warm, glowing, faintly sweet ooze that wells up at **seeps**. Grub is food, fuel, mortar and medicine. Deeper down, the Titan's bones hold **Marrow**, a dense, luminous crystal that makes steel stronger, magic sharper and machines sing. The lands built on the corpse are called **the Reach**.

Tone: *whimsical-grim*. Warcraft III's bright, chunky, slightly comic world, with Shogun's seriousness on the battlefield. Units have personality and joke when you click them too many times; battles feel weighty and dangerous.

### Peoples of the Reach

- **Four playable factions** (below), each a different answer to "what do you do with a dead god?"
- **The Grubfolk.** Small, stubborn, neutral farmers who live in hamlets around seeps. They trade, they hire out militia, and they hold grudges. See [07 · World and Maps](07-world-and-maps.md).
- **Titan-spawn.** Feral creatures that grow from the corpse: gutworms, bile toads, marrow golems. They guard rich spots and drop loot (Warcraft-style creep camps).
- **Mercenary companies** (planned). Bands of fighters with a captain and a price who camp at hamlets or wander between them, and serve whoever pays and keeps paying. See [18 · AI](18-ai.md#the-living-world-encounters).
- **Other peoples** (later). Factions met while exploring the Reach, friendly, neutral or hostile, each with its own mind and memory of how you treated it.
- **The Titan itself.** It is still slightly alive. Its tremors reshape matches (see *Titan Events* in [04 · Economy](04-economy.md)).

---

## Faction design rules

All four factions share:

- The same two resources: **Grub** (common) and **Marrow** (rare, tech and elites).
- A supply cap of **150**, raised by a faction-specific housing structure.
- Three tech tiers, unlocked by upgrading the main building.
- Three heroes (one at a time at tier 1, up to three by tier 3), levelling 1 to 10.
- One **faction meter** shown next to the resources. It is the only extra thing on the HUD.
- Six to eight regiment types plus workers, siege and one tier-3 monster.

What differs is *how you get Grub*, *what the meter does*, and *the signature rule* each faction bends.

Regiment sizes and costs below are first-pass numbers for balance to start from. Full rosters with every stat, ability, summon and hero, plus a few regiments added to fill gaps, are in [17 · Unit Rosters](17-unit-rosters.md); the canonical values live in `data/rosters/*.ron`, not in these docs.

**Supply costs** (common to all factions): worker 1, light regiment 3, line regiment 4, elite or cavalry 5, artillery 4, air flight 3, monster 6, hero 5.

---

## 1 · The Ember Clans

> *"The Titan gave us its flesh. We give it discipline."*

Feudal clans who terraced the Titan's flanks into rice-paddy steps, forged its marrow into blades, and fight in tight disciplined blocks under their lords' banners. The **Shogun** faction, and the recommended first faction for new players because it plays like a classic RTS.

**Fantasy:** banners, drums, spear walls, charging samurai cavalry, fire rockets at dusk, giant war kites.

### Economy: carry and store (the classic)
- **Hands** (workers) walk to a seep, fill buckets, carry Grub back to the **Clan Hall** or a **Granary**. Up to 5 Hands per seep saturate it; the seep shows five pips so saturation is obvious.
- Marrow is mined the same way at Marrow veins (3 Hands saturate).
- Vulnerable but readable: raiders kill workers in transit. Granaries placed close to seeps shorten trips.

### Faction meter: **Honor** (0 to 100)
Earned by fighting well, lost by fleeing:
- +5 when an enemy regiment routs in combat with yours
- +10 for winning an engagement while outnumbered
- +2 per minute per Shrine held
- −5 when one of your regiments routs

Spent on **Edicts**, global abilities cast from the Ancestor Shrine:

| Edict | Honor | Effect |
|---|---|---|
| Rice Tax | 25 | Instantly gain 300 Grub |
| Forced March | 20 | Selected army +30% speed for 30 s |
| Banner of the Ancestors | 35 | Regiments in an area cannot rout for 20 s |
| Ancestral Return (tier 3) | 80 | Summon a spectral samurai regiment for 60 s |

Honor makes the Clans *want* to take fair fights and win them, which is exactly Shogun's spirit.

### Signature rule: **Formation discipline**
Clan regiments get +15% defence while in a tight formation and not moving, and their spear regiments can **Brace** (hold stance) to double charge-defence. They are the best at holding a line and the worst at chasing.

### Structures
| Structure | Role |
|---|---|
| Clan Hall → Castle Keep → Daimyo's Citadel | Main building, workers, Grub drop-off, tiers |
| Farmstead | +10 supply |
| Granary | Grub drop-off, research economy upgrades |
| Dojo | Infantry |
| Stables | Cavalry |
| Ember Workshop | Rocket carts, battering rams |
| Kite Loft | Air |
| Ancestor Shrine | Heroes and Edicts |
| Forge | Weapon and armour upgrades |
| Watchtower, Palisade, Gate | Defence |

Cost, hit points, build time and size for each: [21 · Buildings](21-buildings.md#ember-clans).

### Regiments
| Unit | Tier | Models | Cost (Grub/Marrow) | Role and signature |
|---|---|---|---|---|
| Hands | 1 | 1 | 50 / 0 | Worker |
| Yari Ashigaru | 1 | 28 | 120 / 0 | Spear line. **Brace** vs cavalry |
| Yumi Ashigaru | 1 | 16 | 130 / 0 | Archers. **Fire Arrows** toggle for burning buildings |
| Battering Ram | 1 | 1 | 100 / 0 | Siege, only hits buildings |
| Katana Samurai | 2 | 16 | 160 / 40 | Elite melee, high morale, **Kiai** shout breaks enemy morale |
| Mounted Samurai | 2 | 8 | 180 / 40 | Shock cavalry, devastating charges |
| Ember Rocket Cart | 2 | 4 | 150 / 60 | Long-range area artillery, inaccurate, terrifying |
| War Kite | 2 | 4 | 140 / 40 | Air scout, drops fire pots |
| Yabusame Riders | 2 | 8 | 170 / 40 | Horse archers that shoot on the move (added in 17) |
| Oni-Bound | 3 | 1 | 250 / 150 | Bound demon monster, **Terror** aura shakes nearby enemy morale |

### Heroes
- **The Daimyo** (commander): *Rally* (restores morale in an area), *Command Aura* (+armour), *Challenge* (forces an enemy hero to duel), ultimate *Shogun's Banner* (army-wide morale and speed).
- **The Ronin** (duelist): *Iaijutsu Dash* (blink strike), *Wind Cut* (cone damage), *Wanderer* (passive regen when alone), ultimate *Thousand Cuts*.
- **The Onmyōji** (mystic): *Fire Spirit* (summon), *Ward Talisman* (zone that blocks ranged fire), *Divination* (reveal area), ultimate *Kitsune Storm*.

**Strengths:** defensive lines, morale, approachable. **Weaknesses:** slow, worker-raidable, punished by flanks they can't turn to face.

---

## 2 · The Brood

> *"We were the grubs in the god's belly. Now we are the god's hunger."*

The namesake faction. Insects that grew fat and clever eating the Titan from the inside, now a single hive mind with a thousand mouths. **Zerg-flavoured but built around a network, not a swarm.**

**Fantasy:** pulsing hives, glowing root-veins across the ground, mid-battle metamorphosis, devouring the dead.

### Economy: the vein network
- **Gnawers** (worker grubs) **burrow into a seep and stay there** (5 per seep). Burrowed Gnawers are invisible and immune to almost everything.
- Grub flows back to the **Hive** through **root-veins**, a creep-like network that spreads from the Hive and from **Pustules** (cheap relay nodes, which also give +8 supply).
- **The network is the weak point.** If a raider kills a Pustule, every seep beyond it stops delivering until the vein is regrown. You raid the Brood by cutting veins, not by hunting workers. Veins glow brighter as Grub flows through them, so both players can read the economy at a glance.
- **Reclaimers** walk over corpses on any battlefield and digest them into Grub. The Brood profits from every fight.
- Brood structures are grown by *consuming* a Gnawer, so building costs you a worker.

### Faction meter: **Larvae** (0 to N)
- Every regiment hatches from a larva. Each Hive and Hatchery pit holds up to 3 larvae and spawns one every 12 s.
- The meter shows larvae available across all hives. It is both a production limiter and a decision: Gnawer or warrior?

### Signature rule: **Metamorphosis in the field**
Every tier-1 Brood regiment can evolve, *on the battlefield*, into one of two tier-2 forms. It cocoons for 15 s (vulnerable, cannot move), then emerges as the new form with its veterancy intact. You adapt your army to the fight you are in, not the one you guessed at.

```
Skitterlings ─┬─▶ Spitters  (ranged acid, armour-melting)
              └─▶ Burrowers (tunnel under terrain, ambush from below)
Chitin Guard ─┬─▶ Mantid Reapers (anti-cavalry, high damage)
              └─▶ Shellbacks     (near-immobile bastion, anti-air spines)
```

### Structures
| Structure | Role |
|---|---|
| Hive → Great Hive → Throne Hive | Main building, larvae, tiers |
| Pustule | Vein relay, +8 supply |
| Hatchery Pit | +3 larva capacity, faster spawning |
| Chrysalis Chamber | Unlocks evolutions |
| Moth Roost | Air |
| Gorge | Monsters and siege |
| Spine Mound | Defensive tower |
| Queen's Chamber | Heroes |

Cost, hit points, build time and size for each: [21 · Buildings](21-buildings.md#brood).

### Regiments
| Unit | Tier | Models | Cost (Grub/Marrow) | Role and signature |
|---|---|---|---|---|
| Gnawer | 1 | 1 | 50 / 0 | Worker, burrows in seeps, consumed to build |
| Skitterlings | 1 | 32 | 100 / 0 | Fast, fragile swarm; evolves |
| Chitin Guard | 1 | 12 | 130 / 0 | Armoured melee anchor; evolves |
| Reclaimer | 1 | 1 | 75 / 0 | Eats corpses into Grub, slow heal aura |
| Drone-Moths | 2 | 6 | 120 / 30 | Air scouts, spore bombs, reveal burrowed units |
| Broodmother Bulk | 2 | 1 | 160 / 60 | Siege, lobs larva sacs that hatch temporary Skitterlings |
| Colossus Maw | 3 | 1 | 260 / 140 | Monster, **Devour** swallows enemy models whole, terror |

### Heroes
- **The Queen-in-Waiting** (mother): *Spawn Brood* (free temporary swarm), *Royal Jelly* (heal), *Pheromone Trail* (speed for allies), ultimate *Swarm Tide*.
- **The Devourer** (bruiser): *Gorge* (eats a model to heal), *Grow* (gets permanently larger as it kills, capped), *Burrow Charge*, ultimate *Maw of the Titan*.
- **The Whisperer** (controller): *Mind-Spore* (take control of a creep or small enemy regiment), *Confusion Cloud*, *Hive Sight*, ultimate *Puppet Army*.

**Strengths:** safe workers, flexible army, feeds on war. **Weaknesses:** fragile network, larva bottleneck, buildings cost workers.

---

## 3 · The Choir

> *"Listen. The god is still singing. We only tune the chord."*

Ancient crystal automata, the Titan's own nervous system come loose and self-aware, or perhaps visitors that fell with it. They harvest by *resonance*. The **sci-fi** faction: luminous glass, harmonic machines, light-beams, teleportation.

**Fantasy:** humming pylons, triangles of light stretched across the map, beams that refract through crystal soldiers, armies that blink across the battlefield.

### Economy: chords and geometry
- **Tuners** (worker constructs) build and repair. They do not gather.
- **Resonators** (pylons) placed within 4 tiles of a seep *tune* it, producing Grub with no workers at all. Each Resonator also gives +4 supply.
- Resonators within 14 tiles of each other link with a visible beam. **Any three mutually linked Resonators form a Chord**: a glowing triangle on the ground.
- **Harmony bonus:** every seep inside a Chord produces extra Grub, scaling with the triangle's area, from +10% for a small chord to +60% at the maximum area. A seep only takes its best chord; overlapping chords do not stack.
- **The weak point is geometry.** Kill one Resonator and every chord using it collapses. Bigger triangles pay more and are harder to defend. Every Choir player is constantly choosing between greed and safety, and every opponent can *see* that choice on the map.
- Marrow is harvested by **Siphons**, special Resonators built on Marrow veins.
- Choir structures must sit inside a Resonator's field (powered); outside it they shut down.

### Faction meter: **Resonance** (0 to 100)
Charges continuously while chords are intact (faster with more chords). Spent on:

| Ability | Resonance | Effect |
|---|---|---|
| Overclock | 20 | A structure or regiment works/attacks 50% faster for 20 s |
| Phase Shift | 60 | Teleport selected regiments to any of your Resonators |
| Silence the Chord (tier 3) | 80 | Enemy abilities and faction meters near a target point freeze for 15 s |

### Signature rule: **Refraction**
Choir beam weapons pass *through* allied crystal units and split. A Prism Lancer firing past a Shardling regiment hits two targets instead of one. Positioning your own units as lenses is the faction's micro skill ceiling, and it reads beautifully on screen.

### Structures
| Structure | Role |
|---|---|
| Spire → Cathedral → Grand Organ | Main building, Tuners, tiers |
| Resonator | Tunes seeps, links chords, +4 supply, powers buildings |
| Siphon | Marrow harvesting |
| Forge of Facets | Infantry |
| Harmonics Lab | Upgrades |
| Skyglass Loom | Air |
| Bell Tower | Defence, pulse that silences abilities |
| Sanctum of Echoes | Heroes |

Cost, hit points, build time and size for each: [21 · Buildings](21-buildings.md#choir).

### Regiments
| Unit | Tier | Models | Cost (Grub/Marrow) | Role and signature |
|---|---|---|---|---|
| Tuner | 1 | 1 | 60 / 0 | Builder and repairer |
| Shardlings | 1 | 14 | 110 / 0 | Light melee; shatter on death for small area damage |
| Prism Lancers | 1 | 10 | 140 / 0 | Beam ranged; **Refraction** through allied crystal |
| Harmonic Walkers | 2 | 6 | 170 / 50 | Tanky tripods, **Sonic Pulse** knockback breaks enemy formations |
| Skyglass | 2 | 4 | 130 / 40 | Fast fragile air, strong anti-air |
| Conductor | 2 | 1 | 120 / 60 | Support, carries a **mobile Resonator** that can complete a chord |
| Glass Striders | 2 | 6 | 175 / 45 | Crystal cavalry, **Phase Charge** blinks into contact (added in 17) |
| Monolith | 3 | 1 | 280 / 160 | Slow siege monster, sonic beam shatters structures |

### Heroes
- **The Maestro** (economy/support): *Crescendo* (allied attack speed), *Retune* (move a Resonator instantly), *Harmony* (passive chord bonus), ultimate *Symphony* (all chords +100% for 30 s).
- **The Refractor** (damage): *Split Beam*, *Mirror Shield* (reflect projectiles), *Lens* (makes any allied unit refract), ultimate *Prismatic Lance*.
- **The Hollow Bell** (control): *Toll* (stun), *Silence*, *Echo Step* (blink), ultimate *Deafening Peal* (area rout check).

**Strengths:** no workers to kill, mobility, high skill ceiling. **Weaknesses:** brittle chords, low model counts, expensive losses.

---

## 4 · The Carrion Concord

> *"Everything on this corpse is collateral. Even you."*

A guild of merchant-necromancers who run the Reach's trade roads, buy the dead at a discount and raise them to settle debts. The **trade and undeath** faction: bone caravans, ledgers, lanterns, skeleton pikemen who never break.

**Fantasy:** long caravans rolling between towns, ledger-clerks stamping contracts, raising yesterday's battlefield as today's army.

### Economy: trade routes and tithes
- **Ledger Clerks** (workers) build, and mine Marrow the classic way (carry).
- **Caravans** carry goods between your **Counting House** and a **trade post**: a neutral Grubfolk hamlet (full rate), your own **Waystation** (half rate), or an allied player's main building (full rate, and the ally gets a share). **Income per trip scales with distance.**
- Caravans are big, slow and visible. Raiding them is the main way to hurt the Concord, and escorting them is the main way to defend. Long routes are rich and dangerous; short routes are safe and poor.
- Every map guarantees a hamlet within reach of every start position, and Waystations let the Concord trade even if hamlets are hostile or razed.

### Faction meter: **Tithe** (0 to 250)
- Every model that dies, *on any side*, within 12 tiles of a Concord **Reliquary** pays 1 Tithe. Reliquaries are cheap and can be built anywhere you have vision, including next to other players' fights.
- Tithe buys:

| Contract | Tithe | Effect |
|---|---|---|
| Raise the Bound | 40 | Turn corpses in an area into a temporary Bound Dead regiment (90 s) |
| Indenture | 100 | Raise a *permanent* Bound Dead regiment from corpses; uses no supply but decays away from Reliquaries |
| Foreclose | 60 | Take control of a neutral creep camp |
| Hire Militia | 30 | Hire a Grubfolk militia regiment from a friendly hamlet |

### Signature rule: **The dead don't rout**
Concord undead regiments ignore morale entirely. Instead they **crumble**: they cannot be reinforced with Grub, only with Tithe, and they slowly lose models when far from a Reliquary. Their living units (crossbowmen, cavalry riders) do rout. Mixing reliable dead with fragile living is the faction's puzzle.

### Structures
| Structure | Role |
|---|---|
| Counting House → Exchange → Grand Bourse | Main building, Clerks, Caravans, tiers |
| Waystation | Own trade post, extends routes |
| Tenement | +10 supply |
| Reliquary | Collects Tithe, raises dead |
| Ossuary | Undead infantry |
| Guildhall | Living infantry and cavalry |
| Lantern Spire | Air |
| Tollgate | Defensive tower; damage it deals earns Grub |
| Vault of Names | Heroes |

Cost, hit points, build time and size for each: [21 · Buildings](21-buildings.md#carrion-concord).

### Regiments
| Unit | Tier | Models | Cost (Grub/Marrow) | Role and signature |
|---|---|---|---|---|
| Ledger Clerk | 1 | 1 | 50 / 0 | Worker |
| Caravan | 1 | 1 | 75 / 0 | Trade unit; no attack |
| Bone Pikes | 1 | 19 | 90 / 0 | Undead pike line, cheap, never routs |
| Crossbow Auditors | 1 | 12 | 140 / 0 | Living, armour-piercing ranged |
| Gravewardens | 2 | 12 | 170 / 40 | Undead heavy infantry, shrugs off flank and rear hits (added in 17) |
| Debt Collectors | 2 | 6 | 190 / 50 | Bone-horse heavy cavalry; hits on buildings steal Grub |
| Lantern Wraiths | 2 | 4 | 140 / 40 | Air, drains enemy morale |
| Mausoleum Engine | 3 | 1 | 240 / 120 | Mobile crypt siege, raises dead around it |
| The Insolvent | 3 | 1 | 260 / 150 | Stitched flesh giant monster |

### Heroes
- **The Magnate** (economy): *Hostile Takeover* (convert a hamlet to your exclusive trade post), *Bribe* (pacify a creep camp), *Compound Interest* (passive income), ultimate *Market Crash* (enemy income −50% for 30 s).
- **The Undertaker** (necromancy): *Raise Dead*, *Corpse Explosion*, *Last Rites* (heal undead), ultimate *Mass Grave*.
- **The Usurer** (duelist/debuff): *Leech Debt* (life drain), *Lien* (marked target takes extra damage), *Collateral* (steal a buff), ultimate *Foreclosure* (instantly kill a damaged regiment's weakest third).

**Strengths:** grows richer the longer and bloodier the game, undead never break. **Weaknesses:** slow start, exposed caravans, depends on the map and on others fighting.

---

## Faction pairing and release order

Recommended order (see [13 · Roadmap](13-roadmap.md)):

1. **Ember Clans and the Brood first.** They contrast hard (disciplined lines versus adaptive swarm; carry economy versus network economy) but both are easy to understand, and between them they exercise every core system: formations, morale, workers, structures, heroes, evolution, corpses.
2. **The Choir third.** Needs the chord-geometry system and teleportation.
3. **The Carrion Concord fourth.** Needs NPC hamlets, trade routes and corpse-raising, which arrive with the living-world milestone.

## Factions considered and set aside

- **Drowned Tide** (water faction, tides and sunken reefs as economy). Parked until naval gameplay exists.
- **Feathered Republic** (sky-nomads with an all-air army). Too hard to balance against ground-only factions early on; a possible fifth faction once air is mature.
