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
- Golden replay tests and `field-8000pt-2v2` benchmark. *Built (2026-10-15): four recorded battles in `replays/` that CI plays back on Mac, Windows and Linux, and the 6,000-soldier grand battle benchmark (0.3 ms p99 a tick).*
- **Exit:** a 1v1 sandbox battle of ~3,000 soldiers looks and feels like Shogun, at 60 fps on the low tier.
- **Progress (2026-10-12):** M1 is built in eight steps: formations in the sim, soldiers on screen, controls, melee and arrows, morale, pathfinding, sandbox, real art. Step 1 (formations: files, ranks, facing, per-soldier hit points, turning, group moves in line, unit data schema 2) is merged. Step 2 (each soldier drawn as a placeholder puppet that steps into its formation slot) is merged. Step 3 (box select, right-drag lines that set frontage and facing, control groups) is merged. Step 4 (melee with flank, rear and charge bonuses; arrow volleys; right-click an enemy to attack; soldiers fall where they fight; unit data schema 3) is merged. Step 5 (morale: wavering, routing, rallying and shattering; banners over shaken regiments; unit data schema 4 with a morale stat) is merged. Step 6 (pathfinding: a walkable-ground grid from the map, portal search across sectors, smoothed waypoint routes, a per-tick search budget; dotted routes for selected regiments and a view of blocked ground) is merged. Step 7 (sandbox: an empty-field start, placing regiments for any side, commanding any side, removing regiments, pause and 0.1× to 4× speed, live stat tuning and reloading `data/units.ron`) is merged. Step 8 (real art: animated CC0 models for every unit type baked into bone animation textures, two levels of detail, death poses, a credits screen, and the Commander View) is merged. Outside the eight steps, a first audio pass is built: the `gc_audio` mixer and `cpal` output, a generated sound set and two adaptive music loops, wired to volleys, melee and morale. Ahead of M2, the menus and lobby screens are built in `gc_ui` (main menu, Play with host and join, lobby with AI players and a version-gate message, settings saved to disk) against a stub network, so the M2 netcode only had to implement one trait (it does since M2 step 1). The golden replays and the `field-8000pt-2v2` benchmark (its simulation half) are built. Remaining for the M1 exit: the 3,000-soldier, 60 fps check on real machines, where Mario judges the feel.

