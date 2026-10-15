# 02 · Specification

The consolidated, testable list of what Grubcraft must do. Each requirement has an ID for issues and PRs to reference, a priority, and the milestone that delivers it ([13 · Roadmap](13-roadmap.md)). Design detail lives in the linked documents; this file is the checklist.

**Priority:** **M** = must have for 1.0 · **S** = should have for 1.0 · **C** = could have, after 1.0

## Platform and distribution

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| PLT-1 | Runs on macOS 12+ on Apple Silicon and Intel (universal binary) | M | M0 |
| PLT-2 | Runs on Windows 10 and 11, x64 | M | M0 |
| PLT-3 | Mac and Windows players can play in the same match | M | M2 |
| PLT-4 | Distributed as one zip per platform; no installer, no account, no launcher | M | M2 |
| PLT-5 | Full support for Mac trackpads (pan, pinch zoom, rotate) as well as mouse | M | M0 |
| PLT-6 | Game settings, replays and saves stored in the OS's standard per-user app-data folder | M | M2 |
| PLT-7 | Linux build | C | — |

## Performance (see [12 · Performance](12-performance.md))

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| PRF-1 | Peak memory ≤ 4 GB in a 4-player 60-minute match on the largest map | M | M3 |
| PRF-2 | 60 fps at 1080p on the low-tier reference machines during a large battle | M | M1 |
| PRF-3 | Sim tick p99 ≤ 2 ms on low-tier hardware in a 4-player late game | M | M3 |
| PRF-4 | Launch to main menu ≤ 1 s; lobby start to first match frame ≤ 3 s | M | M3 |
| PRF-5 | Zero heap allocations inside a simulation tick, enforced by test | M | M1 |
| PRF-6 | Per-subsystem memory accounting visible in a dev overlay and enforced by CI benchmarks | M | M0 |
| PRF-7 | CI fails on > 5% regression in tracked benchmark metrics | M | M1 |
| PRF-8 | Battery preset that caps frame rate and lowers GPU load for laptops | S | M3 |

## Networking (see [11 · Networking](11-networking.md))

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| NET-1 | 2 to 4 players over a local network, one player hosting on their own machine | M | M2 |
| NET-2 | Hosts on the same network appear automatically in the Join list | M | M2 |
| NET-3 | Join by IP address or join code | M | M2 |
| NET-4 | Deterministic lockstep simulation, identical on all platforms | M | M2 |
| NET-5 | Adaptive input delay; command latency ≤ 100 ms on typical home Wi-Fi | M | M2 |
| NET-6 | Lobby rejects mismatched game builds or data, naming both versions | M | M2 |
| NET-7 | Desync detection with automatic resync and a saved desync report | M | M2 |
| NET-8 | Rejoin a running match after a disconnect | S | M6 |
| NET-9 | Multiplayer save and resume | S | M6 |
| NET-10 | Encrypted, authenticated traffic | S | M2 |
| NET-11 | Host migration | C | — |
| NET-12 | Internet play without extra software (hole punching) | C | — |

## Game modes (see [06 · Game Modes](06-game-modes.md))

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| MOD-1 | Conquest mode: base building, economy, tech, armies; 1v1, 2v2, FFA up to 4, uneven teams | M | M3 |
| MOD-2 | Conquest victory: Annihilation and Titan's Heart, configurable in the lobby | M | M3 |
| MOD-3 | Conquest options: Tempo (Swift, Standard, Epic), Titan Events on/off | M | M4 |
| MOD-4 | Field Battle mode: points-based army builder, deployment phase, rout and victory-point wins | M | M2 |
| MOD-5 | Saved army lists and faction presets in the army builder | S | M4 |
| MOD-6 | Solo sandbox with free unit placement and slow motion | M | M1 |
| MOD-7 | "Restage this battle" from a Conquest result into Field Battle | S | M4 |
| MOD-8 | Regicide and Score victory options | C | — |
| MOD-9 | AI players in Field Battle and Conquest (see the AI section below) | S | M2/M3 |

## AI and relations (see [18 · AI](18-ai.md))

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| AI-1 | Battle AI for Field Battle and the sandbox: picks an army, deploys, forms a line, flanks, plays the morale game, decides to engage or withdraw | S | M2 |
| AI-2 | AI plays fair: sees only what its player sees, same rules and command delay, no cheats at any difficulty | M | M2 |
| AI-3 | AI runs as a player issuing commands on the host; it can never desync a match or change replays and golden hashes | M | M2 |
| AI-4 | Four difficulties (Recruit, Veteran, General, Warlord) and a handicap (income 50% to 150%) for any slot | S | M2 |
| AI-5 | Personalities (two per faction), shown in the lobby | S | M2/M3 |
| AI-6 | Conquest AI for Ember Clans and Brood; AI allies answer pings | S | M3 |
| AI-7 | Relations ledger (stance, liking, trust, fading memories) between all parties; hamlet reputation built on it | M | M4 |
| AI-8 | Mercenary companies: hire, serve, desert, word of mouth; parley panel with previewable answers | S | M4 |
| AI-9 | Conquest AI for Choir and Carrion Concord | S | M5 |
| AI-10 | AI takes over a disconnected player (lobby option) | C | M6 |
| AI-11 | Factions met while exploring; diplomacy between players as a lobby option | C | M7 |
| AI-12 | Headless AI arena: each difficulty beats the one below at least 70% of the time | S | M3 |

