# 15 · Staffing Plan

Who builds what: which work goes to Opus on its own, Opus leading Haiku 5.5 helpers, Sonnet 5.5, parallel threads ("agent teams") and subagents, mapped across every milestone in the [Roadmap](13-roadmap.md).

This is a plan, not a switch. Nothing here changes which model a running thread uses; Mario picks models when threads start, and the coordinator follows this plan when it suggests one.

## The short version

- **Opus owns anything that can silently break a multiplayer game.** That is the deterministic sim (`gc_sim`, `gc_fixed`), netcode (`gc_net`), combat and economy math, save/replay formats, architecture, and review of every PR that touches them.
- **Sonnet owns well-scoped features behind a stable interface.** UI screens, renderer features, audio, the asset pipeline, tools, maps, packaging and platform fixes.
- **Haiku does mechanical work to a clear spec, always under a lead.** CI log triage, tests written from a spec, RON data entry, docs sync, benchmark runs, screenshots, licence checks.
- **One Opus engine thread through M1 and M2**, because almost every step changes the sim and two threads editing it would fight. **Parallel threads from M3**, once the sim's interfaces settle and the work splits along crate lines.

## The roster

### The five ways to staff a piece of work

| Shape | What it is | Use it for | Avoid it for |
|---|---|---|---|
| **Opus alone** | One Opus thread does the whole task itself | Hard, judgment-heavy work where the thinking *is* the work: tick pipeline order, lockstep protocol, combat formulas, desync debugging | Work that is mostly typing; it wastes the expensive model on volume |
| **Opus + Haiku helpers** | Opus plans, splits the task into precise specs, sends Haiku subagents to execute in parallel, then reviews every result | Sim features with lots of surrounding volume: Opus writes the core, Haiku writes the 40 unit tests, the RON entries and the bench runs | Anything where the spec can't be written down precisely; Haiku will guess |
| **Sonnet lead** | One Sonnet thread owns a feature end to end | A feature with a clear contract to the sim: a lobby screen, the minimap, the audio mixer, `gc_mapcheck` | Changing sim rules, net protocol or hashed state |
| **Sonnet + Haiku helpers** | Same as Opus + Haiku, one tier down | Large but straightforward features: settings screen plus 60 rebindable actions, codex entries, map content | Same as Sonnet lead |
| **Agent team (parallel threads)** | Several threads in the project at once, each with its own lead model, each owning different crates | M3 onward, when work splits cleanly: sim, UI, maps, audio | M1–M2; the sim is still moving under everyone's feet |

### Subagents inside a thread

A thread lead spawns subagents for focused side tasks and gets back a summary, which keeps its own context clean. The built-in types, plus a small set of custom ones worth adding to `.claude/agents/` when the first parallel threads start (M3):

| Subagent | Model | Job | Writes code? |
|---|---|---|---|
| `Explore` (built-in) | Haiku | Find where something lives across crates; answer "who calls this?" | No |
| `Plan` (built-in) | Opus | Second opinion on an approach before a large change | No |
| `sim-reviewer` (proposed) | Opus | Reviews any diff touching `gc_sim`, `gc_fixed`, `gc_net`, `gc_data` schema or replay format: determinism, hashing, integer overflow, iteration order | No |
| `test-writer` (proposed) | Haiku | Writes unit tests from a spec the lead supplies (inputs, expected outputs, edge cases) | Tests only |
| `ci-triage` (proposed) | Haiku | Reads a red CI run, finds the first real error, says which crate and test, suggests whether it's ours or flaky | No |
| `data-entry` (proposed) | Haiku | Fills `data/*.ron` from a table in the design docs; runs `gc_bake` to prove it compiles | Data only |
| `bench-runner` (proposed) | Haiku | Runs `gc_bench` scenarios, collects numbers into a table, flags budget breaches | No |
| `screenshotter` (proposed) | Haiku | Launches the game headless (Xvfb + lavapipe), drives it with xdotool, captures the shots a PR needs | No |
| `docs-sync` (proposed) | Haiku | Updates docs progress notes and links after a merge; checks docs against `data/` canonical values | Docs only |
| `feature-builder` (proposed) | Sonnet | A bounded, non-sim feature inside an Opus thread, such as the sandbox's placement UI | Yes, outside sim crates |
| `arena-runner` (proposed) | Haiku | Runs `gc_arena` AI-against-AI ladders and drills, tabulates win rates by difficulty, faction and personality, flags regressions | No |

