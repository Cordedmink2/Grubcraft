# 01 · Vision

> **Grubcraft** is a four-player LAN real-time strategy game where Warcraft III's base-building meets Shogun: Total War's regiment battles, built on an engine so lean it runs a full 4-player war in less memory than a browser tab.

## The pitch in one paragraph

Long ago a god-sized creature fell out of the sky and died. Its body became the land, and its slow, endless rot-and-regrowth seeps a glowing, edible, alchemically potent ooze the locals call **Grub**. Everything in the Reach runs on Grub: rice paddies, insect hives, singing crystal machines, and the ledgers of merchant-necromancers. You and up to three friends each lead one faction, each with a *completely different way of turning Grub into power*, and you fight over the corpse of a god in matches that last 20 to 60 minutes.

## Design pillars

These are the tie-breakers. When two ideas compete, the one that serves more pillars wins.

### 1. Warcraft macro, Shogun micro
You build a base, gather, tech up and expand like Warcraft III. But your army is made of **regiments**: blocks of 8 to 24 soldiers you command as one unit, with formations, facing, flanks, charges and morale, like Total War: Shogun. Heroes are the exception: single, powerful, levelling characters in the Warcraft mould.

Why this matters: regiments give the battlefield the "commander of an army" feel you asked for, and they also *cut the click burden*. Moving 12 regiments is 12 decisions, not 200. That is how the game stays deep without becoming a clicking contest.

### 2. Four economies, four different games
Every faction harvests the same two resources (Grub and Marrow), but *how* they do it is a different mini-game:

| Faction | Economy in one line | What you're really doing |
|---|---|---|
| **Ember Clans** | Workers carry Grub to granaries | Classic, readable, protect your workers |
| **The Brood** | Burrowed grubs feed a living vein network | Defend a network; raiders cut veins, not workers |
| **The Choir** | Crystal pylons form triangles; income scales with enclosed area | Territory geometry; one dead pylon breaks a chord |
| **Carrion Concord** | Caravans trade with neutral towns; deaths pay tithes | Logistics and profiting from *everyone's* wars |

Each one changes what "raiding" and "defending" mean, so a 4-player free-for-all has four genuinely different threats on the map.

### 3. Pick up in five minutes, still learning in fifty hours
The iPad-game test: someone who has played a few games should be doing something meaningful within **five minutes** and be fluent within **fifteen**. Depth comes from interactions between simple, visible rules, not from hidden stats or menus. Concrete rules we hold ourselves to:

- Two resources, one supply number, one faction meter. Nothing else to track on the HUD.
- At most 12 buttons on any command card, always in the same grid.
- Every unit has one *signature* thing it does, readable from its silhouette.
- Every number that matters is visible on a tooltip. No hidden modifiers.
- Defaults are smart: new workers go to work, regiments hold formation, ranged units skirmish by themselves.

### 4. A living, edible world
The map is not a static board. Grub seeps run dry and fall to "dregs", the Titan's corpse shudders and splits open new seeps mid-match, neutral hamlets will trade with you or remember that you burned them, and corpses on the battlefield are a resource for two factions. The world pushes players into each other.

### 5. Absurdly lightweight, without looking cheap
The engine is built from scratch around one hard question at every layer: *what is the least memory and CPU this can possibly cost?* The target is a full 4-player, 60-minute match with thousands of soldiers on screen at **≤ 160 MB of memory** (hard ceiling 256 MB), 60 fps on a base MacBook Air, and replay files measured in kilobytes. See [12 · Performance](12-performance.md). The art direction (stylized, low-poly, painted lighting) is chosen so that lean and beautiful are the same decision, not a trade-off.

## Who it's for

Mario and friends: 2 to 4 players on the same Wi-Fi network (with internet play via a virtual LAN as a later option), on a mix of Macs and Windows PCs. Non-commercial, personal use, built to production quality. There is no store, no monetization, no live service, which frees us to make choices purely for fun and craft.

## The experience we're aiming for

**Minute 0.** You pick the Brood. Your hive pulses, four Gnawer grubs wriggle out and burrow into the nearest seep on their own. Glowing veins crawl back to the hive. You queue a Skitterling swarm.

**Minute 8.** Your scout moths spot a Choir triangle shimmering across the river. Three pylons, a huge enclosed area, a juicy income bonus. You send Burrowers under the river to pop up beside one pylon. The triangle flickers and dies. Somewhere, a friend swears.

**Minute 22.** Two armies meet on a ridge. Ember Clan spearmen lock shields across the pass; their samurai cavalry swings wide. Your Chitin Guard hold the front while Spitters arc acid over them. The Clan cavalry hits your Spitters in the rear, their morale breaks, they rout. You evolve two Skitterling packs into Burrowers *mid-fight* and drop them behind the spear line. The line wavers. It breaks.

**Minute 41.** The Titan shudders. A new seep tears open in the middle of the map. Everyone rushes it. The Concord player, who has been quietly collecting tithes from every death on the map, raises a hundred dead soldiers from the ridge battlefield and marches them in.

**Minute 55.** Someone holds the Titan's Heart for three minutes. Game over. The replay file is 90 KB. "Rematch the ridge battle?" One click and the same two armies are staged on the same ridge in Field Battle mode.

## What we are deliberately *not* doing

- **No campaign map / turn-based layer.** The Total War inspiration is the battlefield, not the strategic map.
- **No Warhammer-level complexity.** No ammunition counters, no fatigue meters beyond a short post-charge "winded", no lord skill trees with 40 nodes, no per-unit equipment.
- **No dedicated servers, accounts, matchmaking or anti-cheat.** You play with friends; the host's machine runs the lobby.
- **No naval combat in the first release.** Water is terrain at first; naval gameplay is a planned expansion ([13 · Roadmap](13-roadmap.md)).
- **No bots in the first release**, though the architecture keeps the door open (the simulation is headless and deterministic, which is exactly what a bot needs).

## Success criteria

1. Four friends finish a 45-minute Conquest match on Wi-Fi, two on Mac and two on Windows, with zero desyncs and no one waiting on a loading screen longer than 3 seconds.
2. A friend who has never seen the game wins a skirmish fight against another newcomer after a 10-minute tutorial.
3. Peak memory on the largest map stays under 160 MB, measured automatically on every change.
4. People ask to play it again next week.