## Exploration and campaign (see [19 · Campaign and Exploration](19-campaign.md))

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| EXP-1 | Conquest maps have 4 to 8 frontier site slots, filled from the match seed, that reward whoever scouts them first | S | M4 |
| EXP-2 | First sight of a site, hamlet or company is a ledger event (first contact) | S | M4 |
| CMP-1 | Campaign mode: a world map of the Reach from a body template and a seed, played over several sessions by 1 to 4 players and AI players, saved after every season on every player's machine | C | C-1 / M7 |
| CMP-2 | Simultaneous seasons: everyone plans at once with an optional timer, then every party's moves resolve together | C | C-1 |
| CMP-3 | Campaign battles are Field Battles with the hosts' exact regiments on a map generated from the region; or the battle AI fights every side in the real simulation at full speed, leaving a replay | C | C-1 |
| CMP-4 | One relations ledger for the whole campaign, memories fading by season; battles start from it and hand their memories back | C | C-1 |
| CMP-5 | Friendly, neutral, mercenary and hostile peoples on the campaign map, with minds where they make choices | C | C-1 / M7 |
| CMP-6 | Diplomacy between players in the campaign (ceasefire, alliance, tribute, call to arms), on by default | C | M7 |
| CMP-7 | Guest commanders: players not in a battle can command its other sides | C | M7 |
| CMP-8 | Campaign state under 1 MB; deterministic and hashed every season on all platforms | C | C-1 |

## Factions, units and combat (see [03 · Factions](03-factions.md), [05 · Combat](05-combat.md))

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| FAC-1 | Ember Clans and Brood fully playable | M | M3 |
| FAC-2 | Choir and Carrion Concord fully playable | M | M5 |
| FAC-3 | Each faction has a distinct economy, one faction meter and one signature rule | M | M3/M5 |
| FAC-4 | Three heroes per faction with levels 1 to 10, abilities and items | M | M3/M5 |
| CMB-1 | Units are regiments of multiple models commanded as one, with per-model HP | M | M1 |
| CMB-2 | Formations with right-drag to set position, frontage and facing | M | M1 |
| CMB-3 | Group moves keep formation and arrange melee/ranged/cavalry sensibly | M | M1 |
| CMB-4 | Facing matters: flank and rear damage multipliers and morale shock | M | M1 |
| CMB-5 | Morale with Steady, Wavering, Routing, Shattered; rallying | M | M1 |
| CMB-6 | Charges, bracing, skirmish behaviour | M | M1 |
| CMB-7 | Terrain effects: high ground, forest, grass, fords, roads, bogs | M | M1 |
| CMB-8 | Air units for every faction | M | M3 |
| CMB-9 | Reinforcement of damaged regiments and veterancy | M | M3 |
| CMB-10 | Naval units and water maps | C | — |

## Economy and world (see [04 · Economy](04-economy.md), [07 · World and Maps](07-world-and-maps.md))

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| ECO-1 | Two resources (Grub, Marrow), supply cap 150, upkeep bands | M | M3 |
| ECO-2 | Seeps and veins deplete to dregs at 25% rate | M | M3 |
| ECO-3 | Three tech tiers per faction | M | M3 |
| WLD-1 | Grubfolk hamlets: market, mercenaries, rumours, raiding, reputation | M | M4 |
| WLD-2 | Creep camps with hero experience and item drops | M | M4 |
| WLD-3 | Shrines and the Titan's Heart | M | M3 |
| WLD-4 | Titan Events: Tremors/Bloodseeps, Marrow Bloom, Heartbeat | S | M4 |
| MAP-1 | Four launch Conquest maps and four Field Battle maps | M | M4 |
| MAP-2 | Map validator for fairness | S | M4 |
| MAP-3 | Player-facing map editor; seeded map generator | C | — |

## UI and UX (see [08 · UI and UX](08-ui-ux.md))

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| UI-1 | HUD shows only Grub, Marrow, supply, faction meter, clock | M | M3 |
| UI-2 | Fixed 4×3 command grid with consistent hotkeys across all units | M | M1 |
| UI-3 | Army tray of regiment cards with model count, morale, veterancy | M | M1 |
| UI-4 | Commander View at far zoom | M | M1 |
| UI-5 | Minimap with pings; ping wheel; text chat | M | M3 |
| UI-6 | Edge notifications and idle-worker button | M | M3 |
| UI-7 | Tutorial (≈ 10 min) and a primer per faction | M | M4/M5 |
| UI-8 | Codex with stats for every unit, structure and mechanic | S | M4 |
| UI-9 | Post-game summary with graphs and battle list | S | M4 |
| UI-10 | Full key rebinding, UI scale, colour-blind palettes | M | M6 |
| UI-11 | A new player can start a match and act meaningfully within 5 minutes of first launch | M | M4 |

## Replays and tooling

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| RPL-1 | Every match automatically saved as a replay (≤ 150 KB per hour) | M | M2 |
| RPL-2 | Replay viewer with free camera, speed control and seeking | S | M4 |
| TL-1 | Headless replay runner verifying state hashes | M | M1 |
| TL-2 | Golden replays run on macOS and Windows in CI with identical hashes | M | M2 |
| TL-3 | Benchmark suite with budget gates | M | M1 |
| TL-4 | Asset baker producing a single memory-mappable pack | M | M0 |

## Art and audio (see [09 · Art and Audio](09-art-and-audio.md))

| ID | Requirement | Pri | Milestone |
|---|---|---|---|
| AV-1 | Stylized low-poly, palette-textured art; team colour by palette swap | M | M1 |
| AV-2 | Every unit type readable by silhouette at default zoom | M | M3 |
| AV-3 | Adaptive music per faction; unit acknowledgement voices | S | M4 |
| AV-4 | Maximum 32 simultaneous audio voices with priority | M | M4 |
