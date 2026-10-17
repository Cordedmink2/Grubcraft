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
| — | [Glossary](glossary.md) | Terms used across the docs |
| — | [ADR 0001: Engine and language](adr/0001-engine-and-language.md) | Why a custom Rust engine |
| — | [Dependencies](dependencies.md) | Every third-party crate, why it's there and what it costs |
| — | [Performance log](perf-log.md) | Before and after numbers from each milestone's performance pass |

## Conventions

- Numbers in these docs (costs, rates, sizes) are **first-pass targets**. Once code exists, the canonical values live in `data/*.ron` and budgets in the benchmark suite; docs link to them rather than duplicate them.
- **Docs change with the code.** Every pull request updates the documents it affects (architecture, performance, roadmap progress, controls in the README, dependencies) in the same pull request. Where the design describes more than exists yet, a short **Built so far** note says what is actually built.
- Technical decisions are recorded as ADRs in `docs/adr/`.
- Requirement IDs from the [Specification](02-specification.md) are referenced in issues and pull requests.
