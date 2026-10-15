# 08 · UI and UX

The UI goal: **comprehensive, never burdensome**. Think of the best iPad strategy games: you understand what you're looking at in seconds, and the depth is in the decisions, not the interface.

## UI principles

1. **The world is the UI.** Economy, threats and objectives are shown on the map (seep pips, glowing veins, chord triangles, caravan routes, hamlet banners), not in panels.
2. **Glance, don't read.** Every important state has a shape and colour you can recognise in peripheral vision. Text is for tooltips.
3. **One grid, everywhere.** The command card is always a 4 × 3 grid with the same hotkeys (`Q W E R / A S D F / Z X C V`). Common orders always sit in the same slots across every unit and faction.
4. **No modal screens during play.** No pop-ups that pause you, no nested menus. Research and production are queued from the command card.
5. **Smart defaults, optional control.** Workers go to work, groups move in formation, ranged units skirmish. You can override everything, but you never *have* to.
6. **Numbers on demand.** Hover (or hold Alt) on anything to see exact numbers. Nothing important is hidden.

## Screen layout (1080p reference)

```
┌────────────────────────────────────────────────────────────────────────┐
│ Grub 1,240  Marrow 185  Supply 64/90  Honor ███░ 70        ⏱ 14:32  ☰ │  ← top bar (thin)
│                                                                        │
│  ▲ under attack (edge arrow)                                           │
│                                                                        │
│                          THE BATTLEFIELD                               │
│                                                                        │
│                                                     ◆ idle workers (3) │
│                                                                        │
├──────────┬───────────────────────────────────────────────┬─────────────┤
│          │ [card][card][card][card][card][card][card]... │ Q  W  E  R  │
│ MINIMAP  │   ARMY TRAY: regiment cards (health, morale,  │ A  S  D  F  │
│          │   veterancy, ability ready dots)              │ Z  X  C  V  │
└──────────┴───────────────────────────────────────────────┴─────────────┘
```

- **Top bar:** the four economic numbers, match clock, menu. Nothing else.
- **Minimap (bottom-left):** terrain, fog, units as dots, seeps as glowing dots, pings. Click to jump, right-click to order.
- **Army tray (bottom-centre):** the Total War unit-card row. Each selected regiment (or, with nothing selected, your current control group) has a card showing a portrait, model count bar, morale bar, veterancy chevrons and ability-ready dots. Clicking a card selects that regiment; double-click centres the camera on it. When a structure is selected, the tray shows its production queue and research instead.
- **Command card (bottom-right):** 12 slots. Common layout:

| | | | |
|---|---|---|---|
| `Q` Move | `W` Attack | `E` Hold | `R` Formation |
| `A` Ability 1 | `S` Ability 2 | `D` Ability 3 | `F` Ultimate / special |
| `Z` Skirmish / Brace | `X` Charge | `C` Run | `V` Retreat |

Structures use the same grid for production. Workers' build menu is one sub-page (opened with `B`), the only nested menu in the game.

- **Built so far (Conquest UI, piece 1):** the bottom bar is drawn in battles and the sandbox (not behind the menus). The minimap shows the ground (water, woods, grass, rising hide), a dot per regiment your side can see in its team colour, your selection bigger with a white rim, and the camera's view as a white outline; click or drag on it to move the camera, right-click to send the selection there. The army tray shows a card for each selected regiment (or, with nothing selected, the control group you last recalled): a team-coloured portrait with the unit's initial until there is art, a red rim when it is fighting, the name, a model-count bar, a morale bar (blue steady, yellow wavering, red routing) and the model count. Click a card to select that regiment alone (Shift toggles it); double-click centres the camera on it. Too many cards shrink, then show a "+N" count. Clicks on the bar never reach the battlefield. The bar fills the bottom edge, so scrolling by pushing the mouse against the bottom of the screen is off during a match (the arrow keys still scroll). Not yet: veterancy chevrons and ability dots on cards (no unit has them yet), the command grid, edge notifications, idle button, the top bar and structure queues (later pieces and Conquest itself).
- **Edge notifications:** small arrows at the screen edge point to off-screen events (under attack, research done, Tremor beacon). Press `Space` to jump to the latest one.
- **Idle worker button:** right side, with a count. `F1` selects the next idle worker.

## Camera

