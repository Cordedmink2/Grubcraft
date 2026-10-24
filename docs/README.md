# Grubcraft design documents

Read in order the first time; afterwards each document stands alone.

| # | Document | What's in it |
|---|---|---|
| 01 | [Vision](01-vision.md) | The pitch, design pillars, the experience we're aiming for, non-goals |
| 02 | [Specification](02-specification.md) | Every requirement with an ID, priority and milestone |
| 03 | [World and Factions](03-factions.md) | The fallen Titan, Grub, and the four factions in full |
| 04 | [Economy and Resources](04-economy.md) | Grub, Marrow, supply, upkeep, tiers, Titan Events, trading and raiding |
| 05 | [Combat and Units](05-combat.md) | Regiments, orders, facing, morale, terrain, air, heroes, counters |
| 06 | [Game Modes](06-game-modes.md) | Conquest, Field Battle, "Restage this battle", tutorials, future modes |
| 07 | [World and Maps](07-world-and-maps.md) | Map anatomy, launch maps, Grubfolk hamlets, creeps, map tooling |
| 08 | [UI and UX](08-ui-ux.md) | Layout, controls, camera, onboarding, lobby, settings |
| 09 | [Art and Audio](09-art-and-audio.md) | "Painted diorama" style, faction looks, sourcing, music and sound |
| 10 | [Technical Architecture](10-architecture.md) | Stack, threads, deterministic sim, rendering, memory, assets, testing |
| 11 | [Networking](11-networking.md) | Host-sequenced lockstep, transport, lobby, desync, rejoin, replays |
| 12 | [Performance](12-performance.md) | Budgets for memory, frame and tick; benchmarks; hill-climbing process |
| 13 | [Roadmap](13-roadmap.md) | Milestones M0 to M6 and beyond |
| 14 | [Decisions Needed](14-decisions-needed.md) | Calls for Mario, each with a recommendation |
| 15 | [Staffing Plan](15-staffing-plan.md) | Which work goes to Opus, Sonnet, Haiku, parallel threads and subagents, milestone by milestone |
| 16 | [Art Sourcing](16-art-sourcing.md) | Vetted free (CC0) models, textures and sounds for the Ember Clans and the Brood, with licences and gaps |
| 17 | [Unit Rosters](17-unit-rosters.md) | Every unit, summon and hero for all four factions: stats, costs, abilities, counters, and the data schema they need |
| 18 | [AI](18-ai.md) | Computer opponents and allies, battle tactics, difficulty and personalities; the relations system (stance, liking, trust, memories) and the minds of hamlets, mercenaries and factions met while exploring |
| 19 | [Campaign and Exploration](19-campaign.md) | Exploring inside a match (frontier sites, first contact), and the Campaign: a world map of the Reach played over several game nights, with seasons, hosts, the peoples you meet, relations over time, and battles fought or handed to the generals |
| 20 | [Ambushes](20-ambushes.md) | Hiding in woods and tall grass, spotting, springing an ambush and its morale shock, keeping it readable and fair, counterplay, faction leanings, the AI's Ambush doctrine, and the road to fog of war and campaign ambushes |
| 21 | [Buildings](21-buildings.md) | Every faction's buildings with first-pass cost, build time, hit points, armour, size and supply; construction, repair and power rules; damage against buildings |
| 22 | [Concord models](22-concord-models.md) | The fourteen Carrion Concord 3D models: how they are made, their triangle and bone counts, budgets, sources and how to rebuild one |
| 23 | [Choir models](23-choir-models.md) | The eleven Shard Choir 3D models: crystal gems on bones with floating parts, their triangle and bone counts, budgets, sources and how to rebuild one |
| 24 | [Brood models](24-brood-models.md) | The fifteen Brood 3D models: insect rigs and clips made in code, their triangle and bone counts, budgets, sources and how to rebuild one |
| 25 | [Scenery models](25-scenery-models.md) | The 26 scenery models (trees, bushes, grass and rocks) that dress each land: what grows where, triangle counts, the bake and how to rebuild one |
| 26 | [Fortifications and Bases](26-fortifications.md) | Walls for every faction drawn at any angle with three levels, gates that open for your side, manned walls, three kinds of tower that fire, stakes, barricades and traps, sieges you can see, walled towns, and the eight-stage build plan (approved, not built yet) |
| 27 | [Living Land](27-living-land.md) | Organic world generation (planned): land grown from noise, rock strata, erosion, rivers and lakes instead of line and disc stamps; fairness by measurement with a Mirrored option; habitats inside each biome, riversides and trails; the build stages |
| 28 | [Every Soldier Fights](28-soldier-combat.md) | Individual soldier combat (decided 2026-10-23, not built yet): every soldier fights his own duel when regiments clash; duels in the simulation (option B) first when the project resumes, every soldier simulated (option C) as a later stage; the scrum, exchanges, matched kills, charge impacts, layered animation, the fight library and the seven build stages |
| 29 | [Restart plan](29-restart-plan.md) | How the work after the pause is split: eleven threads by area of code over five waves (Living Land, cinematic lighting, every soldier fights, fortifications and ideas 1 to 19), with models, effort, helpers, cost tiers, shared-file rules and the six wave 1 threads |
| 30 | [Cinematic lighting](30-cinematic-lighting.md) | Light, air and colour as the way to a cinematic look (stages 1 to 3 built): a `Look` with five presets, sky, height fog, film curve and grade, cloud shadows, rim light, backlit foliage, baked ground occlusion and a zoom-linked camera tilt; 4× anti-aliasing, contact shadows, softer shadows, water that mirrors the sky, light motes and the particle system; each land's own air, the lobby's time of day, Clarity and a tuning panel; the stages after (camera craft, weather and night) |
| 31 | [Surface detail](31-surface-detail.md) | Shared repeating detail textures by material (stone, wood, cloth, chitin...) layered over every building and soldier, and the stronger bake with crevice darkening, per-part tint and stronger normals (idea 21 parts 1 and 4) |
| 32 | [Changing land](32-changing-land.md) | Ground that changes mid-match: clearing scrub, woods and thicket to grow a base (idea 22, built), the one ground-change path that burning woods, floods and rubble reuse, reading the ground (idea 13, built: climbs, charges on slopes, mud), weather and the time of day (ideas 1 and 9, built: the sky's state shared with the lighting, fog, rain and night cut sight and range, snow slows the march, rain soaks low ground to mud, the dark hides troops and torches give them away), and later firelight, floods and seasons |
| 33 | [Faction lands](33-faction-lands.md) | Ground that shows whose it is: the Brood's living skin and glowing veins (idea 19, built) drawn by the terrain shader from a per-tile grid, and the Clans, Choir and Concord lands that reuse it (idea 15) |
| 34 | [Ambient life and busy bases](34-ambient-life.md) | The living world as a picture, never a match rule: clearing land on screen (workers cutting, trees tipping over, built), then living touches, busy bases, wildlife and villagers (ideas 3, 10, 22) |
| 35 | [Ground marks](35-ground-marks.md) | Ground that remembers: turned earth where trees were felled and bare tracks where regiments march and fight, healing over minutes (idea 2 render side, idea 22 ground) |
| 36 | [Habitat ground](36-habitat-ground.md) | Each habitat's own ground: bog pools, mud, flower drifts, heather, gravel, crust, bone ripples, leaf litter and moss, drawn by the terrain shader from a per-tile look (Living Land stage 4, ground half) |
| 37 | [The commander on the field](37-commander.md) | Idea 12, built: each player's general rides with their best regiment, steadies the men near him, speaks before the lines meet, and when he falls his side panics |
| — | [Glossary](glossary.md) | Terms used across the docs |
| — | [ADR 0001: Engine and language](adr/0001-engine-and-language.md) | Why a custom Rust engine |
| — | [Dependencies](dependencies.md) | Every third-party crate, why it's there and what it costs |
| — | [Performance log](perf-log.md) | Before and after numbers from each milestone's performance pass |

## Conventions

- Numbers in these docs (costs, rates, sizes) are **first-pass targets**. Once code exists, the canonical values live in `data/*.ron` and budgets in the benchmark suite; docs link to them rather than duplicate them.
- **Docs change with the code.** Every pull request updates the documents it affects (architecture, performance, roadmap progress, controls in the README, dependencies) in the same pull request. Where the design describes more than exists yet, a short **Built so far** note says what is actually built.
- Technical decisions are recorded as ADRs in `docs/adr/`.
- Requirement IDs from the [Specification](02-specification.md) are referenced in issues and pull requests.
