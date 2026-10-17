# 04 · Economy and Resources

The economy should be *readable at a glance* and *interesting to attack*. Every faction's income has a visible weak point on the map: workers in transit, veins, triangles, caravans.

## The HUD shows four things

```
[ Grub 1,240 ]  [ Marrow 185 ]  [ Supply 64/90 ]  [ Honor ███████░░░ 70 ]
```

That is the entire economic HUD. Everything else is visible in the world (seep pips, glowing veins, chord triangles, caravan routes).

## Resources

### Grub (common)
- Comes from **seeps**. Every faction can harvest every seep.
- Pays for almost everything.
- A fresh seep holds **7,500 Grub**. When it runs dry it becomes **dregs** and yields 25% of its normal rate forever, so late games never fully starve and stalemates still have income.

### Marrow (rare)
- Comes from **Marrow veins**, usually 1 near each start position and several in exposed or creep-guarded spots.
- A vein holds **3,000 Marrow** and does not regenerate. It turns to dregs at 25% like seeps.
- Pays for tier upgrades, elite units, monsters, hero revival and most research. Marrow is what makes expansion and map control matter.

### Supply
- Cap of **150**, raised by housing (Farmstead +10, Pustule +8, Resonator +4, Tenement +10; each main building tier +10). The Clans' Outpost Hall adds +6, and a Choir Siphon +4 like the Resonator it is.
- A regiment takes its supply when it starts training, and counts toward the cap while it trains. A queue waits while there is no room under the cap.
- Supply costs are listed in [03 · Factions](03-factions.md#faction-design-rules). Each building's supply, with its cost, build time and hit points, is in [21 · Buildings](21-buildings.md).

### Faction meter
One per faction (Honor, Larvae, Resonance, Tithe). See each faction.

*Built (2026-10-17, faction depth stage 1): Brood larvae are simulated. Each Hive and Hatchery Pit holds up to 3 larvae and grows one every 12 s while it has room, at the match's tempo. The opening Hive starts full, and a Hive or Pit built from a foundation starts empty, while a tier upgrade keeps its larvae. Every Brood unit, workers included, takes a larva when it is queued, and training is refused while none is ready; calling it off gives the larva back, to whichever Hive or Pit with room holds the most. Brood regiments in one queue hatch together, in queue order: one waiting for supply holds back those behind it, and a Brood building is grown by a Gnawer that is used up at the foundation.*

## Harvest rates (first-pass targets)

All four economies are tuned so that a **fully developed seep produces about 200 Grub per minute** for every faction at Standard tempo. They differ in startup cost, risk and ceiling, not in the base number:

| Faction | To saturate one seep | Ceiling | Main risk |
|---|---|---|---|
| Ember Clans | 5 Hands (250 Grub) + optional Granary | 200/min | Hands killed in transit |
| Brood | 5 Gnawers (250 Grub, 5 larvae) + vein link | 200/min | Pustule killed, vein cut |
| Choir | 1 Resonator (150 Grub) | 200/min, up to 320/min in a large chord | One pylon breaks the chord |
| Concord | n/a (trade-based) | ~200/min per 3-caravan route of average length | Caravans raided |

A Marrow vein saturated by 3 workers (or one Siphon) yields about **50 Marrow per minute**.

*Built (2026-10-16, M3 step 2): workers are simulated. A worker regiment ordered to gather walks to a node, fills a load (10 Grub in 9 s, or 5 Marrow in 12 s), carries it to the nearest drop-off that takes that resource and banks it. A seep works five workers at once and a vein three; others wait beside it. A node that runs dry becomes dregs: it still gives whole loads, but each takes four times as long. In tests, five workers on a seep seven tiles from their hall bring in about 200 Grub a minute, and three on a vein about 50 Marrow a minute, so a drop-off near a seep pays. Nothing can be built on a seep or vein, and a worker that three times in a row gets no nearer to its node or drop-off gives up. A worker that routs goes back to work once it rallies. Every faction uses plain workers for now; Brood veins, Choir Resonators and Concord caravans come later (the Brood's larvae came first, see the faction meter).*

*Built (2026-10-16, M3 step 3): construction, training and tiers are simulated. A foundation stands at 10% of its hit points and rises as it is built, with up to four builders (the first at full speed, each further one at half). It gives no supply, tier or drop-off until finished. A building's queue holds five regiments, paid for when queued, and each starts once supply has room. Calling off a building refunds 75%. Tier upgrades take their time with no builders, and the building keeps working. Tempo scales all of it. The lobby and HUD come in step 4.*

*Built (2026-10-16, Conquest step 4a): until each faction's own economy arrives (Brood veins, Choir Resonators, Concord caravans), every faction's main building takes both Grub and Marrow from workers, so all four can gather in Conquest. The roster files say so for now (`data/rosters/*.ron`), each marked to come out when that faction's own economy is built.*

## Starting state (Conquest)

| | Clans | Brood | Choir | Concord |
|---|---|---|---|---|
| Main building | Clan Hall | Hive | Spire + 2 Resonators | Counting House |
| Workers | 5 Hands | 4 Gnawers + 3 larvae | 3 Tuners | 4 Clerks + 1 Caravan |
| Grub / Marrow | 300 / 0 | 300 / 0 | 300 / 0 | 400 / 0 |
| Supply cap | 12 | 12 | 12 | 12 |

Every start position has 2 Grub seeps and 1 Marrow vein within its natural base area, a "natural expansion" with 2 more seeps nearby, and a Grubfolk hamlet within reach.

## Upkeep (Warcraft III style)

Big armies tax your income, which keeps the economy relevant late and rewards fighting over turtling:

| Supply used | Grub income |
|---|---|
| 0 to 60 | 100% |
| 61 to 100 | 75% |
| 101 to 150 | 55% |

Upkeep is built in the simulation (2026-10-16, M3 step 2), and it never cuts Marrow. The current upkeep band is shown as a colour on the supply counter (white, amber, red), with a tooltip.

## Tech tiers

| Upgrade | Cost | Time | Unlocks |
|---|---|---|---|
| Tier 2 | 300 Grub, 100 Marrow | 60 s | Elite infantry, cavalry, air, artillery, second hero |
| Tier 3 | 600 Grub, 250 Marrow | 90 s | Monsters, top upgrades, third hero, tier-3 meter abilities |

Pacing targets at Standard tempo: first tier-2 around minute 5 to 6, first big army clash around minute 8 to 12, supply cap reached around minute 18 to 25.

## Reinforcement (Total War replenishment)

Regiments that lose models can **reinforce** instead of being replaced, which keeps veterans valuable:

- A damaged regiment standing near a reinforcement point regains 1 model every 3 s, paying the per-model share of its original cost.
- Reinforcement points: Clans near any Hall, Dojo or Stables; Brood anywhere on veins; Choir within a Resonator field; Concord living units near a Guildhall, undead only through Tithe.
- **Veterancy** (3 ranks, earned by kills) survives reinforcement: +10% damage and +10 morale per rank.

## Expansion

Each faction expands differently, consistent with its economy:

- **Clans:** build an **Outpost Hall** (cheaper main building without production) by the new seeps.
- **Brood:** chain Pustules out to the seeps, then optionally a Hive for larvae.
- **Choir:** just plant Resonators, and decide whether to stretch a chord over them.
- **Concord:** build a Waystation, which both extends trade routes and works as a drop-off for Marrow.

## Titan Events (the living world)

The Titan is still slightly alive. Events push players together and break stalemates. They are on by default and can be turned off in the lobby.

| Event | When | What happens |
|---|---|---|
| **Tremor** | Minute 12, 24, 36 | A **Bloodseep** (double rate, 5,000 Grub) erupts at a contested location. A beacon marks the spot 60 s before. |
| **Marrow Bloom** | Random, after minute 15 | A creep-guarded vein refills to full. |
| **Heartbeat** | Once per 10 minutes | The Titan's Heart (map centre) pulses; for 30 s units near it heal and gain morale. |
| **Gut Fog** | Map setting | Periodic fog banks that shrink vision ranges in lowlands. |

Events are deterministic (seeded by the match seed) so they work with lockstep networking and replays.

## Trading and raiding the Grubfolk

Neutral **Grubfolk hamlets** sit near seeps across the map. Any faction can interact with them, the Concord most of all. Full detail in [07 · World and Maps](07-world-and-maps.md#grubfolk-hamlets).

- **Market:** swap Grub for Marrow or back. Every trade moves the price by 5% against you; prices drift back over time. Buying a lot of Marrow fast is possible but expensive.
- **Mercenaries:** hire a Grubfolk militia regiment (or something stranger at bigger hamlets).
- **Rumours:** pay to reveal an area of the map for 30 s.
- **Raiding:** destroy the hamlet's palisade and you loot its store (roughly 400 Grub, 100 Marrow). The hamlet burns for 5 minutes, then rebuilds *hostile to you*. Every other hamlet becomes **Wary** of you (prices +25%, no mercenaries). Raiding is a strong tempo play with a long memory.

Reputation per hamlet per player: **Friendly → Neutral → Wary → Hostile**. Visible on the hamlet's banner. It is one use of the relations ledger shared by every party in the game ([18 · AI](18-ai.md#relations)).

## Creep camps

Warcraft-style neutral monster camps guard Marrow veins, treasure and Shrines. Killing them gives hero experience and loot (single-use or passive items, three inventory slots per hero). Camps do not respawn, so clearing them is a race.

## Tempo setting

A lobby option that scales the whole economy so the same map plays at different lengths:

| Tempo | Gather rate | Build and research time | Typical 1v1 length |
|---|---|---|---|
| Swift | ×1.5 | ×0.75 | 15 to 25 min |
| Standard | ×1.0 | ×1.0 | 25 to 40 min |
| Epic | ×0.8 | ×1.2 | 45 to 70 min |

## Field Battle points

Field Battle has no economy. Armies are bought with **points**, where a unit's point cost is `Grub + 2 × Marrow` from its Conquest cost (plus fixed prices for upgrades and hero levels). One price list for both modes means balance work carries over.