- **Bird's-eye view** at a fixed pitch (about 55°), with rotation limited to ±45° (reset with middle-click). Fixed pitch keeps the battlefield readable and the renderer cheap.
- **Zoom** from close (individual soldiers fighting) to far. Past a threshold the view smoothly switches to **Commander View**: soldiers disappear and each regiment is drawn as a banner block with its facing and state, like Total War's tactical map. It is easier to command a big battle in, and much cheaper to render.
- **Built so far (M1 step 8):** zooming out past a camera distance of 175 switches to Commander View (and back in below 160, so it doesn't flicker at the threshold). Each regiment becomes a block the size of its formation in its team colour, with a pale strip along its front and a flagpole whose flag grows with the zoom so it stays readable. `C` opens the credits screen (the authors and licences of the art in the game, read from `CREDITS.md`); `C` or `Esc` closes it.
- Edge scroll, `WASD`-free arrow scroll (WASD is not used for camera, because the grid uses those keys), middle-drag pan.
- **Mac trackpad:** two-finger pan, pinch zoom, two-finger rotate. Many friends will play on MacBooks without a mouse; this must feel native.

## Selection and control

- Left-click / drag-box to select. Double-click selects all of that type on screen.
- **Control groups** `1` to `0` (`Ctrl+number` to set); double-tap to centre.
- **Group formation:** when several regiments are ordered to move with a right-drag, they arrange themselves along the dragged line in a sensible order (melee front, ranged behind, cavalry on the flanks). This is the single most important "feels like a commander" feature.
- `Tab` cycles sub-groups within a mixed selection (for abilities).
- **Smart cast:** abilities fire on key-press at the cursor (option for click-to-confirm).

**Built so far (M1 step 6):** click and drag-box select (Shift adds or removes), right-click to move, right-drag to set a line's position, width and facing, right-click an enemy to attack, a dotted line along each selected regiment's route with a plate where it will stop, `G` to show the ground no regiment can cross (a dev view for now), `L` for the AI lens (what each computer player is planning, see [18 · AI](18-ai.md#testing-tuning-and-explaining)), and control groups (`Ctrl`/`Cmd` + number to set, the number to recall, `Shift` + number to add, double-tap to centre). `F` cycles the selection's formation (Line, Tight, Loose, and Shield wall when any selected regiment has shields; it moves to `R` with the rest of the order grid once the sandbox's reload key moves, see [05](05-combat.md#weapons-and-formations)). Not yet: double-click to select a type, arranging melee in front and ranged behind on a group order (today regiments keep their left-to-right order, and a big group forms several lines with whoever is furthest forward in front), `Tab` sub-groups, and the order hotkeys.

## Readability on the battlefield

- **Regiment banner** above every regiment: faction colour, unit icon, small model-count bar, morale shown by banner state (upright, tilting when wavering, flat and white-flagged when routing).
- **Team colour** is on banners, cloth and a ground ring when selected. Colour-blind palettes available.
- **Silhouettes first:** every unit type is identifiable by outline at the furthest zoom before Commander View.
- **Ground decals** show formation previews while dragging, attack ranges on hover, chord triangles, vein networks, ability areas.
- **Combat text is off by default.** Damage numbers clutter battles. You read the battle through models falling and banners tilting.
- **Woods and tall grass hide regiments** ([20 · Ambushes](20-ambushes.md#reading-it-on-screen)). Enemies your side can't see aren't drawn and can't be clicked; your own hidden regiments (and your allies') carry a moss-green flag above the treetops, so you always know which of yours the enemy can't see. When an ambush is sprung, by you or on you, an **Ambush!** message names who caught whom, with a war drum and a horn.
- **Built so far (ambushes, 2026-10-15):** trees and tall-grass tufts on the generated maps (the Commander View leaves the grass out), unseen enemies not drawn or picked, the moss-green flag (scaled up in the Commander View), the Ambush! message and sound, and `V` in the sandbox to show every regiment, hidden or not (sandbox only, never in a Field Battle or a network match, and starting one switches it off). Not yet: last-seen ghosts and the hidden tooltip ([20](20-ambushes.md#build-plan), step B).

## Onboarding

- **First launch:** name, colour, a 30-second "how to read the screen" overlay, then straight into the **First Harvest** tutorial (skippable).
- **Contextual hints** in the first 3 matches only: one line at a time, near the thing they describe ("Your Hands are idle: right-click a seep"). Off forever after, or turned off in settings.
- **Codex:** every unit, structure and mechanic with numbers, a 3-second looping clip, and "good against / weak against".
- **Faction picker** shows one sentence per faction plus a difficulty rating (Clans: easy; Brood: medium; Choir: hard; Concord: medium-hard).

## Menus and lobby

- Main menu: **Play** (Host / Join), **Learn** (tutorial, primers, sandbox), **Replays**, **Settings**, **Quit**.
- **Host:** choose mode and map, then you're in the lobby. Friends on the same network see the lobby in their Join list automatically; others join by IP or join code.
- **Lobby:** player rows (name, faction, colour, team, ready). Host sets map, mode, Tempo, what happens when a player drops (Wait or Play on), victory conditions. A version check shows clearly if someone's build doesn't match ("Alex is on 0.4.1, host is on 0.4.2").
- From double-clicking the app to being in a friend's lobby should take **under 15 seconds**.

**Built so far (menus and lobby):** the main menu has Play, Sandbox, Settings and Exit (Resume and New sandbox appear when you press Esc during a match). Play is one screen: **Host a game** (map, mode, tempo, Host) beside **Join a game** (list of games found on the network, an address or join code box, Join). Hosting opens the lobby: four player rows (colour swatch, name, faction, team, ready or AI), the host's Add AI and Remove buttons, Start, and a line saying why the match can't start yet (alone, someone not ready, a version mismatch spelled out as "Alex is on 0.4.1, host is on 0.4.2", Conquest not playable yet). Settings has name, colour, faction, graphics preset, UI scale (75% to 150%), V-sync, frame cap, five audio sliders and the accessibility toggles; they save when you go Back. Working today: name, colour, faction and UI scale (Larger text adds 25% to the scale). Saved but not acted on yet: graphics preset, V-sync, frame cap, audio, colour-blind palette, high-contrast banners, reduced shake, hints. Joining works over the local network since M2 step 1: Join's list shows games hosted on the network, or type the host's IP address ([11 · Networking](11-networking.md)). Since step 3, the Host card takes an optional password and the Join card a password box. The Join list marks locked games, and the host's lobby shows a join code with its address, which the Join box accepts. Since step 5a, a Field Battle lobby has a **Points** stepper for the host (others see "Points: 4,000"), the hint line says what your army is ("Army: picked for you" or "Army: 10 regiments, 4,000 of 4,000 points"), and an **Army** button opens the army builder: a row per regiment of your faction with its points and − count + buttons (+ greys out when the regiment would go over the budget), the total spent, an "Over budget" warning when the host lowers the budget under your army (the lobby line turns red too, and the regiments lowest on the list stay at home), **Pick for me** and **Done** (or Esc). The lobby's footer has the host's **Champions before AI battles** toggle (on by default; it only matters when a computer player takes part, and other players see it as text); in the battle, the panel announces the champions' challenge and its result for eight seconds ([05 · Combat](05-combat.md#champions-before-battle)). Not yet: Learn and Replays entries, key rebinding, the live memory readout, text chat. Enter presses the highlighted (orange) button and Esc goes back one screen.

## Communication

- **Ping wheel** (`G` + mouse direction): "Attack here", "Defend here", "Danger", "On my way", "Need help", "Good game". Pings show on the minimap and play a sound.
- Text chat (`Enter`), team or all.
- No in-game voice: friends already use Discord or FaceTime.
- **AI allies use the same channels:** they treat your pings as requests, ping their own plans ("Attacking here", "Danger"), and send short chat lines in their personality's voice ([18 · AI](18-ai.md#ai-allies)).

## Settings

- Graphics presets (Battery, Balanced, High) plus individual options; a live memory/frame readout in the settings screen.
- Resolution scale, UI scale (75% to 150%), frame cap, V-sync.
- Full key rebinding; alternate grid layouts (e.g. AZERTY).
- Audio sliders: master, music, effects, unit voices, UI.
- Accessibility: colour-blind team palettes, high-contrast banners, larger text, reduced screen shake, subtitles for unit barks.

## Pause, speed, and fairness

- Multiplayer pause: any player can pause up to 3 times per match; unpausing has a 3-second countdown.
- Game speed is fixed in multiplayer (lobby choice of Normal or Fast). Solo modes allow slow-motion and fast-forward.
- **Built so far (M1 step 7):** `P` pauses, `-` and `=` step through 0.1×, 0.25×, 0.5×, 1×, 2× and 4×. The sandbox panel in the top-left corner shows the speed, the side you command, the type you are placing and the keys; `T` opens a stats panel for that type (Up and Down pick a stat, Left and Right change it, `Shift` for ten steps) and `H` hides the panel. It is drawn with the dev overlay's text for now and moves to the `gc_ui` widgets once the menus land.

## Post-game

- Summary: winner, graphs (income, army value, supply over time), key battles list with "Watch" and "Restage this battle" buttons, MVP regiment (most kills), funniest stat ("Most Grub lost to raids").
- Save replay (automatic).

## AI players in the lobby

- The host can fill any slot with an AI player, as an opponent or a teammate. The row shows a **difficulty** (Recruit, Veteran, General, Warlord) and a **personality** (each faction's two, or Random, the default), both chosen with one click.
- Every slot, human or AI, has a **handicap** setting (income from 50% to 150%), shown on the row whenever it isn't 100%, so nobody is surprised.
- Details in [18 · AI](18-ai.md#difficulty).
- **Built so far (AI-1 step 1):** an AI row has a level button (Recruit, Veteran or General; click to step through them, Veteran to start) beside Remove; other players see the level as text. The personality is picked at random from the faction's two each match and named in the sandbox panel once the battle starts. Not yet: the Warlord level (AI-2), choosing the personality, handicap.
- **In the battle,** the sandbox panel lists each computer player's personality, level, battle plan, posture and expected result, and `I` hands the side you command to the computer (Recruit, then Veteran, then General, then back to you).

## Parley (planned, M4)

Dealing with hamlets, mercenary companies and, later, other peoples on the map uses one small panel, never a pop-up that pauses the game:

- Select one of their units or buildings and the panel opens beside the command card. It shows their **stance**, and **liking** and **trust** as one bar each, with the top reasons underneath ("You traded with us", "You burned Millbrook").
- At most four actions for that party (Trade, Hire, Rumours, Gift; later Request passage, Demand tribute), on the usual grid keys.
- Every action shows its likely answer *before* you commit ("Likely to accept: they like you, you're asking a lot"), because the answer is a visible rule, not a dice roll. See [18 · AI](18-ai.md#deals-and-contracts).
