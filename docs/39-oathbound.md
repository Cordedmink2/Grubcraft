# 39 · The Oathbound (fifth faction) and the Brood re-theme

Status: first pass, placeholder art. Data, rules and plumbing are in; the Meshy models are follow-up
swaps (below). Original IP only: nothing here comes from any existing game, film or book.

## The Oathbound

A timber-and-stone forest kingdom held together by sworn oaths. Meter: **Oaths** (baked as the existing
Honor machinery, so there is **no `gc_sim`, `SIM_VERSION` or protocol change**). Economy A (carry and
store): Seedtenders carry Grub to the Rootwatch Hall, a Hearthhouse or a Grove. Wall line: Thornwall,
Stonebough Wall, Crown Wall; Oathgate, Warden's Gatehouse; Oath Stakes and Hurdle Barricade.

- Roster: `data/rosters/oathbound.ron` (3 tiers, 11 trainable types plus the Briar Hounds summon, 3 heroes,
  22 buildings). Stat blocks are copied from the nearest Ember or Brood template so the repo's roster
  design-rule tests (`cargo test -p gc_bake`) hold on it as on the other four factions.
- Guide: `data/guide/oathbound.ron`. AI: two personalities and openings in `data/ai/`.
- Not built yet (needs new simulation, deliberately out of this pass): growing Groves (economy B), the
  Crown Wall morale twist, the Oath of the Wall power effect.

## The Brood re-theme

Layers (so Mario can accept one without the others):

| Layer | What | Rules? |
|---|---|---|
| L1 Look | Re-draw the 15 existing models and 10 buildings as ant-plus-second-animal hybrids; same names, stats and clips | no |
| L2 Lore | New lore text; the Brood as a "swarm that learns" (kills are eaten and grafted into the next brood) | no |
| L3 Roster | Overseer, Graft Stalkers, Gloomwings, Boilback, Graft Chamber, Carrion Larder (+ optional Graft meter) | yes, needs Mario |

This PR is **catalogue-only for the Brood**: no Brood text, roster, stats or models change here. L1 (look) and L2 (lore) are proposals only, pending Mario's agreement; the existing Brood model entries are merely annotated with their catalogue IDs (B-xx, `retheme-pending`) so a later asset swap is a file swap. The L2 lore text will follow in a separate PR.

## Art pipeline pieces

- **Placeholders.** `assets-src/buildings/oathbound.ron` bakes `assets/buildings/oathbound.pak` from `repo:`
  sources only (Kenney Castle Kit, CDmir's Alien Bug, both CC0; docs/16). Any asset that is never modelled
  keeps its placeholder, or draws as before.
- **Catalogue IDs.** `assets-src/catalogue.csv` maps every catalogue ID (V-01..V-36, B-01..B-40) to the model
  entry it lands in and a status. `cargo run -p gc_artbake --release -- --catalogue assets-src/catalogue.csv`
  fails if a row names a model its pack lacks. Swapping an asset in is a file swap plus a `source` edit and a re-bake.
- **Shared rigs and variants.** Meshy auto-rig does bipeds only. `tools/blender_rig/rig_nonbiped.py` rigs a
  static mesh from a **template** (`quadruped`, `crawler`, `flyer`, `engine`): fixed joint names, parents and rest
  pose, so every unit of one template shares one skeleton and one clip set (Idle, Walk, Run, Attack, Attack2,
  Hit, Die, Cheer), and `--tint r,g,b` recolours one base mesh into a variant without a second rig.
  Test on placeholder meshes: `blender -b --python tools/blender_rig/rig_nonbiped.py -- --in
  assets-src/kenney/castle-kit/glb/siege-ram.glb --out ram.glb --template engine` (Blender 4.0+). The outputs in
  `assets-src/rig-test/` are baked in `oathbound.ron` (Heartwood Ram, Briar Hounds, Gale Sails, and a tinted
  Heartwood Ram Variant on the same rig).

## Follow-up

Meshy asset swaps still to come (budget: one Meshy Pro month, about 1,000 credits, covering the playable
core of both factions); the rest keep placeholders. Brood L3 roster. Economy B and the Oath power.