### M2 · LAN Battle (first game night)
*Goal: Mario and friends play Field Battle together over Wi-Fi.*
- `gc_net`: UDP transport, encryption, host-sequenced lockstep, adaptive input delay. *Built: lockstep in step 1, the adaptive delay in step 2, encryption in step 3.*
- LAN discovery, Host/Join screens, lobby, version gate. *Built, with join codes and a lobby password since step 3.*
- Field Battle mode: army builder with points, deployment phase, victory conditions, 2 battle maps.
- Ambushes: woods and tall grass that hide regiments, spotting, and the morale shock of a sprung ambush; the AI plays fair with them and lays its own ([20 · Ambushes](20-ambushes.md)). *Built (2026-10-15): step A, the battle rules, the woods on the maps, the on-screen cues and the AI's Ambush doctrine. Forest slow-down and arrows caught by trees followed (2026-10-16); the rest of step B (unit traits) and step C (fog of war, with M3) follow.*
- Teams: teammates are allied, through one hostility test that reads a stance table filled from the lobby ([18 · AI](18-ai.md#engine-hooks), hook 1). *Built with AI-1 step 1: lobby teams reach the sim and teammates don't fight.*
- AI track: the battle AI (phases AI-1 and AI-2), so any Field Battle or sandbox slot can be an AI opponent or ally. *AI-1 built (2026-10-13): the battle AI at three levels, reading the ground, and the AI lens (`L`). AI-2 built (2026-10-15): the fourth level, the Warlord, which reads the enemy's plan from how its army moves and rehearses its own plans in a copy of the real simulation before choosing one ([18 · AI](18-ai.md#build-plan)).*
- Terrain rules for battle maps: high ground and fords ([05 · Combat](05-combat.md#terrain)). *Built (2026-10-13): fighting downhill and shooting from high ground, slow wading, no charging out of a river, extra blows on waders. Friendly regiments queue in fords and passes instead of stacking (built the same day); elsewhere they push through each other slowly and fall into disorder (2026-10-15).*
- Desync detection with report files; cross-platform determinism CI on golden replays. *Detection built in step 1, report files in step 2 (replay files since step 6a); the golden-replay CI was built for the M1 exit.*
- Pausing for a player who drops (up to 90 s, or play on). *Built in step 2.*
- Replays (record and play back). *Built in step 6a: every battle saved, watched from the main menu.*
- Soldier fighting, stage 1: duel pairs and living formations (acceleration, settle, idle sway), see [09](09-art-and-audio.md#animation). *Built.*
- Battle feel from Mount & Blade ([14](14-decisions-needed.md#battle-feel-from-mount--blade-decided-2026-10-15), picked 2026-10-15): weapon room (spears from two ranks, swinging blades that need room, shields, and the Tight, Loose and Shield wall formations, [05](05-combat.md#weapons-and-formations)) *built 2026-10-15*; duels that read the numbers (soldier fighting, stage 2: parries, blocks, staggers, crush-through, charge knockdowns and partner kills from the real stats, [09](09-art-and-audio.md#animation)) *built 2026-10-15*; champions before battle in battles with computer players (a lobby option, on by default, [05](05-combat.md#champions-before-battle)) *built 2026-10-15*; lines that push (the side winning a melee drives the other back, weighted by mass; a regiment pinned against friends, water or a cliff falls into disorder, [05](05-combat.md#lines-that-push)) *built 2026-10-15*.
- Packaging: Mac `.app` (ad-hoc signed), Windows `.exe`, one zip each.
- **Exit:** a 2v2 Field Battle across two Macs and two Windows PCs on home Wi-Fi with zero desyncs.
- **Progress (2026-10-13):** the netcode is built in steps. Step 1 is built: `gc_net` hosts and joins lobbies over UDP, finds games on the network, checks builds, data and maps match, and plays Field Battles in host-sequenced lockstep with desync detection and disconnect notices Step 2 (2026-10-15): the input delay follows the slowest connection, every machine saves a desync report when worlds disagree, and the match pauses for a player who drops. Step 3 (2026-10-15): every link is encrypted, the host's lobby shows a join code, and a lobby can have a password. Step 4 (2026-10-15): maps are made with whole-number maths, so every machine builds the identical map from the seed. Step 5a (2026-10-15): the points budget and army builder in the lobby. Step 5b (2026-10-15): a deployment phase before the battle. Step 6a (2026-10-15): every battle is saved as a replay to watch from the main menu, and desync reports are replays. Step 6b (2026-10-15): a player whose game restarts can rejoin the match and catch up. Step 7 (2026-10-15): packaging. The Mac zip holds the ad-hoc signed app and a read-me, the Windows zip holds a Grubcraft folder with `grubcraft.exe` and a read-me, and pushing a `v*` tag builds both and attaches them to a GitHub Release ([release checklist](release-checklist.md)). Tailscale code (2026-10-16): with Tailscale running, the host's lobby shows a code friends anywhere can join with. [11 · Networking](11-networking.md#transport) has the detail. **Parked until needed:** repairing a player who goes out of sync (sim snapshots, step 6c, [issue #59](https://github.com/mariothapliyal/Grubcraft/issues/59)); build it only if desyncs show up in real games.

### M3 · First Harvest (Conquest playable)
*Goal: the primary mode, with two factions.*
- Workers, Grub seeps and Marrow veins, structures and construction, production queues, supply and upkeep, tech tiers.
- Soldier fighting with the chosen faction art kits: shield-raise, parry and stagger clips made for the new models (stage 2's rules are built in M2, see above).
- Ember Clans economy and Honor; Brood veins, larvae, metamorphosis, Reclaimers.
- Unit, hero, ability and building data for all four factions is already drafted in `data/rosters/` ([17 · Unit Rosters](17-unit-rosters.md), [21 · Buildings](21-buildings.md)); M3 bakes it into the game.
- Heroes: levelling, abilities from the effect-primitive system, items, altar revive.
- Fog of war (built on the ambush vision rules, [20](20-ambushes.md#from-hidden-regiments-to-fog-of-war)), minimap, army tray, command grid, edge notifications. *Built early (2026-10-15, Conquest UI thread): the minimap and army tray, the command grid, and edge alerts for attacks, ambushes and routs, and the hero panel on a made-up hero (Shift+H), on today's battles; the economy parts, fog and the real hero data follow.*
- Annihilation and Titan's Heart victories.
- One Duel map (Ribcage Pass) and one Warfield map (The Four Teeth).
  - **Progress (2026-10-15):** both maps and the map validator `gc_mapcheck` are built (`data/maps/`, [07 · Map format and tooling](07-world-and-maps.md#map-format-and-tooling)): first versions with plain terrain, checked for fairness, and not yet loaded by the game.
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
- Campaign track: exploring inside a match, with frontier sites and first contact (C-0 in [19](19-campaign.md#build-plan)).
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
- Art pass: custom models for faction-defining units and heroes, moving toward high-fidelity soldiers (smooth skinning, 63-bone skeletons, textured materials, shadows and glow are built ahead of time; more detail levels follow); soldier fighting stage 3, matched kills for heroes, charges and cavalry.
- Simple self-update check (host's version offered to joining friends).
- Lobby option: an AI runs a disconnected player's army until they rejoin.
- **Exit:** 1.0 tagged; a month of game nights with no crash or desync.

## The AI track

AI players were planned for after 1.0; on 2026-10-12 Mario asked for an intelligent AI designed now, one that also plans for exploration and for friendly, neutral and mercenary factions. [18 · AI](18-ai.md) is that design. It runs as a track beside the milestones: the work lives in its own crate (`gc_ai`) and tool (`gc_arena`), so a parallel thread can build it without touching code the engine thread is changing. Only a few small engine hooks have to land inside the milestones above, and two of them (team stances, copying a battle area for "Restage this battle") are needed anyway.

| Phase | What | With |
|---|---|---|
| AI-1 · Battle AI | Perception, terrain analysis, battle estimator, battle tactics with four doctrines, difficulty levels, drills | Starts once the sandbox lands; ships with M2. Built in three steps: (1) the battle AI itself, playable from the lobby and the sandbox; (2) terrain sense, the eight drills and the estimator check; (3) the AI lens and journal overlay. All three steps are built (step 1 on 2026-10-12, steps 2 and 3 on 2026-10-13) |
| AI-2 · Rehearsal | The AI plays candidate battle plans ahead in a copy of the real sim; the Warlord difficulty | M2. Built (2026-10-15): the Warlord reads the enemy's plan, rehearses its own and beats the General 81% of the time (195 of 240 arena battles) |
| AI-3 · Conquest AI | Strategy, economy, scouting, armies; Clans and Brood; AI allies; the arena | M3 |
| AI-4 · Relations and the living world | Relations ledger, hamlets, creep camps, mercenary companies, parley | M4 |
| AI-5 · Choir and Concord AI | Their economy modules and doctrines | M5 |
| AI-6 · Exploration and diplomacy | Factions met while exploring; diplomacy between players as an option | After 1.0 |

## The campaign track

On 2026-10-15 the Campaign was designed as a third mode ([19 · Campaign and Exploration](19-campaign.md)): a world map of the Reach played over several game nights, where you explore, meet friendly, neutral and mercenary peoples, and fight the battles that matter as Field Battles. Like the AI track it lives in its own crate (`gc_campaign`) and AI module, so a parallel thread can build it.

| Phase | What | With |
|---|---|---|
| C-0 · Exploring inside a match | Frontier sites, first sight and first contact on Conquest maps | M4 |
| C-1 · Campaign core | World map, seasons, hosts, exploring, Grubfolk and companies, battles fought or handed to the generals, saves; Clans and Brood | Starts once M4 lands, alongside M5 |
| C-2 · Peoples and diplomacy | Minor factions with minds (AI-6), diplomacy between players, alliances, guest commanders, the Chronicle | M7 |
| C-3 · The full campaign | All four factions, Fellowship mode, stronghold battles, Iron campaign | M7 |

### M7 · Across the Reach (first milestone after 1.0)
*Goal: the full campaign.*
- C-2 and C-3 above, on top of the C-1 core built alongside M5.
- **Exit:** four friends finish a Long campaign over several game nights with no desync, and want to start another.

## After 1.0 (unordered)

| Feature | Notes |
|---|---|
| Naval expansion | Water maps, ships, coastal raids; the map format already reserves a depth layer |
| Exploration and diplomacy | Now part of the campaign (M7 above): factions met while exploring, with their own minds and memories, and diplomacy between players (AI-6 in [18 · AI](18-ai.md#build-plan), C-2 in [19](19-campaign.md#build-plan)). Diplomacy between players in Conquest matches stays a later lobby option |
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
