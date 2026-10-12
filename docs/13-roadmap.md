# 13 · Roadmap

Built in vertical slices: every milestone ends with something you can actually play, measured against the performance budgets. Milestones are ordered by risk: the hardest unknowns (determinism across Mac and Windows, regiment combat feel, Wi-Fi lockstep) come first.

## Why Field Battle before Conquest

Conquest is the primary mode, but Field Battle is built first because it is the smallest thing that exercises the riskiest systems: the deterministic simulation, regiment movement and combat, morale, rendering thousands of soldiers, and lockstep networking. Everything Conquest needs on top (economy, structures, fog, heroes, tech) is lower risk and sits on that proven core.

## Milestones

### M0 · Foundations
*Goal: an empty battlefield that runs everywhere, measured from day one.*
- Cargo workspace and crate boundaries ([10 · Architecture](10-architecture.md#workspace-layout)).
- `winit` window, `wgpu` on Metal and DirectX 12, terrain rendering from a heightmap, RTS camera with mouse and trackpad.
- `gc_fixed` math and the empty `gc_sim` tick loop with state hashing.
- Asset pack format and baker skeleton; data files compiled from RON.
- Tagged allocator, dev overlay, Tracy hook.
- CI: build and test on macOS (arm64) and Windows (x64); universal Mac app bundle.
- **Exit:** the app opens to a rendered map on Mario's Mac and a Windows PC within the 4 GB memory budget; the cross-platform hash test passes on an empty sim.
- **Progress (2026-10-12):** first slice landed: workspace, `gc_fixed`, `gc_sim` skeleton with golden determinism, zero-allocation and no-float tests, tagged allocator and OS memory stats, wgpu terrain and regiment-block renderer, RTS camera with trackpad gestures, select-and-move on a scripted 4-player drill, `gc_bench` budget gates, CI on macOS/Windows/Linux, universal Mac app script. Measured on Mario's M4: about 190 MB, well inside the 4 GB budget Mario set on 2026-10-12. Second slice: asset pack format (`gc_assets`), baker (`gc_bake`) compiling `data/units.ron` into embedded data tables with a BLAKE3 data hash, data-driven regiment types in the sim, the dev overlay (`` ` `` or `F11`), and the Tracy hook (`--features profile`). Remaining for the M0 exit: a run on a Windows PC.

### M1 · Regiments on a field (solo sandbox)
*Goal: moving and fighting regiments feel great.*
- Regiments, formations, right-drag frontage and facing, group formation ordering.
- Pathfinding: nav grid, sectors, flow fields.
- Melee, volleys, facing multipliers, charges, morale and routing.
- Soldier puppets, instanced rendering, bone animation textures, Commander View.
- Prototype units from CC0 kits for Ember Clans and Brood (no economy yet).
- Sandbox: place armies for both sides, slow motion, hot-reload unit stats.
- Golden replay tests and `field-8000pt-2v2` benchmark.
- **Exit:** a 1v1 sandbox battle of ~3,000 soldiers looks and feels like Shogun, at 60 fps on the low tier.
- **Progress (2026-10-12):** M1 is built in eight steps: formations in the sim, soldiers on screen, controls, melee and arrows, morale, pathfinding, sandbox, real art. Step 1 (formations: files, ranks, facing, per-soldier hit points, turning, group moves in line, unit data schema 2) is merged. Step 2 (each soldier drawn as a placeholder puppet that steps into its formation slot) is merged. Step 3 (box select, right-drag lines that set frontage and facing, control groups) is merged. Step 4 (melee with flank, rear and charge bonuses; arrow volleys; right-click an enemy to attack; soldiers fall where they fight; unit data schema 3) is merged. Step 5 (morale: wavering, routing, rallying and shattering; banners over shaken regiments; unit data schema 4 with a morale stat) is merged. Step 6 (pathfinding: a walkable-ground grid from the map, portal search across sectors, smoothed waypoint routes, a per-tick search budget; dotted routes for selected regiments and a view of blocked ground) is merged. Step 7 (sandbox: an empty-field start, placing regiments for any side, commanding any side, removing regiments, pause and 0.1× to 4× speed, live stat tuning and reloading `data/units.ron`) is merged. Step 8 (real art: animated CC0 models for every unit type baked into bone animation textures, two levels of detail, death poses, a credits screen, and the Commander View) is merged. Outside the eight steps, a first audio pass is built: the `gc_audio` mixer and `cpal` output, a generated sound set and two adaptive music loops, wired to volleys, melee and morale. Ahead of M2, the menus and lobby screens are built in `gc_ui` (main menu, Play with host and join, lobby with AI players and a version-gate message, settings saved to disk) against a stub network, so the M2 netcode only has to implement one trait.

### M2 · LAN Battle (first game night)
*Goal: Mario and friends play Field Battle together over Wi-Fi.*
- `gc_net`: UDP transport, encryption, host-sequenced lockstep, adaptive input delay.
- LAN discovery, Host/Join screens, lobby, version gate.
- Field Battle mode: army builder with points, deployment phase, victory conditions, 2 battle maps.
- Teams: teammates are allied, through one hostility test that reads a stance table filled from the lobby ([18 · AI](18-ai.md#engine-hooks), hook 1). *Built with AI-1 step 1: lobby teams reach the sim and teammates don't fight.*
- AI track: the battle AI (phases AI-1 and AI-2), so any Field Battle or sandbox slot can be an AI opponent or ally.
- Desync detection with report files; cross-platform determinism CI on golden replays.
- Replays (record and play back).
- Packaging: Mac `.app` (ad-hoc signed), Windows `.exe`, one zip each.
- **Exit:** a 2v2 Field Battle across two Macs and two Windows PCs on home Wi-Fi with zero desyncs.

### M3 · First Harvest (Conquest playable)
*Goal: the primary mode, with two factions.*
- Workers, Grub seeps and Marrow veins, structures and construction, production queues, supply and upkeep, tech tiers.
- Ember Clans economy and Honor; Brood veins, larvae, metamorphosis, Reclaimers.
- Unit, hero and ability data for all four factions is already drafted in `data/rosters/` ([17 · Unit Rosters](17-unit-rosters.md)); M3 bakes it into the game.
- Heroes: levelling, abilities from the effect-primitive system, items, altar revive.
- Fog of war, minimap, army tray, command grid, edge notifications.
- Annihilation and Titan's Heart victories.
- One Duel map (Ribcage Pass) and one Warfield map (The Four Teeth).
- AI track: Conquest AI for the Ember Clans and the Brood; AI allies answer pings; the AI arena (AI-3).
- **Exit:** a full 4-player Conquest game finishes inside the 4 GB memory budget.

### M4 · The Reach Lives
*Goal: the world feels alive and the game is learnable.*
- Grubfolk hamlets (market, mercenaries, rumours, raiding, reputation), creep camps and items, Shrines.
- Titan Events (Tremors and Bloodseeps, Marrow Bloom, Heartbeat).
- Tutorial "First Harvest" and faction primers for Clans and Brood; contextual hints; codex.
- Audio: unit voices, faction themes, battle-ambience layers (the mixer, a first sound set and adaptive calm/battle music are already built; see [09](09-art-and-audio.md#built-so-far)).
- Remaining launch maps (Hollow Lung, Marrowfen), map validator.
- Post-game screen with graphs and "Restage this battle".
- AI track: the relations ledger (stance, liking, trust, memories) that hamlet reputation runs on; mercenary companies; the parley panel (AI-4).
- **Exit:** a newcomer can learn and win a game after the tutorial; first full balance pass.

### M5 · Choir and Concord
*Goal: all four factions.*
- The Choir: Resonators, chord geometry and harmony, Resonance, Phase Shift, Refraction.
- The Carrion Concord: caravans and trade routes, Reliquaries and Tithe, raising the dead, crumbling undead.
- Primers for both; balance pass across all pairings.
- AI track: Conquest AI for the Choir and the Concord (AI-5).
- **Exit:** every faction pairing played at least five times among the group, no faction winning more than 60%.

### M6 · Production polish (1.0)
*Goal: robust enough that nobody thinks about the tech.*
- Disconnect handling and rejoin, multiplayer save and load.
- Settings completeness, key rebinding, accessibility options.
- Performance pass against every budget; stretch targets attempted.
- Art pass: custom models for faction-defining units and heroes.
- Simple self-update check (host's version offered to joining friends).
- Lobby option: an AI runs a disconnected player's army until they rejoin.
- **Exit:** 1.0 tagged; a month of game nights with no crash or desync.

## The AI track

AI players were planned for after 1.0; on 2026-10-12 Mario asked for an intelligent AI designed now, one that also plans for exploration and for friendly, neutral and mercenary factions. [18 · AI](18-ai.md) is that design. It runs as a track beside the milestones: the work lives in its own crate (`gc_ai`) and tool (`gc_arena`), so a parallel thread can build it without touching code the engine thread is changing. Only a few small engine hooks have to land inside the milestones above, and two of them (team stances, copying a battle area for "Restage this battle") are needed anyway.

| Phase | What | With |
|---|---|---|
| AI-1 · Battle AI | Perception, terrain analysis, battle estimator, battle tactics with four doctrines, difficulty levels, drills | Starts once the sandbox lands; ships with M2. Built in three steps: (1) the battle AI itself, playable from the lobby and the sandbox; (2) terrain sense, the eight drills and the estimator check; (3) the AI lens and journal overlay. Step 1 is built (2026-10-12) |
| AI-2 · Rehearsal | The AI plays candidate battle plans ahead in a copy of the real sim; the Warlord difficulty | M2 |
| AI-3 · Conquest AI | Strategy, economy, scouting, armies; Clans and Brood; AI allies; the arena | M3 |
| AI-4 · Relations and the living world | Relations ledger, hamlets, creep camps, mercenary companies, parley | M4 |
| AI-5 · Choir and Concord AI | Their economy modules and doctrines | M5 |
| AI-6 · Exploration and diplomacy | Factions met while exploring; diplomacy between players as an option | After 1.0 |

## After 1.0 (unordered)

| Feature | Notes |
|---|---|
| Naval expansion | Water maps, ships, coastal raids; the map format already reserves a depth layer |
| Exploration and diplomacy | Factions met while exploring, with their own minds and memories; diplomacy between players as a lobby option (AI-6 in [18 · AI](18-ai.md#build-plan)) |
| Host migration | Every client already has full state |
| Internet join codes | UDP hole punching via a tiny rendezvous service |
| Map editor for players | The internal edit mode, exposed |
| Seeded map generator | Validated by `gc_mapcheck` for fairness |
| Co-op Siege mode | Wave defence against Titan-spawn |
| Fifth faction | Feathered Republic (air nomads) or Drowned Tide (with naval) |

## How work gets done

- Each milestone is broken into GitHub issues; each issue lands as a pull request with tests and, where relevant, benchmark results.
- Every PR runs: format, lint (including the no-floats-in-sim rule), unit tests, golden replays on Mac and Windows, and headless benchmarks with budget gates.
- Game nights are the real milestone reviews: what felt good, what felt bad, captured as issues the next day.
