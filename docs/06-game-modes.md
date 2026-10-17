# 06 · Game Modes

Two modes at launch, and a third, the **Campaign**, planned to follow. All of them run on the **same simulation, unit data and controls**, so everything built for one improves the others.

| | **Conquest** (primary) | **Field Battle** (secondary) |
|---|---|---|
| What it is | Warcraft-style base building, economy, tech, army, war | Total War-style pitched battle with pre-built armies |
| Players | 2 to 6: 1v1, 2v2, 3v3, 1v1v1, FFA up to 6, 2v1 | 2 to 6: 1v1, 2v2, 3v3, FFA up to 6 |
| Length | 20 to 60 min (Tempo setting) | 5 to 20 min |
| Economy | Full | None (points budget) |
| Fog of war | Yes | Optional (default: light fog, enemies visible once spotted) |

## Conquest

*Built (2026-10-16, Conquest step 4a): Conquest starts from the lobby, on Ribcage Pass (2 players) or The Four Teeth (4 players). The lobby offers only those two maps for Conquest and won't start with more players than the map holds. A Tempo setting sets how fast gathering and building go: Standard (gather 100%, build and train times 100%), Swift (gather 150%, times 75%) and Epic (gather 80%, times 120%). At the start every side gets its faction's starting buildings (its main building on its map start), its starting units (workers, and for the Concord also a Caravan) and its starting stores (300 Grub, or 400 for the Concord). Since 2026-10-17 every AI slot is a computer player that plays the whole game ([18 · AI](18-ai.md#built-so-far)). Since step 4b the top bar shows your Grub, Marrow, supply and the clock, you can select your buildings to see their training queues, and `F1` finds idle workers ([08 · UI and UX](08-ui-ux.md#screen-layout-1080p-reference)); since step 4c workers lay buildings from a build menu, buildings train regiments, upgrade and rally from the command grid, and a right-click sends workers to a seep, a vein or a building to build. The `--conquest ribcage_pass|four_teeth` command-line flag starts Conquest straight away, with every other start held by an idle side.*

*Built (2026-10-17, Conquest step 6b): six-player Conquest on big maps. After the two shipped maps the lobby's Conquest list offers **Land: Random**, **Land: Plains**, **Land: Mixed** and **Land: Wild**: six-player Wilds Conquest maps 768 tiles a side, made from the map seed ([07 · Big Conquest maps](07-world-and-maps.md#field-battle-maps-and-recipes)), so **New map** and the map code work as they do for Field Battles (the shipped maps ignore the seed). Land: Random takes its land from the seed. With fewer than six players the starts are spread round the map: two players start across the map from each other, three in a triangle (every other start) and four in the corners. A soak test plays an hour of six scripted sides on one of these maps and holds it to the 6 GB budget ([12 · Performance](12-performance.md)).*

### Flow of a match
1. **Lobby:** map, teams, factions, colours, Tempo, victory conditions, Titan Events on/off.
2. **Opening (0 to 5 min):** workers out, first structures, scout, first regiments. Creep a camp with your hero.
3. **Mid game (5 to 20 min):** tier 2, expand to the natural, contest Marrow and Shrines, trade or raid hamlets, first big clash. Titan's first Tremor opens a Bloodseep.
4. **Late game (20 min +):** tier 3 monsters and ultimates, upkeep pressure, the Titan's Heart becomes the decisive objective.

### Victory conditions (lobby options, both on by default)
- **Annihilation:** destroy all of an opponent's main buildings (Hall, Hive, Spire, Counting House at any tier). A player with no main building gets 90 s to build one before elimination, shown to everyone. No "hunt the last worker in the corner" endings.
- **Titan's Heart:** the map centre holds the Heart relic. Hold it uncontested with any unit for **3 continuous minutes** to win. It unlocks at minute 20 (Standard tempo), so early rushes can't take it, and it guarantees that turtle games end.

*Built (2026-10-16, Conquest step 5a): the lobby's Victory setting picks Both (the default), Annihilation or Titan's Heart, and a Conquest match is won only that way, never by holding the field. **Annihilation:** a player whose last main building falls (any tier) has 90 seconds to lay a new one, and everyone sees the countdown; then they are out, and their regiments and buildings leave the field. A new main building ends the countdown once it is finished; while it is going up the countdown waits, and calling it off takes the countdown up where it stopped, not from the top. When everyone left is on one team, that team wins (a player who never had a main building still counts while they have an army); if the last players are put out together, nobody wins. **Titan's Heart:** a red stone on the map's centre (on the field and the minimap) that glows once it unlocks: minute 20 at Standard, 15 at Swift, 24 at Epic. A side with any unbroken regiment within 6 tiles and no enemy there holds it, the stone and its minimap mark take the holder's colour, and a line shows the hold ("1:12 of 3:00"); an enemy arriving or everyone leaving starts it over. The simulation makes both rulings (`gc_sim::verdict`), so every machine and the replays agree.*

*Built (2026-10-17, Conquest step 5b): **fog of war.** A Conquest match played for a victory is played in fog. Each regiment sees 32 tiles round it (an archer's reach and a little more) and each building as far as its role allows (main building 32, outpost 28, tower 36, wall 8, the rest 20), edge to edge, and allies share what they see. An enemy regiment out of every friendly regiment's and building's sight isn't drawn and can't be targeted; woods and tall grass still hide regiments within sight, as in a Field Battle, and buildings spot into them as regiments do. Coming into sight is no surprise: only cover springs an ambush. An enemy building appears, on the field and the minimap, once your side has seen it, and stays known after that; your regiments can only be sent to attack a building your side has seen. The ground you can't see now is drawn darker, and ground you have never seen darker still. Not yet: remembering a building as it was last seen (for now it shows as it stands), ridges and slopes that block sight, scouts that see further, and a fog setting for Field Battles.*

Optional:
- **Regicide:** each player's first hero is their "lord". Lose your lord permanently (no revive) and you lose. Short, brutal, very Shogun.
- **Score limit:** first to N points from kills, structures and Shrines held. For a fixed 30-minute evening game.

### Teams
- Allies share vision and can trade resources (with a 10% tax), and the Concord can run caravans to allied main buildings.
- Allied victory: a team wins together.
- Any slot can be an **AI player**, as an opponent or a teammate, with a difficulty and a personality. AI allies answer pings and ping back. See [18 · AI](18-ai.md).

### Surrender and leaving
- Surrender is a vote in team games, instant in FFA. A surrendered player's structures crumble over 30 s (the Concord can still collect tithes from the collapse, of course).
- A disconnected player's units hold position and defend themselves; their ally can take shared control, or (a later lobby option) an AI runs their army until they rejoin. See [11 · Networking](11-networking.md#disconnects-and-rejoining) for rejoining.

## Field Battle

The Total War experience in a 10-minute bite: two or more armies, one battlefield, no economy.

### Flow
1. **Lobby:** map, teams, factions, **points budget** (Skirmish 2,000 · Battle 4,000 · Grand Battle 8,000), time limit, weather/time-of-day preset, and **Champions before AI battles** (on by default).
2. **Army builder (2 min):** spend points on regiments, one to three heroes (hero levels cost points), and upgrades. Saved army lists can be loaded with one click; the builder shows faction presets for newcomers ("Balanced", "Aggressive", "Defensive").
3. **Deployment (90 s):** place regiments inside your deployment zone. You can see the terrain, not the enemy (unless a Field Battle option reveals deployments). Drag to place, drag to set frontage, exactly like battle orders.
4. **Battle:** fight until victory or time.

### Victory
- **Rout:** an army whose regiments are all routed, shattered or dead loses.
- **Victory points:** some maps have 1 to 3 capture points; holding the majority when time expires wins.
- **Time limit:** default 20 min, then victory points, then remaining point value decides.

### Field Battle extras
- **Pause** (team-voted in multiplayer, free in solo sandbox). Giving orders while paused is allowed only in solo.
- **Slow-motion** in solo sandbox and replays, not in multiplayer.
- **Sandbox mode** (solo): place any armies on any map for both sides and watch or play. This is the main balance-testing and learning tool, and the first thing we build. Once the battle AI exists, the other side can be handed to it.
  - **Built so far (M1 step 7):** `grubcraft --sandbox` starts on an empty field. You can place regiments of any type for any of the four sides (`B`, then click; `[` and `]` choose the type), command any side (`Tab` switches), remove regiments (`Delete`) or clear the field (`Shift` + `Delete`), try buildings (`K`, then click to put the chosen building down with its footprint centred on the cursor, for the side you command; `[` and `]` then choose the building instead of the regiment; `Delete` over a building takes it away; select regiments and right-click an enemy building to send them to knock it down; the sandbox is the only place buildings exist so far, and each is a plain box in its side's colour that dims as it is damaged, with a bar over it, until real models arrive), pause (`P`), run from 0.1× to 4× speed (`-` and `=`), tune a unit type's stats live (`T`), and reload `data/units.ron` from disk (`Shift`+`R`, in a build made with `--features dev`, run from the repository). Without `--sandbox` the same tools work over the four-army demo. The main menu's Sandbox entry opens the empty field. `I` hands the side you command to the battle AI (Recruit, then Veteran, then General, then Warlord, then back to you), so you can fight it or watch two AIs fight. `L` shows the AI lens: each computer player's plan drawn on the field and its latest decisions in the panel.
- **Army builder and points (M2 step 5a, 2026-10-15):** in a Field Battle lobby the host sets the **Points** budget (2,000, 4,000 or 8,000; 4,000 by default) where the Tempo stepper is for other modes. Instead of a timed two-minute phase, each player builds their army in the lobby itself: **Army** opens a screen listing their faction's regiments with what each costs and − and + buttons, the points spent out of the budget, **Pick for me** and **Done**. Every regiment costs 400 points today (the `points` field in `data/units.ron`), so a 4,000-point army is ten regiments, as in the balance pass. An empty army means "picked for you": the faction's preset mix, as many regiments as the budget pays for, which is also how AI players build. Changing faction clears the army. The host raises every army and holds each to the budget and to the player's faction, whatever a joiner sends. Not yet: heroes, upgrades, saved army lists, and points that differ by unit.
- **Champions before battle:** in a battle with a computer player, champions may duel between the lines before the armies close, and the winner's side takes heart ([05 · Combat](05-combat.md#champions-before-battle)). A lobby option, on by default. **Built 2026-10-15.**
- **Deployment phase (M2 step 5b, 2026-10-15):** each player's army starts in the standard deployment inside a **deployment zone**, a circle round their start drawn as a dashed amber ring on the ground (big enough for the standard deployment with ten tiles to spare, at least 24 tiles across the radius). The zone is also capped at just under half the distance to the nearest other start, so the six zones of a six-player map don't overlap (31 tiles in 3v3 on the 256-tile map); 1v1 and 2v2 zones are unchanged. A zone always holds its army's standard deployment, so six Grand Battle (8,000-point) armies are wider than that room and neighbouring zones may touch on the Small 320-tile map; the Standard six-player map (768 tiles, 2026-10-17) leaves them room. While the armies deploy, time stands still: nothing marches, fights or loses heart, enemy regiments are hidden, and an order sends a regiment straight to its new place (right-drag sets the line and its facing as in battle; a line drawn past the zone's edge slides inside it whole, and a spot off the map or on ground no one can stand on is refused). Formations can be changed; attack orders wait. The command-grid corner of the bottom bar shows DEPLOYMENT, the time left and a **Ready** button. The battle starts when every person has pressed Ready, or after 90 seconds. Computer players keep the standard deployment and are ready at once, and a battle of computers alone has no deployment phase. The rules live in the simulation (`gc_sim::deploy`: a `DeployZone` command per player, move orders that place regiments while deploying, and `Ready`), so every machine agrees when the battle starts, and a move order sent in the last moment before the battle simply marches. Attack orders, removing regiments and tuning units wait for the battle; the battle clock (and the time in VICTORY or DEFEAT) starts when it does. Not yet: dragging a regiment to place it, a lobby option to reveal deployments, and the computer choosing its deployment from the ground.
- **Map choice and map code (random maps step 1, 2026-10-16):** the host picks a Field Battle map from ten: the Proving Ground, Random: Pass, Ford, Hill or Open (one from each recipe), Random: Any, or the big Wilds maps, Land: Random (the default, as wild as the seed picks), Plains, Mixed or Wild (2026-10-17). **New map** draws a new map seed. The map code (`XXXX-XXXX-XXXX-XXXX`) is that seed as players share it, and a code typed in sets it. Everyone sees the code and a small picture of the map with its six starts marked (on the Wilds, also its villages, outposts, lairs and standing stones, which the minimap marks too). The recipes are in [07 · Field Battle maps and recipes](07-world-and-maps.md#field-battle-maps-and-recipes). **Map size (2026-10-17):** the host also picks Small, Standard (the default) or Large in the lobby's header; with the player count it sets the map's side, from 256 tiles (Small, two players, about a minute and a half's march across) to 896 (Large, six players). A Standard map is 512, 640 or 768 tiles, three to four minutes across; see [07 · Map size](07-world-and-maps.md#field-battle-maps-and-recipes).
- **Versus AI:** any Field Battle slot can be an AI player that picks its army from its personality's presets, deploys and fights ([18 · AI](18-ai.md#captain-battle-tactics)).
  - **Built so far (AI-1 step 1):** a lobby match is a Field Battle. Every side fields the army its player built (see **Army builder and points** above) or, left empty, regiments from its faction's roster in a preset mix up to the points budget (the player "Balanced", each AI its personality's preset), set out in the standard deployment (line in front, archers behind, fast troops on the wings) on one of the start plateaus of the lobby's map (the four corners for up to four players, plus the middles of the north and south for five or six): this machine's player and its allies take the south, facing north as the camera does, and the others the north. With five or six players in three teams with pairs among them (2v2v2, 2v2v1), each team takes a column instead: this machine's team the west (SW, then NW), the next team the east, the last the middle. Since M2 step 5b (2026-10-15), a battle with at least one person in it opens with the **deployment phase** below. There is no time limit yet. **Victory:** the first side (with its allies) left as the only one with regiments still fighting, held for ten seconds so a rout that rallies doesn't end it early, wins (the simulation's own ruling, `World::verdict`, so every machine agrees; regiments hidden in cover don't hold the field, see [20 · Ambushes](20-ambushes.md)); the panel shows VICTORY or DEFEAT and the battle carries on until you press Esc. `grubcraft --battle recruit|veteran|general|warlord` starts a one-on-one against an AI of that level straight away. **Balance today:** after the Field Battle balance pass every faction pairing is close to even: two Veteran AIs win between 42% and 59% of the time whichever factions they play (table and method in [17 · Unit rosters](17-unit-rosters.md#field-battle-balance)).

### Restage this battle (the bridge between modes)
After a Conquest match, the post-game screen lists its biggest battles (by models killed). Pick one and the exact armies that fought it, with their veterancy and heroes, are staged on a Field Battle map generated from the same patch of terrain. Rematch the fight that decided the game. Because both modes share one simulation and replays record everything, this is cheap to build: it is a snapshot of regiments at the moment the battle began, loaded into a battle map.

## Campaign (planned)

A world map of the whole Reach, played over several game nights by 1 to 6 players (and AI players). Each season everyone plans at once: march your hosts, send your Pathfinder into the fog, raise regiments, deal with the peoples you meet. When hostile armies meet, the battle is fought as a Field Battle with the armies exactly as they are, on a map made from that region, or handed to the battle AI on both sides, which plays it out in the real simulation in seconds. One relations ledger runs the whole campaign, so the Reach remembers what you did on the first night. Full design in [19 · Campaign and Exploration](19-campaign.md); it is built as its own track starting after M4.

## Practice and learning

- **Tutorial: "First Harvest"** (≈ 10 min, solo). Teaches camera, selection, workers and building, one fight with formations and a flank charge, a hero ability, and ends by destroying a training camp. Uses the Ember Clans.
- **Faction primers** (≈ 3 min each): one short scenario per faction that teaches its signature mechanic (Brood: cut and regrow a vein, evolve mid-fight; Choir: build a chord and refract a beam; Concord: run and defend a caravan, raise the dead).
- **Sandbox** (above).
- **Replays** of every match, with free camera, speed control and a timeline of battles.

## Later modes (not in first release)

AI players are no longer a later mode: they arrive in stages with Field Battle (M2) and Conquest (M3), as [18 · AI](18-ai.md#build-plan) describes.

| Mode | Idea | Needs |
|---|---|---|
| **Co-op Siege** | Up to 6 players defend a hamlet against Titan-spawn waves | Wave scripting |
| **Naval maps** | Water lanes, ships, coastal raids | Naval units, water pathing |
| **King of the Corpse** | Short 4-player FFA where the Heart moves | A variant map script |
