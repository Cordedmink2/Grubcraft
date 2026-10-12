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
| Formation | `R` (cycle) | Line / Column / Loose (vs artillery) / Wedge (cavalry) |
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
| Forest | Conceals non-moving units from beyond 4 tiles; cavalry slowed 30%; ranged accuracy reduced |
| Tall grass | Conceals infantry that are not attacking |
| River ford / shallows | Slow (−40%); units in water take +20% melee damage |
| Road | +15% speed |
| Cliff / rib-ridge | Impassable except to air and Burrowers |
| Bog | Slow, no charges |

## Ranged combat

- Ranged regiments fire **volleys**. Each volley rolls hits against the target area, so firing into a tight block hits more than firing at skirmishers in loose formation.
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

Counters are multipliers between 0.7× and 1.5×, never hard immunities, so a well-positioned "wrong" army can still win.

## Structures in combat

- Structures have armour types: **Fortified** (main buildings, towers, walls), **Structure** (production) and **Living** (Brood). Siege units deal bonus damage to Fortified.
- Infantry can damage structures, slowly. Clan archers with Fire Arrows deal burning damage; Concord Debt Collectors steal Grub on hit.
- Walls and gates (Clans and Concord) create chokepoints; the Brood spreads veins instead; the Choir relies on teleporting armies to defend.

## Veterancy

Regiments gain experience from kills. Three ranks, shown as chevrons on the regiment banner and card: +10% damage and +10 morale per rank. Veterancy survives reinforcement and Brood metamorphosis.

## Combat simulation in one paragraph

The simulation models each regiment as a formation (position, facing, width, depth) with per-model hit points and a mask of which slots are alive. Melee is resolved along the contact frontage, file by file; ranged fire is resolved per volley against the target's formation footprint. Individual soldiers on screen are a **presentation layer** that animates those results: they step into gaps, swing, fall, and flee. This keeps the deterministic simulation tiny (hundreds of regiments, not thousands of soldiers) while the screen shows thousands of soldiers. See [10 · Architecture](10-architecture.md#the-two-tier-simulation).
