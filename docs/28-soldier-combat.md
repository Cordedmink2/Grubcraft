# 28 · Every Soldier Fights

When two regiments clash, every soldier picks a man and fights him: guard, swing, block, stagger, kill, as in Total War and Mount & Blade, instead of two blocks of bodies leaning into each other. This is idea 20 on Mario's list (2026-10-23).

**Decided (Mario, 2026-10-23):** option B, **duels in the simulation**, is the official plan and is built first when the project resumes. Option C, **every soldier simulated**, is a later stage on top of it. **Nothing in this document is built yet**; the project is paused. The concept page with sketches, a playable before-and-after toy and the sources is [Every Soldier Fights](https://claude.ai/artifact/3faYPzf1Njf69cHa4fWyUc); its files are in the project files under `design/soldier-combat/`.

Numbers here are first-pass and move in balance passes, like everywhere else in these docs.

## Where the game stands (2026-10-23, main at `7084966`)

About a third of the look is already built, all of it on screen only ([09 · Animation](09-art-and-audio.md#animation), [10 · The two-tier simulation](10-architecture.md#the-two-tier-simulation)):

- **Built:** front-rank duel pairs that face each other and swing out of step; blows on screen drawn from the simulation's own hit chance (parries, shield blocks, staggers, crush-through, charge knockdowns, the partner's killing blow); champions before battle; the fallen and spent arrows stay for the whole battle.
- **Half there:** only the front rank fights. Everyone behind stands in his slot, and no soldier strays more than 0.6 tiles from it (`DUEL_LEASH` in `gc_game/src/puppets.rs`), so the meeting line stays straight. Each soldier has ten clips (`ClipKind` in `gc_assets/src/model.rs`: idle, walk, run, two attacks, hit, die, cheer, shoot, block).
- **Missing:** the battle maths doesn't know who fights whom. Melee strikes once a second per soldier who can reach, with `hit_percent` = 50 + attack − defence kept between 10% and 90% (`gc_sim/src/combat.rs`), and the damage goes to the regiment as a whole: `take_damage` (`gc_sim/src/regiments.rs`) removes hit points from the back ranks forward. The screen then picks a front-rank man to fall, so two players can see different men die. There are no matched kills, no bodies pushing each other, no men knocked back by a charge, no feet placed on slopes and no heads turning toward the enemy.

## What we learned from Total War and Mount & Blade

From developer talks, official and community modding documentation, measurements players publish, and watching the games. **We do not reverse-engineer or copy other games' files** (Mario's rule of 2026-10-13; on 2026-10-23 we noted that fair use does not cover lifting another game's animations even for a non-commercial game, New Zealand has only narrow fair dealing, and the repository is public). The source list, marked read or search-only, is on the concept page and in `design/soldier-combat/sources.md`.

- **Every man has his own fight.** Total War rolls each soldier's blow against his own target (players measure about 35 + melee attack − melee defence, kept between 8% and 90%). Mount & Blade makes every soldier a full character.
- **The line breaks into a scrum** two or three men deep at contact, and the men behind step up as gaps open.
- **Matched kills** (two soldiers playing one move together) are loved but costly: players rate Shogun 2's the best for ordinary soldiers; Warhammer, Troy and Three Kingdoms keep them mostly for heroes and big monsters. Long ones lock men in place and make fights feel random, so ours stay short.
- **Bodies have weight.** In Warhammer II a collision's force comes from the two bodies' speed and their difference in mass.
- **Blows interrupt**: a hit staggers and cancels the victim's swing, and heavy weapons crush through blocks (Mount & Blade; we already have it on screen).
- **Smooth, varied movement** comes from many clips per weapon, layering (legs apart from arms), additive flinches, inertialization (Gears of War) and foot IK, and **detail goes where the camera looks**.
- **Not to copy:** Mount & Blade's big melees turn into blobs where formations vanish in seconds. Grubcraft keeps regiments readable from the commander's height.

## The three ways, and the decision

| | A · Look only | **B · Duels in the sim (chosen)** | C · Every soldier simulated (later stage) |
|---|---|---|---|
| What the battle maths knows | Regiments, as today | Regiments, plus who fights whom inside a locked melee | Every soldier as a body with position, weight and collisions |
| On screen | Total War-like up close | The same, plus gaps where men really died and two-on-ones that really cost | The most natural: charges plough and scatter, lines bulge and wrap round a flank |
| Who decides who dies | The screen | The simulation | The simulation |
| Balance and replays | Unchanged | Balance tables re-run, golden replays re-recorded, replay format bumped | Start again |
| Simulation cost | None | Small: about 2,000 men in melee in the biggest benchmark battle, one blow each a second or so (to be measured) | Large: about 6,000 bodies colliding every tick, perhaps ten times today (to be measured) |
| Risk | Low | Medium | High |

Close up, B and C look the same: the duels, staggers and kills are animation either way. They differ in how the crowd moves. In B the scrum is a ragged band and the blocks behind still move as blocks; charge impacts are decided by the maths and played on screen. In C the bodies really collide, so those things happen by themselves.

**Why B first, then C (Mario agreed 2026-10-23):** almost all the work carries over to C. The scrum, the animation engine, the fight library, matched kills and effects are the same for both, and C also needs B's per-soldier pairing, blows and hit points. What B-then-C wastes is some glue that keeps pairs inside a block and one extra round of balance tables and replay re-recording. In return there is a playable result much sooner, battles never sit broken for weeks, and we learn whether B already looks good enough. B's soldier records are designed so C only adds bodies and collisions.

## Option B: duels in the simulation

Blocks still march, turn, push, brace and rout as blocks, so pathfinding, formations, lines that push, the computer players and the balance work all carry over. Only the inside of a locked melee changes.

- **Pairing.** When two regiments lock in melee, the simulation pairs soldiers one to one across the contact line: the front rank, the second rank of units with reach (spears), and two-on-ones where one line overlaps the other's flank. Pairing is deterministic (slot order, fixed-point distances, PCG32 substreams), recomputed only when a pair breaks.
- **Blows.** Each pair trades blows on its own clock (about one a second, offset per soldier), using the same hit chance and damage rules as today, including flank, rear, charge, formation and morale modifiers.
- **Deaths.** A soldier dies when his own hit points run out, so the man who falls is the man who was beaten. The slot he leaves is filled from behind as now.
- **Events to the screen.** The simulation reports every blow (striker, target, landed or parried, stagger, crush-through, kill), and the screen plays exactly that, so every player and every replay sees the same fight.
- **What it changes.** Kills come a little later and in bursts, because damage no longer piles onto one man at a time, so the balance tables (faction, team and terrain) are re-run and retuned. `SIM_VERSION` is bumped, the golden replays are re-blessed, the replay format changes, and the `field-8000pt-2v2` benchmark is checked against the tick budget (combat and morale 0.5 ms of the 2.0 ms p99).
- **Ready for C.** Each soldier gets a small record in the simulation (slot, hit points, partner, blow clock), laid out so C can later add a position, velocity and mass without reshaping it.

## On screen

- **The scrum.** Lines loosen into a band two or three men deep. Every man in reach finds a partner, two-on-ones form where lines overlap, rear ranks press in and step into gaps, spacing follows weapon reach (spearmen hold a spear's length, Brood claws get right on top of their man), and men waiting their turn shift their weight and shout.
- **Exchanges, not single swings.** Each pair plays short exchanges of two to four moves (feint, swing, block, shove, riposte) chosen to end on the blow's result. Good defence parries more, veterans stagger more, wavering regiments fight sloppily.
- **Matched kills.** When a blow kills, killer and victim play one move together, lined up during the wind-up by motion warping so the blade meets the body. About a second long; heroes and champions get longer ones. Played near the camera and in the ride-with-the-banner view; far away, a plain swing and fall.
- **Charges that hit home.** Riders and monsters knock men back or flat, the line bends around the wedge, riders slow and start cutting, and braced spears stop a charge dead (the weights already come from the maths).
- **One soldier, several layers.** Legs play footwork while arms swing, flinches are added on top from the side the blow came from, heads and chests turn toward the man, feet sit on slopes, moves blend smoothly and each man plays them a touch faster or slower, mirrored or not. Technically: a pose step on the graphics card mixes a few clips per soldier before skinning, and clips are stored once per skeleton and shared, so the library costs megabytes, not hundreds of them.
- **Each faction fights its own way** (to confirm with Mario): Ember Clans keep disciplined spacing and thrust Yari from the second rank; the Concord's dead barely flinch and collapse into bones; Choir crystal chips and shatters; Brood Skitterlings leap two or three onto one man and the Colossus Maw tosses men aside.

## The fight library and where moves come from

A human weapon type (sword and shield, spear, two-hander, polearm, axe or mace) needs about 50 moves: guards and idles, attacks from four directions, parries and blocks, hits from the front, sides and back, stagger, knockdown and get-up, eight or more deaths, footwork and six or so matched kills. Brood and Choir bodies need about 30 each, made for their own skeletons in Blender as now.

- **Mixamo** (Adobe): free, commercial use allowed, no credit needed; Mario downloads, we fit to our skeletons in Blender. Raw files may not be redistributed as an asset pack, and may not train AI.
- **Bought motion capture**: only packs whose licence allows our own engine (some Unreal Marketplace and Unity "Restricted" packs don't).
- **Phone video to motion** (Rokoko, Move AI, Plask, DeepMotion): terms unclear or paid-only for commercial use; get a written answer first.
- **Blender keyframes**: right for insects and crystal bodies, stiffer for human swordplay.
- **Non-commercial sets**: Mario said on 2026-10-23 that Grubcraft is non-commercial, so sets licensed for non-commercial use may be usable; sets that forbid derivatives (such as Ubisoft's LAFAN1) are still out, because fitting moves to our skeletons makes a derivative.
- **Never**: moves taken from other games.

## Build stages

Each stage is one or two pull requests. Stages marked *sim* change the battle maths and run the full golden-replay and balance checks.

1. **The scrum** (screen only). The band, partners for every man in reach, two-on-ones, rear ranks stepping into gaps, weapon spacing, waiting men alive. Uses today's ten clips, so it shows quickly. Opus lead with a Sonnet helper.
2. **Duels in the sim** (*sim*, option B). Pairing, per-pair blows, per-man deaths, blow events, balance tables re-run, goldens re-blessed, replay format bumped, benchmark checked. Opus (battle maths stays with the lead), Haiku for benchmark runs. Two pull requests.
3. **The animation engine**. Many tagged moves per skeleton, layers, additive flinches, smooth blends, foot IK, look-at, and a picker that chooses moves by weapon, direction and result; a new model format. Sonnet 5.5 at high effort, in the materials and animation thread of the [restart plan](29-restart-plan.md). Can run beside stage 2.
4. **The fight library**. About 50 moves per human weapon type and 30 per Brood and Choir body, one faction at a time, starting with the Ember Clans. Sonnet 5.5 alone (Mario's 3D modelling rule, 2026-10-23). Needs Mario's call on where moves come from.
5. **Matched kills and impacts**. Paired kills with motion warping, charge knock-backs and tumbling bodies, crush-through finishers, hero and champion moves, extra detail near the camera.
6. **Sparks, blood and sound**. Sparks on parries; blood, ichor, crystal shards or bone dust by faction; hit sounds by weapon and armour. Shares particles with the cinematic lighting plan and sound with the positional soundscape idea.
7. **Every soldier simulated** (*sim*, option C, a later stage). Soldiers become bodies in the simulation with position, velocity, mass and collisions; regiments steer their men. Planned separately as a milestone of its own once B is in and Mario has seen it.

## Open calls for Mario

Decided: B now, C later (decision 29 in [14](14-decisions-needed.md#every-soldier-fights-decided-2026-10-23)). Still open, each with a recommendation the work follows unless Mario says otherwise:

- **Where the moves come from:** Mixamo plus Blender, a bought pack later if swordplay still looks stiff.
- **Blood:** stylised (tapestry-like splashes) by default, full as a setting.
- **What first when the project resumes:** the scrum (stage 1, quick and visible), with stage 2 straight after; or stage 2 first if Mario wants the maths right before the look.
