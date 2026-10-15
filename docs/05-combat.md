# 05 · Combat and Units

Combat is where Grubcraft earns its "Total War" half. The goal: battles that look and feel like two armies colliding, where *position* matters more than clicking speed, but which a new player can read in seconds.

## The regiment

The basic unit of command is the **regiment**: a block of soldiers ("models") that moves, fights and breaks together.

| Regiment class | Typical models | Notes |
|---|---|---|
| Light infantry / swarm | 20 to 24 | Fast, fragile, numbers |
| Line infantry | 12 to 24 | The battle line |
| Ranged | 10 to 16 | Archers, beams, acid |
| Cavalry | 6 to 8 | Charges |
| Artillery / siege | 1 to 4 | Long range, slow |
| Air flight | 4 to 6 | Flies over terrain |
| Monster | 1 | Huge, terror |
| Hero | 1 | Levels, abilities, items |
| Worker | 1 | Not a regiment, but uses the same systems |

Every model has its own hit points. Damage removes models one at a time, from the back ranks forward, so a regiment visibly thins out and **hits less hard as it shrinks**. That is the "health bar" you read on the battlefield, and it is also shown precisely on the regiment card.

## Orders

Kept deliberately small. Every order is on the same hotkey for every regiment.

| Order | Key | What it does |
|---|---|---|
| Move | right-click / `Q` | Move in current formation, keep facing toward travel |
| Move and face | right-drag | Drag a line: sets position, frontage width and facing (Total War's signature control) |
| Attack | right-click enemy / `W` | Engage target, or attack-move to a point |
| Hold | `E` | Hold position and facing, only fight what reaches you |
| Charge | `X` | Cavalry and shock units charge for bonus damage and morale shock |
| Skirmish / Brace | `Z` (toggle) | Ranged units keep distance from approaching melee; spears brace against charges |
| Formation | `R` (cycle) | Line / Tight / Loose / Shield wall (shields only); Column and Wedge (cavalry) later. See [Weapons and formations](#weapons-and-formations) |
| Run | double right-click / `C` | Faster move, then **Winded** for 10 s |
| Retreat | `V` | Disengage toward the nearest friendly structure, ignoring attacks |

Keys follow the fixed 4 × 3 command grid; abilities take the middle row (see [08 · UI and UX](08-ui-ux.md#screen-layout-1080p-reference)).

**Smart defaults so you don't have to micro:** regiments in a group move together at the speed of the slowest and arrive in formation; ranged regiments fire at will and Skirmish by default; units auto-engage enemies that attack them; a regiment ordered to attack a target that is out of reach paths around obstacles as a block.

## Facing, flanks and charges

| Hit from | Damage multiplier | Morale shock |
|---|---|---|
| Front | ×1.0 | none |
| Flank | ×1.4 | moderate |
| Rear | ×1.8 | heavy |

- Regiments take time to turn. Big blocks turn slowly; cavalry and swarms turn fast.
- **Charges** add a bonus proportional to speed at impact, then fade over a few seconds. Charging braced spears into the front is suicide; charging archers in the rear wins battles.
- A regiment engaged in melee is **locked**. It can only disengage with Retreat (taking free hits) or by routing.

## Weapons and formations

A regiment's weapons decide which formation suits it, so the Formation order is a real choice per unit type (adapted from Mount & Blade, [research note MB-3](14-decisions-needed.md#battle-feel-from-mount--blade-decided-2026-10-15)). Three flags on each unit carry it: **reach** (spears and pikes), **swing room** (big swinging blades: two-handed swords, axes, mandibles) and **shield**.

| Formation | Spacing | Melee defence | Weapons | Arrows |
|---|---|---|---|---|
| **Line** (default) | as the unit's stats | +0 | full | as usual |
| **Tight** | 80% | +10 | swinging blades deal 80% | denser block, takes more |
| **Loose** | 150% | −10 (swinging blades +0) | spears and shields deal 85% | sparser block, takes fewer |
| **Shield wall** (shields only) | 70% | +10 from the front only | swinging blades deal 80% | 30% from the front |

- **Reach:** spears and pikes fight from their first two ranks when the foe is in front (the second rank at half strength), and when a foe comes at their points they strike the moment lines meet instead of waiting for their next blow (then skip that blow, so they strike sooner, not more often). Their support needs a close line, so Loose order costs them.
- **Swing room:** big swings need space. Cramped in Tight order or a shield wall they lose a fifth of their damage; in Loose order they keep their defence, because the gaps are what they fight in.
- **Shield wall:** the strongest answer to archers (arrows from the front do 30% damage, and the front holds better in melee), but the regiment marches at half pace, can't charge, and is no better than a Line when hit in the flank or rear. Only regiments with shields can form one.
- Ranks re-form only out of the fight: a regiment locked in melee keeps its formation, and a routing one drops back to Line and takes no formation orders until it rallies. A group order with a shield wall in it marches at the wall's pace.
- Each unit also has a **mass** (light, medium or heavy: swarms and flyers light, cavalry and monsters heavy), which decides how hard it shoves in a locked melee once lines that push arrive.

## Morale

The core of Shogun-style battles: armies lose because they *break*, not because every soldier dies.

Each regiment has a **morale** value. It drops from:
- Casualties (especially fast casualties)
- Being hit in the flank or rear
- Nearby friendly regiments routing
- Terror units (monsters, some abilities)
- Being outnumbered locally
- Their general (any hero) dying nearby

It rises from:
- Nearby heroes (leadership aura)
- Winning the local fight, routing enemies
- Veterancy
- Holding a Shrine, faction effects (Honor Edicts, Heartbeat)

### Morale states
| State | Effect |
|---|---|
| **Steady** | Normal |
| **Wavering** | −15% damage; banner flickers |
| **Routing** | Flees toward its home, ignores orders. Can **rally** after 10 s if no enemy is nearby |
| **Shattered** | Routed twice in a short time; flees to the nearest structure and only recovers there |

Routing regiments are slower than their pursuers, so a rout is a real loss, but they are not deleted: a commander who saves a broken regiment can reinforce it.

**Conquest is gentler than Field Battle.** In Conquest, rallying is faster and morale loss from casualties is lower, because losing a regiment to a rout costs economy. In Field Battle the full Shogun bite applies.

**Exceptions** that make each faction feel different: Concord undead never rout (they crumble instead); Brood regiments lose less morale from casualties but more from losing a nearby hero; Clans regiments in tight formation resist flank shock.

## Terrain

| Terrain | Effect |
|---|---|
| High ground | +15% ranged range and +10% melee damage when fighting downhill |
| Forest | Hides regiments until an enemy comes within 4 tiles (10 while they march); cavalry slowed 30%; ranged accuracy reduced |
| Tall grass | Hides regiments standing still until an enemy comes within 3 tiles |
| River ford / shallows | Slow (−40%); units in water take +20% melee damage |
| Road | +15% speed |
| Cliff / rib-ridge | Impassable except to air and Burrowers |
| Bog | Slow, no charges |

**Built so far (terrain rules):** high ground and fords work as in the table. "Higher" means the regiment's tile stands at least half a tile above its foe's; the ranged bonus counts only against targets that far below the archers. A regiment whose centre is in water wades at 60% pace, takes 20% more melee damage, and can't charge (it reaches the bank with no charge bonus). Steep slopes still cost routes but don't slow the march. Woods and tall grass hide regiments, as the table says, with the full rules for spotting and springing ambushes in [20 · Ambushes](20-ambushes.md#the-battle-rules); the forest's slow-down and arrow cover come later, and roads and bogs come with the map tile types.

## Ranged combat

- Ranged regiments fire **volleys**. Each volley rolls hits against the target area, so firing into a tight block hits more than firing at skirmishers in loose formation. A shield wall takes only 30% from volleys that land on its front.
- Arcing fire (bows, acid, rockets) can shoot over friendly lines but is less accurate at long range. Direct fire (crossbows, beams) needs line of sight.
- **No ammunition counter.** Ranged units have unlimited ammo; instead they are slow to reposition and terrible in melee.
- Friendly fire is on for artillery only, so placing rocket carts and Broodmothers is a skill.

## Air

- Air units fly over all terrain and ignore pathing and blocking.
- They can only be hit by other air units, structures with anti-air, and ranged units flagged as anti-air (archers and beams can; artillery cannot).
- Air units are deliberately small (4 to 6 models) and fragile; they scout, harass and pick targets. No faction gets an air army that wins alone.

## Heroes

Warcraft III's best idea, kept:

- Level 1 to 10 from combat and creep experience. Three basic abilities (up to 3 ranks each) and an ultimate at level 6.
- Three item slots, filled from creep loot and hamlet merchants.
- Leadership aura: friendly regiments nearby have higher morale. A dead hero shakes nearby morale.
- Dead heroes can be revived at the hero altar for Grub, Marrow and time.
- One hero available at tier 1, two at tier 2, three at tier 3.

## Counters (soft rock-paper-scissors)

| Unit type | Strong against | Weak against |
|---|---|---|
| Spear / pike | Cavalry | Ranged, heavy infantry |
| Cavalry | Ranged, artillery, routing units | Spears, braced lines |
| Ranged | Infantry in the open, monsters | Cavalry, air |
| Heavy infantry | Light infantry, swarms | Ranged, monsters |
| Air | Artillery, workers, isolated ranged | Anti-air ranged, air |
| Monster | Infantry, morale | Focused ranged, heroes |
| Artillery | Blocks of infantry, structures | Everything that reaches it |

Counters are multipliers between 0.7× and 1.5×, never hard immunities, so a well-positioned "wrong" army can still win. The proposed class-by-class table is in [17 · Unit Rosters](17-unit-rosters.md#classes-and-counters).

## Structures in combat

- Structures have armour types: **Fortified** (main buildings, towers, walls), **Structure** (production) and **Living** (Brood). Siege units deal bonus damage to Fortified.
- Infantry can damage structures, slowly. Clan archers with Fire Arrows deal burning damage; Concord Debt Collectors steal Grub on hit.
- Walls and gates (Clans and Concord) create chokepoints; the Brood spreads veins instead; the Choir relies on teleporting armies to defend.

## Veterancy

Regiments gain experience from kills. Three ranks, shown as chevrons on the regiment banner and card: +10% damage and +10 morale per rank. Veterancy survives reinforcement and Brood metamorphosis.

## Combat simulation in one paragraph

The simulation models each regiment as a formation (position, facing, width, depth) with per-model hit points and a mask of which slots are alive. Melee is resolved along the contact frontage, file by file; ranged fire is resolved per volley against the target's formation footprint. Individual soldiers on screen are a **presentation layer** that animates those results: they step into gaps, swing, fall, and flee. Soldiers are planned to fight in visible one-on-one pairs, with matched kills for heroes and charges later ([09 · Art and Audio](09-art-and-audio.md#animation)); this is presentation only, so it never changes who wins. This keeps the deterministic simulation tiny (hundreds of regiments, not thousands of soldiers) while the screen shows thousands of soldiers. See [10 · Architecture](10-architecture.md#the-two-tier-simulation).

**Built so far (M1 step 5):**
- **Fighting:** melee with flank, rear and charge multipliers, locked melee, casualties from the back, and arrow volleys that hit dense blocks harder. The orders are move, move-and-face (right-drag), attack and formation.
- **Weapons and formations (2026-10-15):** reach, swing room, shields and mass work as in [Weapons and formations](#weapons-and-formations). The formation key is `F` for now, because `R` reloads the unit data in the sandbox. Of today's five Field Battle regiments the Yari Ashigaru and Bone Pikes have reach, the Shardlings have swing room and the Skitterlings are light; none carries shields yet, so the Shield wall needs a roster unit with shields (Chitin Guard, Shellbacks, Gravewardens) or the sandbox's stats panel. Computer players don't change formation yet.
- **Morale:** Steady, Wavering, Routing and Shattered work as described above, driven by casualties, flank and rear hits, being outnumbered nearby, and nearby routs. Winning a fight lifts morale. Routers are slower than their pursuers and rally after 10 s with no enemy within 20 tiles. A second rout within 2 minutes shatters a regiment; with no structures yet, a shattered regiment runs, then stands for good. Concord undead (the Bone Pikes) never rout. A battered regiment never recovers its full morale.
- **Not yet:** heroes, terror and veterancy as morale sources, the other faction exceptions, and Conquest's gentler rules. The Hold, Charge, Skirmish/Brace, Run and Retreat orders, the Column and Wedge formations, terrain beyond high ground and fords (see [Terrain](#terrain)), air, heroes, counters and veterancy also come later.

The exact formulas are in [10 · Architecture](10-architecture.md#formations-and-combat).