Subagents that write code in parallel run with **worktree isolation**, so two of them never edit the same checkout.

### Hard rules

1. **Crate ownership.** `gc_fixed`, `gc_sim`, `gc_net`, the replay format and the `gc_data` schema are Opus-owned. A Sonnet thread may *read* them and call their public API; a change to them goes through an Opus thread or gets an Opus `sim-reviewer` pass before merge.
2. **Haiku never edits non-test code in Opus-owned crates.** It may add tests and data there.
3. **One writer per crate at a time.** Two threads can run in parallel only if their PRs touch different crates (or one only adds files). The coordinator checks this before starting a parallel thread.
4. **Golden replay hashes change only on purpose**, re-recorded in the same PR, and only an Opus lead or reviewer signs that off.
5. **Escalate on second failure.** A Haiku helper that fails the same task twice hands it back to its lead. A Sonnet thread that finds it needs a sim change stops and asks for an Opus change rather than working around it.
6. **`gc_ai` is Opus-led but can't break the game.** AI minds only send commands, so a bad AI change makes a weaker opponent, never a desync. Opus owns the AI's architecture, the battle estimator and tactics; Sonnet may add behaviours, personalities, openings and the AI lens through its interfaces. The relations ledger and the rule that answers offers are hashed sim state, so they follow rule 1.
7. **Mechanical backstops let cheaper models move faster.** The no-floats-in-sim lint, cross-platform golden replays and `gc_bench` budget gates catch the mistakes that matter most, so CI, not trust, is what makes a Sonnet or Haiku PR safe.

### Effort level

Effort is set per thread, separately from the model. Default to the model's normal effort; ask for high or max effort only for desync hunts, the lockstep protocol, combat-formula tuning and performance investigations where the first answer is rarely right.

The AI is the other place where effort pays: its design ran at extra-high effort (2026-10-12), and the battle tactics and estimator should too. Keep max effort for the diplomacy logic (the relations ledger and the rule that answers offers), where a subtle mistake makes every faction behave strangely.

## Milestone by milestone

Each table lists every workstream in the roadmap milestone, who leads, who helps, and why.

### M0 · Foundations (done, one item open)

| Workstream | Lead | Helpers | Notes |
|---|---|---|---|
| Windows PC run (exit criterion) | **Mario** or a friend | Haiku `ci-triage` reads any crash log | Needs real hardware; no model can do it |

### M1 · Regiments on a field: one Opus engine thread

The engine thread (currently running) stays Opus and builds the eight steps in order, one PR each. Inside it, helpers take the volume.

| Step | Lead | Helpers | Why this split |
|---|---|---|---|
| 1 Formations in the sim | Opus (merged) | | |
| 2 Soldier puppets | Opus (merged) | | |
| 3 Controls: box select, right-drag frontage, control groups | Opus (merged) | Haiku `screenshotter`, `test-writer` for `group_move` cases | Order expansion feeds the sim; the input plumbing is routine |
| 4 Melee, volleys, facing multipliers, charges | **Opus alone** for formulas and tick order (merged) | Haiku `test-writer` (formula tables), `data-entry` (unit combat stats) | The heart of "feels like Shogun", and all hashed state |
| 5 Morale and routing | **Opus alone** (merged) | Haiku `test-writer`, `bench-runner` | Cascading rout is emergent and easy to get subtly wrong |
| 6 Pathfinding: nav grid, sectors, flow fields | Opus (merged; waypoint routes rather than flow fields, see 10 · Architecture) | Haiku `bench-runner` (path cost per tick), `test-writer` (grid edge cases) | Deterministic and performance critical at once |
| 7 Sandbox: place armies, slow-mo, hot-reload stats | Opus (merged; built in one thread, panels on the overlay text until the `gc_ui` widgets land) | **Sonnet `feature-builder`** for moving the panels to `gc_ui` | First clean seam where Sonnet can build inside the thread |
| 8 Real art: CC0 kits, bone animation textures, Commander View | Opus for the VAT format and renderer integration (in review; built in one thread, with a Rust glTF baker `gc_artbake` instead of a Blender script) | Sonnet `feature-builder` for the Blender export script and `gc_bake` importer; Haiku sources CC0 packs and logs licences | The pipeline is a well-scoped tool; licence checks are pure bookkeeping |
| Golden replays and `field-8000pt-2v2` benchmark | Opus designs the scenario | Haiku records goldens, wires the bench into CI | |
| Exit: 3,000-soldier sandbox at 60 fps | Opus investigates frame time | Haiku `bench-runner`; **Mario** judges the feel | "Feels like Shogun" is Mario's call |

