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

Every model has its own hit points. In melee every blow is one soldier's, struck at the enemy soldier across from him: it lands or is parried, and one that lands wounds that man or kills him when his own hit points run out. The man from the back steps up into his place, so a regiment visibly thins out from the back and **hits less hard as it shrinks**, while wounded men fight on in the front. Where lines overlap, two men strike one. A regiment loses heart from the wounds it takes as well as from the men it loses. That is the "health bar" you read on the battlefield, and it is also shown precisely on the regiment card.

## Orders

Kept deliberately small. Every order is on the same hotkey for every regiment.

| Order | Key | What it does |
|---|---|---|
| Move | right-click / `Q` | Move in current formation, keep facing toward travel |
| Move and face | right-drag | Drag a line: sets position, frontage width and facing (Total War's signature control) |
| Attack | right-click enemy / `W` | Engage target, or attack-move to a point |
| Hold | `E` | Hold position and facing, only fight what reaches you |
| Charge | `X` | Cavalry and shock units charge for bonus damage and morale shock |
| Skirmish / Brace | `Z` (toggle) | **Brace is built:** spears and pikes set against a charge ([below](#brace-and-fall-back)). Skirmish, where ranged units keep distance from approaching melee, comes later |
| Formation | `R` (cycle) | Line / Tight / Loose / Shield wall (shields only); Column and Wedge (cavalry) later. See [Weapons and formations](#weapons-and-formations) |
| Run | double right-click / `C` | Faster move, then **Winded** for 10 s |
| Fall back | `V` | Pull out of the fight in good order: back 8 tiles at half pace, still facing the foe ([below](#brace-and-fall-back)). Retreat toward the nearest friendly structure comes with Conquest |

Keys follow the fixed 4 × 3 command grid; abilities take the middle row (see [08 · UI and UX](08-ui-ux.md#screen-layout-1080p-reference)).

**Smart defaults so you don't have to micro:** regiments in a group move together at the speed of the slowest and arrive in formation; ranged regiments fire at will and Skirmish by default; units auto-engage enemies that attack them; a regiment ordered to attack a target that is out of reach paths around obstacles as a block.

## Facing, flanks and charges

| Hit from | Damage multiplier | Morale shock |
|---|---|---|
| Front | ×1.0 | none |
| Flank | ×1.4 | moderate |
| Rear | ×1.8 | heavy |

- Regiments take time to turn. Big blocks turn slowly; cavalry and swarms turn fast.
- **Charges** add a bonus proportional to speed at impact, then fade over a few seconds. A charge into the front of braced spears doesn't count as a charge, and the spears turn it back on the charger ([Brace and Fall back](#brace-and-fall-back)); charging archers in the rear wins battles. A charge needs a run-up: a regiment only charges after a second of marching without stopping or fighting (about 3 tiles for infantry), so blocks that brush apart and close again, a regiment turned or pushed out of reach that presses straight back in, or one that starts next to its foe, don't win a charge.
- A regiment engaged in melee is **locked**. It can disengage by Fall back (each foe locked with it gets one parting blow, see [Brace and Fall back](#brace-and-fall-back)) or by routing, though a stronger foe can drive it back ([Lines that push](#lines-that-push)). Contact is sticky: two blocks meet when their edges come within half a tile, and a regiment keeps fighting the foe it is touching even when another enemy comes up nearer, so who is winning the fight isn't reset. Lines that drift apart (blocks shrinking as soldiers fall, or turning to face their foe) stay locked, with no other enemy in reach, until their edges are two tiles apart, and close up again at a tile a second; blows land only within the half tile. A foe that routs no longer holds a regiment. (Before this, nine in ten contacts brushed apart and re-formed within two seconds.)

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
- **Formation discipline** (the Ember Clans' signature, a unit trait like the three flags above): a regiment with it gets 15% of its own melee defence more (rounded down, at least 1) while it stands still in Tight order or a shield wall, on top of the +10 (and on a shield wall's front only). "Still" means it has no march to carry out (turning on the spot, or standing in a fight, is still; the screen's and the computer players' readouts use the same rule, `RegimentView::still`); it is lost when the regiment is ordered to move, even locked in melee. It changes nothing in Line or Loose order. The Clans' Yari Ashigaru and Yumi Ashigaru have it in Field Battles.
- Ranks re-form only out of the fight: a regiment locked in melee keeps its formation, and a routing one drops back to Line and takes no formation orders until it rallies. A group order with a shield wall in it marches at the wall's pace.
- Each unit also has a **mass** (light, medium or heavy: swarms and flyers light, cavalry and monsters heavy), which decides how hard it shoves in a locked melee ([Lines that push](#lines-that-push)).

## Pushing through friends

Friendly regiments can march into and through each other, but it costs them. A block pushing more than a quarter tile into a friend slows, down to 40% of its pace once it is a tile and a quarter deep. Untangling is as slow as pushing in, and nobody charges while pushing through friends. While two friendly blocks overlap by more than a tile, both fall into **disorder** (even locked in the same fight: stacking two blocks on one front costs them both): their ranks break, they deal 20% less melee damage and take 20% more, and once clear they take 3 seconds to re-form (the penalty fades as they do). Blocks in a line that only brush or wheel into each other stay in order. On screen a disordered block's soldiers break ranks, mill about and look around, and a ragged amber outline marks it. Routers neither slow nor fall into disorder (they are past caring). In fords and narrow passes regiments queue instead, behind a friend holding its ground or marching on ahead the same way, and a block too wide for the gap narrows into a deeper column while it passes and widens again after (see [10 · Architecture](10-architecture.md#pathfinding)).

**Built so far (2026-10-15):** as described. Other rules can disorder a regiment too (the sim's `Regiments::disorder`), which the Mount & Blade work uses.

## Lines that push

A locked melee is not a standing trade of blows: the side winning it drives the other back, so a battle line visibly bends where it is losing, and a commander can see where to send reserves without reading a number (adapted from Mount & Blade, [decision 24](14-decisions-needed.md#battle-feel-from-mount--blade-decided-2026-10-15)).

- **Who is winning:** each regiment keeps a running tally of the melee damage it has dealt and taken over the last few seconds, each blow weighted by the striker's **mass**: a heavy blow (cavalry, monsters) counts four times a light one (swarms), a medium one twice. The tally is read once a second, after the regiment's own blow, so both sides have always struck the same number of times, and it starts afresh whenever the regiment's foe changes.
- **Giving ground:** a regiment taking up to half as much again as it deals holds on; past that it steps back, faster the worse it is losing, up to 0.4 tiles a second once it takes four times what it deals. It backs straight away from the side its foe is on (from a frontal fight, straight back; from a flank attack, sideways), wading slower in water. The enemies fighting it follow step for step, and so does anyone fighting *them*, so the fight moves as one and nobody breaks contact and charges back in. Only a foe that is fighting it back, winning its own fight and free to step forward drives a regiment back; in a scrum of three or more where the foe in front is itself losing (say, to a regiment in its rear) nobody is pushed.
- **Evenly matched lines hold:** two equal regiments trading blows stay where they are. At equal blows a heavy regiment drives a light one back at full pace and a medium one at about a third of it.
- **Pinned:** a regiment driven back onto something it can't back into is crushed against it instead: a friendly block it would push more than a tile into, an enemy block behind it, a cliff or other blocked ground, the water behind a river bank it stands on, or the map's edge (anywhere along its rear edge, corners included). It stops, falls into **disorder** (as [Pushing through friends](#pushing-through-friends): 20% less damage dealt, 20% more taken) and loses 3 morale for every second in which it is pinned (2.1 at the current [battle pace](#battle-pace)). Fighting with a river or your own reserves at your back is a real risk now.
- Routers, and regiments not in melee, are never pushed.

**Built so far (2026-10-15):** as described. Soldiers on screen keep facing their man as their block gives ground, so a pushed line steps backwards; a pinned one breaks ranks under the amber disorder outline. Computer players don't yet read a bending line to send reserves.

## Charges that plough in

A regiment is a mass of bodies, not a wall (idea 11, physical formations, in the [restart plan](29-restart-plan.md)). A charge that is heavy enough for the ranks it hits ploughs into them and drives them back; one that isn't stops on the front rank.

- **Weight of the charge:** the charger's heft (light 1, medium 2, heavy 4, as in [Lines that push](#lines-that-push)) times 2 for the run.
- **Weight that holds it:** the heft of the regiment it hits, times how many ranks stand behind the point of impact: up to 4 on its front, but only 1 on its flank or rear. On its front, Tight ranks and a shield wall hold half again as well and Loose ranks a third less (67%); nobody braces a side, so any order holds a flank or rear alike.
- **Ploughing in:** when the charge outweighs the ranks, the regiment hit is driven straight away from the charger at full pace (0.4 tiles a second) for a quarter of a second per point of weight the charge has over it (at most 1.75 s, heavy horse into one loose rank of light foot), whatever the exchange of blows says, as long as the charger keeps fighting it. That holds even when it is already locked with someone else: a flank charge drives a fighting line sideways off its foe, who doesn't follow. The chargers follow it in and strike the men they knocked aside at once. Hit on its front, it also falls into disorder (20% less damage dealt, 20% more taken, fading over 3 s); hit on a flank or rear it doesn't, since the blows there already land 1.4 or 1.8 times as hard. Driven back onto something it can't back into, it is [pinned](#lines-that-push) as well. A regiment that breaks, or is no longer in a fight, stops being driven; routers are only run down, never ploughed.
- **Examples:** heavy horse ploughs through three ranks of medium foot or four of light foot, but stalls on four ranks of medium foot or two of heavy foot. Medium foot ploughs a front of its own weight only when it stands one rank deep (two if Loose), but bowls over the flank of anything medium or light.
- **On screen:** the line dents in where the charge went in, up to one and a half ranks deep nearest the charger, and springs back over the last half second as the regiment holds again. Each man the chargers hit goes down whatever his weight, and the men either side of him stagger, or fall three times in ten.

**Built (2026-10-24):** as described. `SIM_VERSION` 27; tests in `crates/gc_sim/tests/plough.rs`. It moved the faction table, so Skitterlings defence and Shardlings morale were retuned ([17](17-unit-rosters.md#plough-balance)).

## Brace and Fall back

Two orders for the fight itself (adapted from Mount & Blade, [decision 28](14-decisions-needed.md#battle-feel-from-mount--blade-decided-2026-10-15)).

**Brace** (`Z`, the grid slot labelled "Brace"; Skirmish shares that slot when it arrives):

- Only regiments with long weapons, the `reach` flag (spears and pikes, such as Yari Ashigaru and Bone Pikes), can brace. A regiment must not be locked in melee, falling back or routing to brace.
- Bracing halts the regiment where it stands. It stays braced until it gets a move or attack order, falls back or routs. `Z` again lifts the spears. It can be set during deployment too.
- A braced regiment doesn't press back in after a foe that is pushed out of reach ([Lines that push](#lines-that-push)).
- A charge that hits a braced regiment's **front** is not a charge. The charger gets no charge bonus, loses its next blow (stopped dead on the points) and falls into disorder, as in [Pushing through friends](#pushing-through-friends): 20% less melee damage dealt and 20% more taken, fading over 3 s. The braced spears strike at once, and for the next 3 seconds their blows carry a charge bonus equal to the bigger of their own and the charger's: the charge's weight turned back on the charger.
- A charge into a braced regiment's **flank or rear** is a normal charge.
- In the drill test, chargers running into braced spears lost about 1.3× as many hit points in the first 3 seconds as against unbraced spears, and the spears lost under half as much.

**Fall back** (`V`, the grid slot labelled "Back"; a regiment pulls out of its fight in good order):

- It backs 8 tiles straight back from the foe it is fighting (or straight behind it, if it isn't fighting), kept on the map, at half pace and still facing the enemy. It stops after 8 seconds, wherever it got to. A second Fall back while it is still falling back does nothing.
- On the tick it is ordered, every enemy locked with it gets one parting blow, which strikes at once. It doesn't strike back while it falls back.
- Its foes let it go: they don't follow it or press back in.
- While it falls back, no enemy in front of it can lock it in melee. An enemy ordered to attack it waits at its front until it stops. An enemy that reaches its flank or rear catches it, and it stops falling back.
- Routing regiments can't fall back. Falling back lifts a brace.

**Built so far (2026-10-16):** Brace and Fall back work as described. Computer players don't use either order yet. The sandbox's `Shift`+`V` now reveals every hidden regiment (it was `V`). Retreat toward the nearest friendly structure still comes with Conquest. The golden replay `replays/field-brace-1v1.gcr` plays a braced Ember Clans line against a Brood army that only charges, with two of side 0's regiments falling back out of the fight once the lines meet.

## Morale

The core of Shogun-style battles: armies lose because they *break*, not because every soldier dies.

Each regiment has a **morale** value. It drops from:
- Casualties (especially fast casualties)
- Being hit in the flank or rear
- Nearby friendly regiments routing
- Terror units (monsters, some abilities)
- Being outnumbered locally
- Being pinned in melee with nowhere to give ground ([Lines that push](#lines-that-push))
- Their general (any hero) dying nearby
- Their side's champion falling in a duel before battle ([below](#champions-before-battle))

It rises from:
- Nearby heroes (leadership aura)
- Winning the local fight, routing enemies
- Veterancy
- Holding a Shrine, faction effects (Honor Edicts, Heartbeat)
- Their side's champion winning a duel before battle

### Morale states
| State | Effect |
|---|---|
| **Steady** | Normal |
| **Wavering** | −15% damage; banner flickers (not over Conquest workers, whose nerve is low by design) |
| **Routing** | Flees toward its home, ignores orders. Can **rally** after 10 s if no enemy is nearby |
| **Shattered** | Routed twice in a short time; flees to the nearest structure and only recovers there |

Routing regiments are slower than their pursuers, so a rout is a real loss, but they are not deleted: a commander who saves a broken regiment can reinforce it.

**Conquest is gentler than Field Battle.** In Conquest, rallying is faster and morale loss from casualties is lower, because losing a regiment to a rout costs economy. In Field Battle the full Shogun bite applies.

**Exceptions** that make each faction feel different: Concord undead never rout (they crumble instead); Brood regiments lose less morale from casualties but more from losing a nearby hero; Clans regiments in tight formation resist flank shock.

## Terrain

| Terrain | Effect |
|---|---|
| High ground | +15% ranged range and +10% melee damage when fighting downhill |
| Forest | Hides regiments until an enemy comes within 4 tiles (10 while they march); slows the march 15% (cavalry and monsters 30%); trees catch 30% of the arrows shot into a wood, and 30% of those shot out of one |
| Tall grass | Hides regiments standing still until an enemy comes within 3 tiles |
| River ford / shallows | Slow (−40%); units in water take +20% melee damage |
| Road | +15% speed |
| Cliff / rib-ridge | Impassable except to air and Burrowers |
| Bog | Slow, no charges |

**Built so far (terrain rules):** high ground and fords work as in the table. "Higher" means the regiment's tile stands at least half a tile above its foe's; the ranged bonus counts only against targets that far below the archers. A regiment whose centre is in water wades at 60% pace, takes 20% more melee damage, and can't charge (it reaches the bank with no charge bonus). Steep slopes still cost routes but don't slow the march. Woods and tall grass hide regiments, as the table says, with the full rules for spotting and springing ambushes in [20 · Ambushes](20-ambushes.md#the-battle-rules). A regiment whose centre is in a wood marches at 85% pace, or 70% for heavy troops (cavalry and monsters), and gives ground as slowly. Trees catch 30% of a volley loosed from a wood and 30% of one landing on a regiment in a wood, so archers shooting from one wood into another land about half their arrows; on screen, the caught arrows stick in the branches over the archers or over their target. Tall grass neither slows nor shields. Routes don't yet steer around woods to save time, and roads and bogs come with the map tile types.

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
- **Hero panel (Conquest UI piece 4):** built as a display, with no hero data yet; see [08 · UI](08-ui-ux.md#screen-layout-1080p-reference).

## Champions before battle

Before the lines meet, each side may send out its best fighter to duel the other's, as in Mount & Blade ([decision 27](14-decisions-needed.md#battle-feel-from-mount--blade-decided-2026-10-15), research note MB-4). It is a chance event in battles with at least one computer player, allowed by the lobby option **Champions before AI battles** (on by default).

- **When:** the first time two enemy regiments that can see each other come within 60 tiles, with both sides able to send a champion, there is an even chance that champions come out. It happens at most once a battle, and never once the battle has begun (a regiment of either side fighting in melee or losing soldiers, to arrows too).
- **Who:** until heroes reach the simulation (M3), a side's champion is a soldier of its best fighting regiment near the front: the highest melee attack plus defence among its regiments within 25 tiles of the one nearest the enemy that are not fighting, broken or hiding in cover ([20 · Ambushes](20-ambushes.md); a champion would give them away) and have at least two soldiers. Then a hero will be the champion.
- **The duel:** the two run to the point halfway between their regiments and trade blows, one swing each in turn. A blow lands with the battle's usual chance to hit (50 + attack − defence, kept between 10% and 90%); a man it lands on reels and loses his next swing, and the third blow to land kills. The whole duel is decided by the match's dice when the challenge is called, so every machine and every replay sees the same fight.
- **The stakes:** when the last blow lands, every regiment on the winner's side gains 6 morale, and the side keeps them as a higher ceiling for the battle (regiments it brings on later too); every regiment on the loser's side loses 6 from its morale and the side from its ceiling (never routing from it alone). The loser's regiment loses him.
- **No result:** if neither falls within 30 swings they part. If the battle starts around them first (a regiment of either side fights in melee or loses soldiers), or a champion's regiment breaks or is destroyed, the duel is lost in the battle. Neither changes anything.
- **The armies don't wait:** a challenge is a moment in the battle, not a pause. Computer players whose side has a champion out halt their advance and watch while he fights, without using up their patience; a human can attack through it, with arrows or in melee, at the cost of whatever the duel would have given.
- **On screen:** the battle panel announces the challenge and its result for eight seconds. The two champions run out of their ranks, fight blow by blow with the duel moves (swing, parry or block, stagger), and the winner cheers over the fallen man before both rejoin the battle.

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

- **Towers shoot** (built 2026-10-24): a finished tower looses its shots each reload at an enemy in range, focus target first, then siege engines, then regiments attacking its side's buildings, then the nearest; every shot lands. See [26 · Fortifications](26-fortifications.md#build-plan).
- Structures have armour types: **Fortified** (main buildings, towers, walls), **Structure** (production) and **Living** (Brood). Blows against structures always land. Most regiments deal half damage to Fortified, artillery and heroes deal full damage, and siege units deal ×3.0 (×2.0 to the other two). The full table and every building's hit points are in [21 · Buildings](21-buildings.md#damage-against-buildings).
- Infantry can damage structures, slowly. Clan archers with Fire Arrows deal burning damage; Concord Debt Collectors steal Grub on hit.
- **Built (2026-10-16):** melee regiments strike a building once a second with their front rank (spears also their second rank, at half). Swords cramped in close order hit buildings as weakly as they hit regiments. Shooters loose a volley each reload, and every arrow in it hits. Blows always land. A regiment that can't find a way to its building gives up after three tries that bring it no nearer.
- Walls and gates (Clans) create chokepoints; the Concord holds roads with Tollgates; the Brood spreads veins instead; the Choir relies on teleporting armies to defend.

## Veterancy

Regiments gain experience from kills. Three ranks, shown as chevrons on the regiment banner and card: +10% damage and +10 morale per rank. Veterancy survives reinforcement and Brood metamorphosis.

## Combat simulation in one paragraph

The simulation models each regiment as a formation (position, facing, width, depth) with per-model hit points and a mask of which slots are alive. Melee is resolved along the contact frontage, file by file; ranged fire is resolved per volley against the target's formation footprint. Individual soldiers on screen are a **presentation layer** that animates those results: they step into gaps, swing, fall, and flee. Soldiers fight in visible one-on-one pairs on screen ([09 · Art and Audio](09-art-and-audio.md#animation)). **Built (2026-10-24):** the duels are in the simulation (option B of [28 · Every Soldier Fights](28-soldier-combat.md)): inside a locked melee each soldier strikes the man across from him, every blow is rolled, a soldier dies when his own hit points run out, and the man who falls on screen is the man the simulation killed; blocks still move as blocks. Every soldier simulated as a body (option C) is a later stage. This keeps the deterministic simulation tiny (hundreds of regiments, not thousands of soldiers) while the screen shows thousands of soldiers. See [10 · Architecture](10-architecture.md#the-two-tier-simulation).

**Built so far (M1 step 5):**
- **Fighting:** melee with flank, rear and charge multipliers, locked melee, casualties from the back, and arrow volleys that hit dense blocks harder. The orders are move, move-and-face (right-drag), attack and formation.
- **Weapons and formations (2026-10-15):** reach, swing room, shields and mass work as in [Weapons and formations](#weapons-and-formations). The formation key is `R`, as in the command grid (the sandbox's unit-data reload moved to `Shift`+`R`; `F` still cycles formation too until the Ultimate slot has an ability). Of today's five Field Battle regiments the Yari Ashigaru and Bone Pikes have reach, the Shardlings have swing room and the Skitterlings are light; none carries shields yet, so the Shield wall needs a roster unit with shields (Chitin Guard, Shellbacks, Gravewardens) or the sandbox's stats panel. Computer players don't change formation yet.
- **Lines that push (2026-10-15):** as in [Lines that push](#lines-that-push).
- **Charges that plough in (2026-10-24):** as in [Charges that plough in](#charges-that-plough-in).
- **Morale:** Steady, Wavering, Routing and Shattered work as described above, driven by casualties, flank and rear hits, being outnumbered nearby, being pinned, and nearby routs. Winning a fight lifts morale. Routers are slower than their pursuers and rally after 10 s with no enemy within 20 tiles. A second rout within 2 minutes shatters a regiment; with no structures yet, a shattered regiment runs, then stands for good. Concord undead (the Bone Pikes) never rout. A battered regiment never recovers its full morale.
- **Not yet:** heroes, terror and veterancy as morale sources, the other faction exceptions, and Conquest's gentler rules. The Hold, Charge, Skirmish and Run orders, the full Retreat toward a structure (with Conquest), the Column and Wedge formations, terrain beyond high ground and fords (see [Terrain](#terrain)), air, heroes, counters and veterancy also come later.

<a id="battle-pace"></a>**Built so far (battle pace, 2026-10-18):** battles were over too fast, so two constants in `gc_sim::combat` slow the killing. `BATTLE_PACE_PERCENT = 70` scales melee blows to 70% of their damage; `VOLLEY_PACE_PERCENT = 85` (the average of 70 and 100) scales volleys to 85%. Volleys are slowed half as much because the approach, where most arrows fly, does not get longer: slowing them as much as melee dropped the Clans to about 28% wins, and not slowing them pushed the Clans to 60-75%. Melee is at 70 and not 50 because at 50 holding a ford stopped paying off (defenders killed foes trickling out of a ford too slowly; the `holding_the_ford_pays_off` drill in `gc_ai` failed). The steady per-second morale drains (flank and rear shock, pinned, half strength, outnumbered) follow the melee pace (`BATTLE_PACE_PERCENT` in `morale::check`), so a regiment does not break before it has been hurt; casualty losses and recovery are unscaled. Fights (first contact to the end) now last about 105 to 140 s instead of 95 to 120 s, and whole battles about 170 to 200 s instead of 175 s. `SIM_VERSION` is 22; the goldens were re-blessed (the scripted Conquest golden marches at 145 s, not 150 s, since slower fights kept side 0 from reaching the enemy main building in 6 minutes).

The exact formulas are in [10 · Architecture](10-architecture.md#formations-and-combat).
