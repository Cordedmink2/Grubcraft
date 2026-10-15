# 14 · Decisions Needed

The calls only Mario can make. Each has a recommendation; work proceeds on the recommendation unless told otherwise. Once decided, record the outcome here (and in an ADR for technical ones).

> **Decided 2026-10-12:** Mario accepted every recommendation in the table below (decisions 1 to 12). The "Smaller questions for later" remain open.

| # | Decision | Outcome (accepted recommendation) | Why it matters |
|---|---|---|---|
| 1 | **Engine and language** | Custom engine in Rust on `wgpu` ([ADR 0001](adr/0001-engine-and-language.md)) | Sets everything else; the memory target is only realistic if we own the floor |
| 2 | **Units as regiments in Conquest too** (not only in Field Battle) | Yes, regiments everywhere | Gives the Total War feel in the main mode, reduces clicking, and makes the performance plan work |
| 3 | **Setting and tone**: the fallen Titan, Grub, "whimsical-grim" | Go with it | Names, art and audio all follow from it |
| 4 | **The four factions** (Ember Clans, Brood, Choir, Carrion Concord) | Keep all four; build Clans and Brood first | Faction work is the biggest content cost |
| 5 | **Art style**: stylized low-poly, palette-textured | Yes | It's both the look and a core memory strategy |
| 6 | **Who makes art and audio** | Start with CC0 kits (Quaternius, Kenney) and royalty-free audio; add custom models for key units later; consider friends recording voice lines | No art means no game; this unblocks M1 |
| 7 | **Field Battle first** in the build order | Yes (M1–M2), Conquest in M3 | De-risks determinism, combat and netcode with the smallest scope |
| 8 | **Internet play** | LAN plus a virtual-LAN tool (Tailscale/ZeroTier) at first; built-in hole punching later | Avoids running any servers |
| 9 | **Default match length** | Standard tempo, aiming at 25 to 40 min for 1v1 and 40 to 60 min for 4 players | Drives economy numbers and map sizes |
| 10 | **Titan's Heart victory on by default** | Yes, unlocking at minute 20 | Guarantees games end; can be turned off |
| 11 | **Mac code signing** | Ad-hoc signing; friends approve first launch once | Notarisation needs a paid Apple developer account |
| 12 | **Who tests on Windows** | Mario's or a friend's Windows PC for each milestone's game night, plus CI runners | CI has no real GPU; frame budgets need real hardware |

## AI and the living world (proposed 2026-10-12)

From the [AI design](18-ai.md). Work proceeds on these recommendations unless Mario says otherwise.

| # | Decision | Recommendation | Why it matters |
|---|---|---|---|
| 13 | **Bring AI players forward** from "after 1.0" | Yes: a battle AI with Field Battle (M2), a Conquest AI with Conquest (M3), built as a parallel track | Practice and solo play long before 1.0; Mario asked for an intelligent AI now |
| 14 | **How difficulty works** | Four levels (Recruit, Veteran, General, Warlord) that differ in how well they think, never in cheating; a separate handicap (income 50% to 150%) for any slot | A fair AI is one you can learn from; the handicap is the honest way to balance mixed skill |
| 15 | **Mercenary companies** as their own parties (bands with a captain, a price and a memory), beyond hamlet militia | Yes, in M4 | The first piece of "encountering people" on the map; tests the relations system early |
| 16 | **Diplomacy between players** (ceasefires, alliances, tribute in free-for-all) | Later, as a lobby option, off by default | It changes how free-for-all plays; the relations system supports it whenever it's wanted |
| 17 | **Exploration content**: factions met while exploring (friendly, neutral, hostile) with their own minds | After 1.0, on the same relations system; sooner if Mario wants exploration to lead | The design keeps the door open; what those peoples are like is a creative call |

## Campaign and exploration (proposed 2026-10-15)

From the [campaign design](19-campaign.md). Mario asked for it on 2026-10-15; work proceeds on these recommendations unless he says otherwise.

| # | Decision | Recommendation | Why it matters |
|---|---|---|---|
| 18 | **A campaign map**, which the vision had ruled out | Yes, as a third mode kept light: a world map that gives battles a reason and the peoples of the Reach somewhere to live; no empire management. The vision's non-goal now reads "no empire management" | Exploring and meeting people become central, as Mario asked |
| 19 | **How campaign turns work** | Simultaneous seasons: everyone plans at once (90 s, ends early when all are Ready), then every move plays out together | Nobody waits for three friends' turns |
| 20 | **How campaign battles are fought** | Fight it as a Field Battle, or let the generals fight it: the battle AI plays every side in the real simulation, in seconds, with a replay | Every battle can be played, none has to be, and quick results are still real battles |
| 21 | **When the campaign is built** | Exploring inside a match with M4 (C-0); the campaign core from after M4, alongside M5 (C-1); peoples, diplomacy and all four factions as M7, after 1.0 | Exploration arrives early without delaying 1.0; moving C-2 and C-3 before 1.0 would push 1.0 back about one milestone |
| 22 | **Diplomacy between players in the campaign** | On by default (ceasefires, alliances, tribute, and the reputation cost of breaking them); fixed teams as an option | Promises kept and broken over weeks are what a campaign adds |
| 23 | **Who lives in the Reach** | Start from the first drafts: Grubfolk, the Bazaar, three mercenary companies, the Heartbound (Titan cult), the Saltwander Caravan (nomad traders), the Discordant (Choir splinter), Titan-spawn lairs | A creative call; the systems work with any cast |

Decision 17 (exploration content after 1.0) is refined by 21: factions met while exploring arrive with the campaign's C-2 phase.

## Smaller questions for later
- Should allied players share supply or resources by default in team games?
- Should Field Battle have a campaign-lite "ladder" among friends (persistent army veterancy across battles)? *Recommended: fold it into the campaign; a Short campaign with only Field Battles covers it ([19](19-campaign.md#open-questions)).*
- Is there appetite for a fifth faction before naval play, or naval first?
- Friendly-fire for all ranged units in Field Battle (more Shogun, less forgiving)?
- Should an AI remember a human's habits across matches (a small local profile per player name)? See [18 · AI](18-ai.md#open-questions) for this and two other AI questions.