**Optional low-conflict side thread during M1:** a Haiku-helped Sonnet thread that writes `data/` entries for the Ember Clans and Brood prototype rosters from [03 · Factions](03-factions.md), in its own files only. Worth starting only if the engine thread ever waits on data. **Done for all four factions** during M1 (an Opus thread designing, Haiku helpers transcribing the Brood, Choir and Concord data): [17 · Unit Rosters](17-unit-rosters.md) and `data/rosters/`.

### M2 · LAN Battle: still one Opus engine thread, Sonnet joins late

| Workstream | Lead | Helpers | Notes |
|---|---|---|---|
| `gc_net`: UDP transport, encryption, host-sequenced lockstep, adaptive input delay | **Opus alone** | Haiku `test-writer` for packet codec round-trips | The highest-risk system in the game; also gets a dedicated security review (Opus `Plan` subagent) of the encryption |
| Network chaos test (latency, jitter, loss on loopback) | Opus | Haiku runs the soak and summarises | |
| Desync detection and report files | **Opus alone**, high effort | | Debugging a desync is the archetypal Opus task |
| Cross-platform determinism CI on golden replays | Opus decides what's hashed | Sonnet writes the macOS/Windows CI jobs; Haiku `ci-triage` | |
| Replays: record and play back | Opus (format is versioned and hashed) | Haiku tests for round-trip | |
| LAN discovery, Host/Join screens, lobby, version gate | Opus defines the lobby protocol | **Sonnet** builds the screens, as a `feature-builder` subagent or a second thread once the protocol is frozen | First point where a parallel Sonnet thread is reasonable: it only touches `gc_ui` and `gc_game`. **Started early (2026-10-12):** a Sonnet thread built the screens against a stub `Network` trait; `gc_net` still has to implement it |
| Field Battle mode: army builder, deployment, victory conditions | Opus for victory rules and deployment in the sim | Sonnet for the army builder UI; Haiku `data-entry` for point costs | |
| Two battle maps | Sonnet | Haiku validates with `gc_mapcheck` once it exists | |
| Packaging: Mac `.app` (ad-hoc signed), Windows `.exe`, zips | **Sonnet** | Haiku writes the release checklist | Platform plumbing with a clear finish line |
| Team stances: one hostility test reading a stance table (AI engine hook 1) | Opus engine thread | | Needed for the 2v2 exit anyway; it later becomes the relations ledger. **Done (2026-10-12)** by the battle AI thread, as a small change reviewed with AI-1 step 1, since the AI needed teams to tell friend from foe |
| Battle AI (`gc_ai`, AI-1 and AI-2 in [18 · AI](18-ai.md#build-plan)) | **Opus, extra-high effort**, as a parallel thread | Haiku `test-writer` writes the drills; Sonnet `feature-builder` builds the AI lens overlay | Safe to run beside the engine thread: it adds a new crate and only reads the sim's public API. The rehearsal hooks (copy a battle area, spawn with state) go through the engine thread. **Step 1 of 3 built (2026-10-12)** by an Opus extra-high-effort thread: the crate, tactics, difficulty levels, personalities, and the lobby and sandbox wiring. **Step 2 built (2026-10-13)** by the same thread, drills included (written by the Opus thread itself, as they shaped the tactics) |
| Exit: 2v2 over home Wi-Fi, zero desyncs | Opus reads desync reports | **Mario and friends** play it | |

### M3 · First Harvest: the first agent team

The sim's shape is settled after M2, so work splits into four parallel threads, each owning different crates. The coordinator starts them; an Opus reviewer covers every sim-touching PR.

| Thread | Lead | Owns | Workstreams |
|---|---|---|---|
| **A · Economy and sim core** | Opus | `gc_sim`, `gc_data` schema | Workers, Grub seeps and Marrow veins, structures and construction, production queues, supply and upkeep, tech tiers, fog of war (sim side), Annihilation and Titan's Heart victories. Haiku `test-writer` and `data-entry` throughout |
| **B · Factions and heroes** | Opus | Faction modules in `gc_sim`, `data/` faction files | Ember Clans economy and Honor; Brood veins, larvae, metamorphosis, Reclaimers; heroes, levelling, the effect-primitive ability system, items, altar revive. Starts after A lands the structure and production APIs; Haiku fills ability and item data |
| **C · Conquest UI** | Sonnet | `gc_ui`, UI parts of `gc_render`, `gc_game` | Minimap, army tray, command grid, edge notifications, fog rendering, hero panel. Reads sim state through the presentation layer only |
| **D · Maps and tools** | Sonnet | `tools/gc_mapcheck`, `data/maps/` | `gc_mapcheck` validator, Ribcage Pass (Duel), The Four Teeth (Warfield); Haiku runs fairness checks |
| **E · Conquest AI** | Opus | `gc_ai`, `tools/gc_arena`, `data/ai/` | Strategy, economy and scouting layers, Clans and Brood economy modules, armies, AI allies answering pings (AI-3). Builds on A's and B's APIs as they merge; Sonnet writes openings and personalities as data; Haiku `arena-runner` runs the ladders |
| Exit: full 4-player Conquest within 4 GB | Opus (thread A) | Haiku `bench-runner` across a 60-minute soak | |

A and B both touch `gc_sim`, so they split by module and B starts once A's interfaces are merged. If they collide, B pauses rather than both editing the same file.

### M4 · The Reach Lives: widest team

| Thread | Lead | Workstreams |
|---|---|---|
| **A · World systems** | Opus | Grubfolk hamlets (market, mercenaries, rumours, raiding, reputation), creep camps and items, Shrines, Titan Events (Tremors and Bloodseeps, Marrow Bloom, Heartbeat), "Restage this battle" (snapshot and re-seed) |
| **B · Learnability** | Sonnet | Tutorial "First Harvest", Clans and Brood primers, contextual hints, codex. Opus reviews any tutorial scripting hook added to the sim; Haiku drafts codex entries from the design docs |
| **C · Audio** | Sonnet | `gc_audio`: unit voices, faction themes, battle ambience (the mixer, first sound set and adaptive music were built early, as a separate Sonnet thread). Haiku sources and licence-checks royalty-free audio |
| **D · Maps and post-game** | Sonnet | Hollow Lung and Marrowfen, map validator improvements, post-game screen with graphs |
| **Balance pass** | Opus | Haiku helpers run hundreds of headless matches through `gc_replay` with scripted players and tabulate win rates; Opus reads the numbers and proposes changes; **Mario** and friends confirm by playing |
| **E · Relations and the living world** | Opus | The relations ledger, contracts and offer rule in `gc_sim` (with thread A, which owns hamlets and creeps), mercenary company minds and the Envoy in `gc_ai` (AI-4). Sonnet builds the parley panel; Haiku enters company rosters and relation events as data |
| Exit: a newcomer learns and wins after the tutorial | | **A real newcomer** has to try it |

### M5 · Choir and Concord: two faction threads

| Thread | Lead | Workstreams |
|---|---|---|
| **A · The Choir** | Opus | Resonators, chord geometry and harmony (deterministic triangle maths), Resonance, Phase Shift, Refraction. Haiku tests geometry edge cases |
| **B · The Carrion Concord** | Opus | Caravans and trade routes, Reliquaries and Tithe, raising the dead, crumbling undead. Haiku data entry |
| **C · Primers and presentation** | Sonnet | Primers for both factions, faction UI, effects. Starts once A and B have playable units |
| **Balance across all pairings** | Opus | Haiku batch-sims every pairing; Opus proposes changes against the "no faction above 60%" exit |
| **D · Choir and Concord AI** | Opus | Their economy modules and doctrines in `gc_ai` (AI-5); starts once A and B have playable units. Haiku `arena-runner` checks each holds 40 to 60% |

A and B both live in `gc_sim` but in separate faction modules; shared hooks they both need are added first, in one small Opus PR, before the two threads start.

### M6 · Production polish (1.0)

| Workstream | Lead | Helpers | Notes |
|---|---|---|---|
| Disconnect handling, rejoin, multiplayer save and load | **Opus** | Haiku chaos and soak runs | Rejoin and save both serialise hashed state |
| Settings completeness, key rebinding, accessibility | **Sonnet** | Haiku enumerates every action and setting from the docs | |
| Performance pass against every budget; stretch targets | **Opus**, high effort | Haiku `bench-runner` sweeps | Hill-climbing per [12 · Performance](12-performance.md#hill-climbing-process) |
| Art pass: custom models for key units and heroes | **Human artist** (see decision 6) | Sonnet keeps the pipeline healthy; Haiku checks every model against the memory and triangle budgets | |
| Self-update check | Sonnet | | Small, non-sim |
| Release: tag 1.0, notes, packages | Sonnet | Haiku drafts release notes from merged PRs | |
| Exit: a month of game nights, no crash or desync | Opus on any desync report | **Mario and friends** | |

### After 1.0

| Feature | Lead | Notes |
|---|---|---|
| Exploration and diplomacy (AI-6) | Opus, max effort for diplomacy logic | Factions met while exploring are mostly content (rosters, personalities, homes) that Sonnet can add once the minds exist; Haiku runs the arena |
| Host migration | Opus | Protocol work |
| Internet join codes (hole punching, rendezvous service) | Sonnet | Opus security review of the rendezvous protocol |
| Player map editor | Sonnet | Exposes the internal edit mode |
| Seeded map generator | Opus | Fairness is a hard maths problem; validated by `gc_mapcheck` |
| Co-op Siege mode | Sonnet | Opus for any new sim rules |
| Naval expansion | Opus | Water pathing and new movement rules |
| Fifth faction | Same shape as M5 | |

## Running across every milestone

| Chore | Who | When |
|---|---|---|
| Red CI on a PR | Haiku `ci-triage` reads it; the PR's lead fixes it | Every failure |
| PR review | Opus `sim-reviewer` for Opus-owned crates; the thread's own lead for the rest | Every PR |
| Benchmarks and budget gates | Haiku `bench-runner`; Opus investigates any breach | Every sim or render PR |
| PR screenshots for Mario | Haiku `screenshotter` | Every visible change |
| Docs progress notes, roadmap updates | Haiku `docs-sync` | After each merge |
| Dependency and licence audit (`cargo deny`, `cargo bloat`, asset licences) | Haiku | Weekly, and before each milestone exit |
| Architecture decisions (ADRs) | Opus | When a choice is hard to reverse |
| Coordination: starting threads, checking crate ownership, relaying to Mario | Coordinator | Ongoing |

## What only Mario (and friends) can do

- Play-test feel: "does this look and feel like Shogun?" is the M1 exit and no benchmark answers it.
- Run builds on real Windows hardware (M0 exit and every game night).
- Game nights, which are the real milestone reviews.
- Merge approvals, art direction calls and the open questions in [14 · Decisions Needed](14-decisions-needed.md).
- Bring in a human artist or voice actors for the M6 art pass.

## Deciding a new task in ten seconds

1. Does it change hashed sim state, the net protocol, a save or replay format, or a combat or economy formula? **Opus.**
2. Is it a feature with a clear contract to the sim (UI, render, audio, tools, maps, packaging)? **Sonnet.**
3. Could someone finish it from a precise written spec without making a design choice? **Haiku, under whichever lead owns the area.**
4. Does it touch a crate another running thread is changing? **Queue it in that thread instead of starting a new one.**
