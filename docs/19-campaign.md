# 19 · Campaign and Exploration

Grubcraft is a game where you **explore the Reach and meet the people who live on it**: Grubfolk who trade with you or remember that you burned their village, mercenaries who fight for whoever pays, peoples who may become your friends or your enemies. This document designs where that happens, at two scales:

1. **Exploring inside a match.** Conquest maps get a frontier: places hidden in the fog that reward whoever goes looking, and the first meetings with the parties described in [18 · AI](18-ai.md#the-living-world-encounters). Small, and built with the living world in M4.
2. **The Campaign.** A third mode: a world map of the whole Reach, played over several game nights, where your faction marches out from its home, explores, meets friendly, neutral and mercenary peoples, makes promises and breaks them, and fights the battles that matter as real Field Battles. The world remembers everything, from the first season to the last.

Both are built on what [18 · AI](18-ai.md) already defines: **parties**, the **relations ledger** (stance, liking, trust, fading memories), **deals and contracts**, the **parley panel** and the **minds** that run on the host. Nothing here invents a second diplomacy system; the campaign gives that system a world big enough and a time long enough for it to matter.

**Built so far:** nothing; this is design only. What it builds on that exists today: the hostility test that reads a stance table (hook 1 in [18](18-ai.md#engine-hooks)), the battle AI, and the deterministic simulation that lets the AI and "Restage this battle" run a battle anywhere.

## Decisions this document proposes

Work proceeds on these recommendations unless Mario says otherwise. They are recorded in [14 · Decisions Needed](14-decisions-needed.md#campaign-and-exploration-proposed-2026-10-15).

| # | Decision | Recommendation | Why |
|---|---|---|---|
| 18 | **A campaign map**, which the [vision](01-vision.md#what-we-are-deliberately-not-doing) ruled out | Yes, as a third mode, kept light: a world map that gives battles a reason and gives the peoples of the Reach somewhere to live. No empire management | Mario wants exploring and meeting people at the heart of the game; Shogun's own campaign map is what made its battles matter |
| 19 | **How turns work** | Simultaneous turns ("seasons"): everyone plans at once, then everything moves together | Nobody waits for three friends to finish their turns |
| 20 | **How campaign battles are fought** | Each one is either fought as a Field Battle or handed to the battle AI on both sides, which plays it out in the real simulation in seconds and leaves a replay | Every battle can be played; none has to be; the quick result is still a real battle, not a dice roll |
| 21 | **When it is built** | Exploring inside a match with M4; the campaign as its own track (C) that starts once M4 lands, first playable with two factions alongside M5, complete after 1.0 as M7 | Exploration arrives as early as the systems it needs; 1.0 is not delayed |
| 22 | **Diplomacy between players in the campaign** | On by default (ceasefires, alliances, tribute and betrayal); fixed teams as an option | Long-term promises between friends are what a campaign has that a match doesn't |
| 23 | **Who lives in the Reach** | Start from the first drafts in [The peoples you meet](#the-peoples-you-meet) | A creative call; the system works with any cast |

## Design rules

The [pillars](01-vision.md#design-pillars) applied to a campaign:

1. **The battles are the game.** The map exists to make battles happen for a reason, between armies that have a history, on ground that was fought over. It never becomes a second game you have to master first.
2. **A season is a minute or two of decisions.** No tax sliders, no city-building chains, no agents, no family trees, no trade-route plumbing, no population happiness. The whole campaign has two resources (Grub and Marrow), up to three armies per player, and one thing to build per region.
3. **Nobody waits.** Everyone plans at the same time. When a battle starts, friends who aren't in it can command the other sides in it ([guest commanders](#guest-commanders)) or plan their next season.
4. **The world remembers.** One relations ledger runs the whole campaign. A village burned on the first night still fears you on the fourth; a company you cheated tells every other company.
5. **The same game everywhere.** Campaign battles are Field Battles with the same units, rules, controls and AI. The parley panel is the same panel. Anything learned in one mode works in the others.
6. **Lean.** The whole campaign state fits in under a megabyte, saves are a few kilobytes, and a battle loads as fast as a Field Battle ([performance](#architecture)).

---

## Part 1 · Exploring inside a match

Conquest maps already hold hamlets, creep camps, Shrines and Bloodseep sites ([07](07-world-and-maps.md#map-anatomy)). Exploring adds a **frontier**: a handful of places hidden in the fog that only reward whoever goes looking, so scouting is a choice with a payoff and not only a way to find the enemy.

### Frontier sites

Each Conquest map has 4 to 8 **site slots**. The match seed picks what appears in each, from the kinds allowed for that slot, so a map the group knows well still holds surprises. Sites are map objects and world rules in the simulation, with no minds of their own (companies aside).

| Site | Found where | What happens |
|---|---|---|
| **Ruined waystation** | Off the roads, behind creeps | The first party to bring a unit to it takes the cache (about 150 Grub and 40 Marrow) |
| **Lost caravan** | On a road between hamlets | A Grubfolk caravan beset by Titan-spawn. Rescue it (kill the creeps) and every hamlet remembers it kindly, and the caravan pays a reward; rob it for more loot and every hamlet remembers that instead; ignore it and it is gone in two minutes |
| **Hermit's hollow** | Remote corners | Pay 50 Grub to learn where the next Tremor will open its Bloodseep, a minute before anyone else's beacon |
| **Titan scar** | Near Bloodseep sites | A glowing wound; standing a unit on it reveals the land around it for the rest of the match |
| **Company camp** | Near a hamlet or a crossing | A mercenary company, met here for the first time (from M4, [18](18-ai.md#deals-and-contracts)) |
| **Relic barrow** | Guarded by a strong creep camp | A hero item, like the creep drops in [07](07-world-and-maps.md#creep-camps-titan-spawn), but always a good one |

### Discovery and first contact

- **First sight.** Sites, hamlets and companies are unknown until one of your units sees them. The first sighting raises an edge notification and a minimap ping, and is a ledger event: the party is now *met* (its stance to you moves from Unknown to its starting stance; see [Stance](18-ai.md#the-ledger)).
- **Rewards go to whoever gets there first**, which makes an early scout a real trade against an early worker.
- **Rumours** (a hamlet deal from [18](18-ai.md#deals-and-contracts)) reveal an area, so a Friendly hamlet becomes a way to explore.
- **Fair by construction.** The map validator checks that site slots are equally far from every start ([07](07-world-and-maps.md#map-format-and-tooling)).

That is the whole of in-match exploration: small, readable, and the same rules the campaign uses at a larger scale.

---

## Part 2 · The Campaign

### A game night in the campaign

**Season 1.** You lead the Ember Clans out of your Seat in the Rib Highlands. The map of the Reach is the Titan's whole body seen from high above, nearly all of it under painted cloud. You send your Pathfinder down the old road toward the Gut and march your one host to the nearest Grubfolk village. Across the table, your friends do the same with their own factions, at the same moment. Everyone presses Ready, and the season plays out: banners move, clouds peel back.

**Season 4.** Your Pathfinder finds a lost caravan on the Gut road and, beyond it, a camp of the Unbannered, a crew of disgraced ronin. You parley: their captain likes the Clans but doesn't trust anyone, so they want the fee up front. You pay. They are yours for six seasons.

**Season 9.** A friend's Brood host comes over the rib pass toward your new village. Both of you choose **Fight**. The campaign hands the two armies, exactly as they are, to a Field Battle on a map made from that pass: rib ridges, a ford, the village on the slope. The village likes you, so its militia joins your side, and your third friend, whose army is far away, takes command of it as a guest. You hold the ford. The Brood host limps home with half its regiments.

**Season 14.** The Titan shudders. A Bloodseep tears open in the Lungs, the richest region on the map for the next few seasons. Everyone wants it. The Heartbound, a cult nobody has met yet, come down from the Crown to stop anyone mining near their god. Your other friend, who promised you a ceasefire, uses the chaos to take your village. The Grubfolk will remember who protected them and who didn't, and so will you.

**Season 30, the fourth game night.** The Chronicle opens with "Previously in the Reach": a page of what happened, each battle linked to its replay. Whoever holds the Heart and two of the Great Places for three seasons wins the Reach.

### Shape of a campaign

| | |
|---|---|
| **Players** | 1 to 4, each one faction; any seat can be an AI player |
| **Modes** | **Rivals** (default): everyone for themselves, with diplomacy, or fixed teams. **Fellowship**: everyone together against the Reach's hostile powers and the Waking (Titan-spawn that grow each season) |
| **Length** | **Short**: about 30 seasons, two or three game nights. **Long**: about 60 seasons, five or six. A season takes 2 to 3 minutes plus any battle, so a night is 10 to 15 seasons and 3 to 6 battles |
| **Rivals victory** | **Crown of the Reach**: hold the Heart and two of the four Great Places for 3 seasons in a row. Or be the last faction with a Seat. If the season limit comes first, the most Renown wins (held places, battles won, promises kept) |
| **Fellowship victory** | Destroy the three great lairs before the Waking reaches every Seat |
| **Saving** | Automatic after every season. Every player's machine keeps a copy, so anyone can host the next night |

### The world map

The campaign map is **the Titan's whole body**, the Reach seen from high above, the way the [vision](01-vision.md) describes the land: rib ridges, hide plains, fur forests, rivers in old veins, the Heart at the centre. Every match map is a patch of this body; the campaign shows all of it.

It is made of **regions** joined by **routes**: about 24 regions for two players, 48 for four. A body template is hand-made (what is where on the Titan); what lives in each region is drawn from the campaign seed and checked for fairness by the map validator, so every campaign is a new Reach with a familiar shape.

| Area of the body | Ground (decides the battle maps) | Who tends to live there |
|---|---|---|
| **The Crown** (skull) | Highlands, crater lakes | The Heartbound; Titan-spawn lairs |
| **The Ribs** | Ridges, passes, terraced slopes | Grubfolk villages, ronin companies; good Ember Clans country |
| **The Lungs** | Vast caverns open to the sky | Bloodseeps after Tremors; the richest and most fought-over ground |
| **The Gut** | Bogs, slow rivers, rich in Grub | Grubfolk towns, Titan-spawn; good Brood country |
| **The Spine** | Mountain chain, rich in Marrow | Marrow golem lairs; few people |
| **The Hands** | Fur forests, deep and dark | The Discordant; hunters' companies |
| **The Tail** | Wastes and long roads | The Saltwander Caravan; the Bazaar of the Reach |
| **The Heart** | The centre, ringed by the four Great Places | The prize: whoever holds it and the Great Places wins |

**A region** has:
- a **ground** type that decides the battle map made there;
- a **yield** of Grub and Marrow each season, which runs down over the campaign (seeps run dry here too, falling to dregs at 25%), so nobody can sit still;
- at most **one feature**: a Seat, a settlement, a company camp, a lair, a site or a Great Place;
- an **owner**, if anyone holds it (see [What you have](#what-you-have));
- one **Work**, if its owner has built one.

**Routes** are roads (fast), passes (slow, and a pass battle map), fords (a river battle map), and, for the Brood only, burrows. Moving along a route costs a host's movement for the season, so where routes run decides where armies meet.

**What you know about a region:**

| Knowledge | Means |
|---|---|
| **Unknown** | Under cloud. You don't know what is there |
| **Rumoured** | Someone told you: you know its ground and its feature, roughly |
| **Scouted** | You have seen it, but what you saw is dated ("as of season 12") |
| **Watched** | One of your hosts, your Pathfinder, your Seat, a Watchtower, an ally or a Friendly settlement sees it now |

You see other parties' hosts only where you watch. That fog is what makes Pathfinders, Friendly villages and alliances worth having.

### What you have

| | What it is | Limits |
|---|---|---|
| **Seat** | Your home region and capital. Tiers 1 to 3 (Clan Hall, Castle Keep, Daimyo's Citadel for the Clans, and so on), unlocking regiment types as in a match | One. Lose it and you have 3 seasons to found a new one or you are out, like the 90 s rule in [Conquest](06-game-modes.md#victory-conditions-lobby-options-both-on-by-default) |
| **Hosts** | Armies on the map, each led by one of your heroes, carrying real regiments with their soldiers, veterancy and items | Up to 3 (one per hero, as in a match), each up to 20 regiments, about a Grand Battle (8,000 points) |
| **Pathfinder** | A fast scout that can't fight and can be caught. Moves twice as far as a host | One; a second at Seat tier 2 |
| **Grub and Marrow** | The same two resources as everywhere else | No other currencies |
| **Regions held** | A region is yours once your host has stood in it for a full season with no hostile host there. Settlements are never owned (except by the Concord's Hostile Takeover); you win their friendship instead | |
| **Works** | One per held region: **Granary** (+50% Grub), **Mine** (+50% Marrow), **Watchtower** (watches neighbouring regions), **Road** (routes from here are fast), **Fort** (battles here give the defender a palisade and towers) | One per region, one built per season |

**Recruiting.** New regiments are raised at your Seat (any type your tier allows) or hired at Friendly settlements (their militia and local types). Raising takes one season. Damaged regiments refill in any region you hold or any Friendly settlement, paying for the soldiers replaced (reinforcement, CMB-9 in the [specification](02-specification.md)).

**Upkeep.** Every regiment costs Grub each season, and mercenaries cost more. Upkeep rises faster than income as you grow, so the biggest faction can't simply snowball.

**Each faction's economy, at campaign scale.** The [four economies](01-vision.md#2-four-economies-four-different-games) carry over, each as one simple twist:

| Faction | On the campaign map |
|---|---|
| **Ember Clans** | Granaries and Roads cost half. Honor earned in campaign battles is kept between battles and spent on campaign Edicts (Rice Tax, Forced March of a host) |
| **The Brood** | Held regions joined to the Hive by an unbroken chain of held regions yield 25% more (the vein network); a raid that cuts the chain cuts the bonus. Brood hosts can move by burrows |
| **The Choir** | Three held regions that form a triangle make a chord: every region inside it yields more, and one lost corner breaks it |
| **Carrion Concord** | Caravans run between Friendly settlements and pay each season; Reliquaries collect Tithe from every battle fought within sight, anyone's |

### A season

A campaign turn is a **season**. (A "turn" already means one 50 ms simulation step; see the [glossary](glossary.md).)

```
  ┌─────────── Plan ───────────┐   ┌──── Resolve ────┐   ┌──── Battles ────┐   ┌── The world ──┐
  │ everyone at once, 90 s     │──▶│ all moves play  │──▶│ fight each, or  │──▶│ income, upkeep,│
  │ (ends when all are Ready)  │   │ out together    │   │ let the generals│   │ memories fade, │
  │ move, recruit, build, deal │   │ (about 20 s)    │   │ fight it        │   │ Titan events   │
  └────────────────────────────┘   └─────────────────┘   └─────────────────┘   └────────────────┘
```

1. **Plan.** Everyone at once. Drag a host or the Pathfinder along routes as far as its movement allows; recruit; start one Work; open a parley with any party you have met. Each host has a **posture**: *Attack* (fight any hostile host it meets), *Cautious* (avoid a fight it would likely lose, using the battle estimator from [18](18-ai.md#captain-battle-tactics)), or *Parley* (stop and talk to whoever it meets). The plan timer defaults to 90 s and ends early once everyone presses Ready; it can be turned off for relaxed nights.
2. **Resolve.** Every party's moves, including the minds of companies and peoples, play out at the same time in small steps. Hosts that meet stop. A meeting between hostile parties where either side's posture is Attack becomes a battle; any other meeting becomes an **encounter** (below).
3. **Battles.** Each battle this season is listed with both sides' armies as the players know them. For each one, the players in it choose:
   - **Fight**: play it as a Field Battle ([Battles on the campaign](#battles-on-the-campaign)). If any player in it chooses Fight, it is fought.
   - **Let the generals fight**: the battle AI commands every side, in the real simulation, at full speed. It takes a few seconds and leaves a replay anyone can watch.
   Small fights (under 1,000 points a side) default to Let the generals fight. Battles are fought one after another; the rest of the table plans its next season or [guest-commands](#guest-commanders).
4. **The world.** Instantly: income and upkeep are paid, contracts move their payments, memories fade a season, settlements whose liking crossed a line change stance, lairs grow, and Titan events happen.

**Titan events on the campaign** are the match events at a larger scale: a **Tremor** about every 8 seasons opens a Bloodseep in one region (double yield for several seasons, announced a season before); the **Heartbeat** every 12 seasons heals every host in the Heart and its Great Places; a **Marrow Bloom** refills a Spine region.

### Exploring the Reach

- **Clouds lift as you go.** Hosts and Pathfinders reveal the regions next to them. Rumours bought from Friendly settlements reveal regions further away, caravans and companies gossip, and allies share what they see.
- **Sites** are the in-match [frontier sites](#frontier-sites) at campaign scale, each a short **encounter card** with up to three choices. Every choice shows what it costs and what it brings before you pick it; some lead to a battle. For example:

  > **A wounded Grubfolk peddler on the Gut road.** *Help him* (−50 Grub; every Grubfolk settlement likes you more) · *Take his cart* (+120 Grub; every Grubfolk settlement remembers) · *Leave him*.

  Encounter cards are data (`data/campaign/encounters.ron`), so new ones are content, not code. No hidden dice: any chance involved is shown as a number.
- **Relics** found at sites are hero items, carried by that hero into every battle.
- **First contact** with a party is a ledger event, as in a match: you learn who they are, where their home is, and what they think of you.

### The peoples you meet

Mario's three kinds of people are all here: **friendly**, **neutral** and **mercenary**, plus the hostile ones that give everyone a common enemy. They are all parties from [18](18-ai.md#parties-everyone-who-can-own-an-army), so they run on the same ledger and deals. The minor factions below are first drafts for decision 23.

| People | Kind | Starts as | Wants | Offers | If angered |
|---|---|---|---|---|---|
| **Grubfolk villages and towns** | Settlement (rules) | Neutral, mildly warm; a little warmer to the Clans, cooler to the Concord | Safety from Titan-spawn and raiders; steady trade | Trade, rumours, militia to hire, a place to refill regiments, a share of tribute for protection | Refuse trade, shoot at your hosts, tell every other village |
| **The Bazaar of the Reach** | Settlement | Neutral | Peace on the roads | The best prices, rare relics; the **Mercenary Exchange**, where companies post their price | Closes its gates and its Exchange to you |
| **Mercenary companies**: the Stewpot Free Company (Grubfolk), the Gutwhistle Tamers (Titan-spawn tamers), the Unbannered (disgraced ronin), and a few more | Company (mind) | Neutral | Pay, a winning employer, a captain's sense of honour | Regiments for a fee plus upkeep per season, for a set number of seasons | Desert, sell out to a higher bidder (if their honour is low), tell every other company who cheats |
| **The Heartbound** | Faction (mind) | Wary to all | Nobody mining or fighting near the Heart and the Great Places | Zealot regiments that never rout; the Heartbeat's blessing on your hosts | Holy war on whoever mines the Spine near the Heart |
| **The Saltwander Caravan** | Faction (mind), moves each season | Neutral to all | Open roads, no raiders | The best trade terms, guides that reveal routes, news from far regions | Leaves your lands and takes its trade elsewhere; rival of the Concord |
| **The Discordant** | Faction (mind) | Friendly to the Choir, Wary to others, Hostile to the Concord (who sold their dead) | To be welcomed back into the chord, or to be left alone | A Choir player can reconcile them and gain their regiments; anyone else can ally with them | Raids from the Hands |
| **Titan-spawn lairs** | Wildlife (rules) | Hostile to all | Nothing | Relics and Marrow when cleared | Unlike in a match, lairs **grow** each season left alone and, once big, send hunting packs into neighbouring regions. In Fellowship mode, three great lairs drive the Waking |
| **The other players' factions** | Players (humans or AI) | From the lobby (Rivals: Hostile unless allied) | To win the Reach | Every deal in the table below | Everything |

### Relations over time

The campaign keeps **one relations ledger for the whole campaign**, saved with it ([18](18-ai.md#relations) defines it). What changes is only the clock:

- **Memories fade by season, not by minute.** The event table in `data/relations.ron` gets a second half-life column for the campaign. First-pass values:

  | Event (B did this to A) | Fades by half in a match | On the campaign |
  |---|---|---|
  | Traded with A | 5 min | 2 seasons |
  | Attacked A's units | 10 min | 4 seasons |
  | Raided (razed) A | 30 min | 12 seasons |
  | Honoured a contract to the end | 20 min | 8 seasons |
  | Broke a contract | 40 min | 16 seasons |
  | Fought A's enemy | 5 min | 3 seasons |

- **Battles feed the ledger.** A campaign battle starts with the ledger's view of every party in it and hands back the memories it made: the village whose militia you protected, the company that watched you rout its rival.
- **Word spreads** to kin exactly as in [18](18-ai.md#memories-and-time). On the campaign, breaking an alliance or a ceasefire between players also spreads, at half, to *every* party that knew both of you: the whole Reach hears that you are not to be trusted.
- **Fear matters more.** Fear is worked out from the strength of your hosts near a party, so the biggest army on the map is courted by some and quietly opposed by others. Minds lean against whoever is winning most, which keeps a runaway leader in check without a rule saying so.
- **Your name in the Reach.** A panel lists what each people you have met thinks of you and the top reasons ("You burned Millbrook", "You paid the Unbannered in full"), from the same ledger.

**Deals on the campaign** are the deals from [18](18-ai.md#deals-and-contracts), with terms in seasons instead of minutes:

| Deal | Between | On the campaign |
|---|---|---|
| **Trade** | Anyone and a settlement or faction | Grub for Marrow at the market price, or a standing trade every season |
| **Hire** | Anyone and a settlement or company | Fee up front, upkeep each season, for 3 to 10 seasons |
| **Rumours** | Anyone and a settlement or the Caravan | Reveal regions out to a distance |
| **Passage** | Anyone and a party with land | Your hosts cross their regions without trespass memories |
| **Protection** | A settlement and anyone | It pays you a share of its yield each season; you must fight anyone who attacks it, or it counts as a broken contract |
| **Tribute** | Anyone and anyone | A payment each season, for peace |
| **Ceasefire, alliance** | Players and factions | Ceasefire: no battles for a set number of seasons. Alliance: shared vision and the **call to arms** below |
| **Call to arms** | Allies | An ally's host in a neighbouring region with posture *Support* marches into your battle as reinforcements, commanded by that ally (or their mind) |

As everywhere, the answer to any offer to a world party is the shared rule, shown in the parley panel before you commit. Offers to a human player are theirs to answer.

### Battles on the campaign

A campaign battle is a **Field Battle built from the campaign's state**:

- **The armies are the hosts, exactly as they are:** every regiment with its surviving soldiers, morale, veterancy and items, and the heroes with their levels. This is "Restage this battle" ([06](06-game-modes.md#restage-this-battle-the-bridge-between-modes)) in the other direction: instead of a snapshot from a Conquest match, the snapshot comes from the campaign.
- **The map is made from the region:** a 64 × 64 to 96 × 96 battle map generated from the region's ground, its route (a pass, a ford, a road) and its feature (the village on the slope, the Fort's palisade), seeded so that the same region always gives the same land. Armies deploy on the side they came from.
- **Everyone present takes part.** A settlement in the region fields its garrison as its own party, with its stance from the ledger: it may join the side it likes, shoot at the side it hates, or stay out of it. Companies under contract fight for their employer. Allied hosts answering a call to arms arrive at the edge of the map after a delay, as reinforcements do in Total War.
- **Assaulting a Seat** is a **stronghold battle**: the defender also gets their Seat's main building, towers and walls from the Conquest structure set, and holds the field if the main building stands at the end. (An option plays Seat assaults as a short Conquest match instead, Swift tempo, 25-minute limit, for groups that want a big-night finale.)
- **Victory** is the Field Battle's: rout, or victory points at the time limit.

**After the battle**, the result flows back to the campaign: survivors per regiment, veterancy and hero experience gained, relics dropped, memories made. The winner holds the region; the loser's host falls back to a neighbouring region it can reach, or surrenders if it can't. Routed regiments survive with their losses; shattered ones are gone. A hero who falls is **wounded** for a few seasons, not killed, unless the campaign was started as an **Iron campaign** (heroes can die, no reloading).

**Letting the generals fight.** The battle is set up exactly as above and played by the battle AI for every side at the General level, the same level for everyone so that it favours nobody. The host machine runs it at full speed; the AI's commands travel to every machine like any other commands, and every machine runs the deterministic simulation, so the result is identical everywhere. A ten-minute battle takes a few seconds. Why not a formula, as most strategy games use: the real simulation respects ground, formation, flanks and morale, so a good army on good ground wins for the reasons it would win if you fought it, and watching the replay shows why.

#### Guest commanders

While two friends fight a battle, the others don't have to wait. Any side in the battle not controlled by a person in it (an allied host's regiments when its owner is busy, a company, a settlement's militia, a minor faction that joined in) can be **taken by a guest**: an uninvolved player commands it, within its own goals (a guest commanding a village's militia can't march it off to attack the village's friends). Anyone who doesn't take a side can watch with the free camera, or plan their next season. This uses command authority (hook 2 in [18](18-ai.md#engine-hooks)), the same rule that lets an employer command hired mercenaries.

### AI on the campaign

- **AI players** get a **campaign mind**: the Sovereign, Steward and Marshal from [18](18-ai.md#inside-a-mind) working at the scale of regions and seasons (a new module, `gc_ai::campaign`), plus the Envoy as it is. A season gives it far more time to think than a match does, but it sees only what its faction knows, like everyone else.
- **Companies and minor factions** run the minds and personalities they already have in [18](18-ai.md#personalities-and-faction-doctrine). On the campaign they also move: companies wander between settlements looking for work, the Saltwander Caravan travels its route, the Heartbound guard the Heart.
- **Settlements and lairs** are rules, as in a match.
- **A friend who can't make it** to game night: their faction is run by a **caretaker** mind that defends, keeps every deal and never starts a war or breaks a promise, so nobody is betrayed by a stand-in. Or the host simply waits for them.

### The campaign screen

- The map is a painted diorama of the Titan's body, drawn with the same terrain renderer as the battles; hosts are banner figures, like regiments in the [Commander View](08-ui-ux.md); fog is painted cloud.
- A **season ribbon** along the top: the season, Grub, Marrow, upkeep, the plan timer, and one Ready light per player.
- Selecting a region shows its ground, yield, feature, owner and what you know about it, and how old that knowledge is.
- The **parley panel** is the one from [18](18-ai.md#the-living-world-encounters), unchanged.
- **Encounter cards** for sites and first meetings; the **battle list** with Fight or Let the generals fight for each.
- **The Chronicle**: the campaign's history, one line per thing that mattered ("Season 14: the Unbannered deserted at Rib Gate"), each battle linked to its replay. Each night opens with *Previously in the Reach*, a short summary since the last session.
- The command card rule still holds: at most 12 buttons on anything you select.

---

## Architecture

- **`gc_campaign`**, a new crate: a deterministic, integer-only simulation like `gc_sim`, but advanced one season at a time. Its state: regions (at most 128), parties (at most 64), hosts (at most 64), the regiments in them (at most 1,500), the ledger (64 × 64 pairs at 4 bytes: 16 KB), a memory pool (2,048 at 16 bytes: 32 KB), contracts, and the Chronicle. Well under a megabyte, hashed after every season.
- **Networking** is the match's lockstep transport at a slower beat: during Plan, each player's orders are commands; at the end of Plan the host sequences them into one bundle; every machine resolves the season identically and compares hashes, as in a match.
- **Handing a battle to `gc_sim`.** The campaign builds a `MatchSetup` with the parties in the battle (mapped onto the match's 32 party slots), their stances and ledger rows, every regiment with its state (hook 6, *spawn with state*), and the generated map's seed. When the battle ends, a `BattleResult` (survivors per regiment, veterancy, experience, relics, memories raised) comes back as a campaign command. Both steps are deterministic, so they happen identically on every machine.
- **One ledger implementation.** The ledger, memories, contracts and the offer rule are written once (planned in `gc_sim` for M4, hook 7) with the fading clock as a parameter, so a match ticks it per second and the campaign per season.
- **Saves:** the campaign state plus the Chronicle, a few kilobytes; each battle's replay is kept beside it (about 100 KB each) for the Chronicle links.
- **Memory:** the campaign screen and a battle are never loaded together; switching unloads one before loading the other. Both sit far inside the 4 GB budget.
- **Tests:** golden campaign seasons in CI, like golden replays: a fixed seed and command list must give the same hash on Mac, Windows and Linux.

### Engine hooks this needs

| # | Hook | Also needed by | When |
|---|---|---|---|
| — | Spawn with state (hook 6 in [18](18-ai.md#engine-hooks)) | Rehearsal, Restage, save games | M2 |
| — | Command authority (hook 2) | Mercenary contracts, taking over a dropped player | M4 |
| — | Parties, ledger and contracts in the sim (hook 7), with the fading clock as a parameter | Hamlets, companies | M4 |
| C1 | **Frontier sites:** site slots in the map format, site rules and first-sight events in the sim | In-match exploration | M4 |
| C2 | **Battle map generator:** a seeded Field Battle map from a region's ground, route and feature | The campaign; the seeded map generator ([07](07-world-and-maps.md#map-format-and-tooling)) shares its parts | C-1 |
| C3 | **`BattleResult`:** what a battle hands back (survivors, veterancy, experience, relics, memories) | Restage's post-game screen | C-1 |
| C4 | **Headless full-speed battle with AI on every side** (most of it exists in the AI's lab mode and the arena) | Let the generals fight | C-1 |

## Build plan

The campaign runs as its own track (**C**) beside the milestones, like the [AI track](13-roadmap.md#the-ai-track). It lives mostly in its own crate (`gc_campaign`) and its own AI module (`gc_ai::campaign`), so a parallel thread can build it without touching the code other threads are changing.

| Phase | What | When | Who (see [15](15-staffing-plan.md)) | Done when |
|---|---|---|---|---|
| **C-0 · Exploring inside a match** | Frontier sites, first sight and first contact as ledger events; site slots in the map validator | With M4 (world systems thread) | Opus for the sim rules; Sonnet for site content | Each launch Conquest map has its sites; scouting pays for itself in the arena |
| **C-1 · Campaign core** | `gc_campaign`; the world map from a body template and a seed; seasons; hosts, Pathfinder, Seat, Works; exploring; Grubfolk, companies and lairs on the ledger; Fight or Let the generals fight; save and load; Clans and Brood; 2 players and AI | Starts once M4 lands, alongside M5 | Opus, high effort; Sonnet for the campaign screen; Haiku for encounter-card data | Two friends play a Short campaign over two nights without a desync |
| **C-2 · Peoples and diplomacy** | The minor factions with their minds (this is AI-6 in [18](18-ai.md#build-plan)); diplomacy between players; alliances and the call to arms; guest commanders; the Chronicle | M7, after 1.0 | Opus, max effort for the diplomacy minds; Sonnet adds peoples as content | A four-player Long campaign where an alliance forms and breaks, and the Reach remembers it |
| **C-3 · The full campaign** | Choir and Concord campaign twists; Fellowship mode and the Waking; stronghold battles; Iron campaign | M7 | Opus; Sonnet for content and screens | Every faction is playable on the campaign; a Fellowship campaign can be won |

If Mario wants the campaign in 1.0, C-2 and C-3 move before M6 and 1.0 moves back by about one milestone.

## Mistakes we design against

| Mistake | How the design avoids it |
|---|---|
| The campaign grows into a second, harder game | Hard limits: two resources, three hosts, one Work per region, 12 buttons |
| Friends waiting on each other | Simultaneous seasons; guest commanders; planning during others' battles |
| Too many battles, or battles that don't matter | Only hostile meetings make battles; small ones default to the generals; Cautious hosts avoid hopeless fights |
| Quick results that feel unfair | The generals fight in the real simulation, at the same level for both sides, and the replay shows why |
| One faction runs away with it | Rising upkeep; seeps running dry; fear makes the Reach lean against the leader |
| A campaign that never ends | The Crown victory, a season limit, and the Waking in Fellowship |
| Forgetting what happened between game nights | The Chronicle and *Previously in the Reach* |
| A bug in a mind desyncs the campaign | Minds act only through commands, as in a match ([18](18-ai.md#the-big-decision-rules-in-the-simulation-minds-outside-it)) |

## Open questions

Smaller questions for later, each with a recommendation:

- **The Field Battle ladder** (in [14](14-decisions-needed.md#smaller-questions-for-later)): a Short campaign with only Field Battles covers it. Recommended: fold it into the campaign.
- **Legacy between campaigns** (the next campaign's peoples remember the last one)? Recommended later, as an option.
- **Two battles at once** on different machines when they involve different players? Recommended later; one after another first.
- **Hidden AI personalities** on the campaign, discovered by meeting them? Recommended yes for minor factions, shown for AI players as in the lobby.
