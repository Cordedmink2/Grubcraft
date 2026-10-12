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
| — | [Glossary](glossary.md) | Terms used across the docs |
| — | [ADR 0001: Engine and language](adr/0001-engine-and-language.md) | Why a custom Rust engine |
| — | [Dependencies](dependencies.md) | Every third-party crate, why it's there and what it costs |

## Conventions

- Numbers in these docs (costs, rates, sizes) are **first-pass targets**. Once code exists, the canonical values live in `data/*.ron` and budgets in the benchmark suite; docs link to them rather than duplicate them.
- **Docs change with the code.** Every pull request updates the documents it affects (architecture, performance, roadmap progress, controls in the README, dependencies) in the same pull request. Where the design describes more than exists yet, a short **Built so far** note says what is actually built.
- Technical decisions are recorded as ADRs in `docs/adr/`.
- Requirement IDs from the [Specification](02-specification.md) are referenced in issues and pull requests.
