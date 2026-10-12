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

## Smaller questions for later
- Should allied players share supply or resources by default in team games?
- Should Field Battle have a campaign-lite "ladder" among friends (persistent army veterancy across battles)?
- Is there appetite for a fifth faction before naval play, or naval first?
- Friendly-fire for all ranged units in Field Battle (more Shogun, less forgiving)?
