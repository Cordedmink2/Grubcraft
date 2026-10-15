# 06 · Game Modes

Two modes at launch, and a third, the **Campaign**, planned to follow. All of them run on the **same simulation, unit data and controls**, so everything built for one improves the others.

| | **Conquest** (primary) | **Field Battle** (secondary) |
|---|---|---|
| What it is | Warcraft-style base building, economy, tech, army, war | Total War-style pitched battle with pre-built armies |
| Players | 2 to 4: 1v1, 2v2, 1v1v1, 4-player FFA, 2v1 | 2 to 4: 1v1, 2v2, 4-player FFA |
| Length | 20 to 60 min (Tempo setting) | 5 to 20 min |
| Economy | Full | None (points budget) |
| Fog of war | Yes | Optional (default: light fog, enemies visible once spotted) |

## Conquest

### Flow of a match
1. **Lobby:** map, teams, factions, colours, Tempo, victory conditions, Titan Events on/off.
2. **Opening (0 to 5 min):** workers out, first structures, scout, first regiments. Creep a camp with your hero.
3. **Mid game (5 to 20 min):** tier 2, expand to the natural, contest Marrow and Shrines, trade or raid hamlets, first big clash. Titan's first Tremor opens a Bloodseep.
4. **Late game (20 min +):** tier 3 monsters and ultimates, upkeep pressure, the Titan's Heart becomes the decisive objective.

### Victory conditions (lobby options, both on by default)
- **Annihilation:** destroy all of an opponent's main buildings (Hall, Hive, Spire, Counting House at any tier). A player with no main building gets 90 s to build one before elimination, shown to everyone. No "hunt the last worker in the corner" endings.
- **Titan's Heart:** the map centre holds the Heart relic. Hold it uncontested with any unit for **3 continuous minutes** to win. It unlocks at minute 20 (Standard tempo), so early rushes can't take it, and it guarantees that turtle games end.

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
  - **Built so far (M1 step 7):** `grubcraft --sandbox` starts on an empty field. You can place regiments of any type for any of the four sides (`B`, then click; `[` and `]` choose the type), command any side (`Tab` switches), remove regiments (`Delete`) or clear the field (`Shift` + `Delete`), pause (`P`), run from 0.1× to 4× speed (`-` and `=`), tune a unit type's stats live (`T`), and reload `data/units.ron` from disk (`R`, when run from the repository). Without `--sandbox` the same tools work over the four-army demo. The main menu's Sandbox entry opens the empty field. `I` hands the side you command to the battle AI (Recruit, then Veteran, then General, then back to you), so you can fight it or watch two AIs fight. `L` shows the AI lens: each computer player's plan drawn on the field and its latest decisions in the panel.
- **Champions before battle:** in a battle with a computer player, champions may duel between the lines before the armies close, and the winner's side takes heart ([05 · Combat](05-combat.md#champions-before-battle)). A lobby option, on by default. **Built 2026-10-15.**
- **Versus AI:** any Field Battle slot can be an AI player that picks its army from its personality's presets, deploys and fights ([18 · AI](18-ai.md#captain-battle-tactics)).
  - **Built so far (AI-1 step 1):** a lobby match is a Field Battle. Until the army builder and points budget arrive, every side fields ten regiments from its faction's roster in a preset mix (the player "Balanced", each AI its personality's preset), set out in the standard deployment (line in front, archers behind, fast troops on the wings) on one of the map's four start plateaus: this machine's player and its allies take the south, facing north as the camera does, and the others the north. There is no army builder or deployment phase yet, and no time limit. **Victory:** the first side (with its allies) left as the only one with regiments still fighting, held for ten seconds so a rout that rallies doesn't end it early, wins; the panel shows VICTORY or DEFEAT and the battle carries on until you press Esc. `grubcraft --battle recruit|veteran|general` starts a one-on-one against an AI of that level straight away. **Balance today:** after the Field Battle balance pass every faction pairing is close to even: two Veteran AIs win between 42% and 59% of the time whichever factions they play (table and method in [17 · Unit rosters](17-unit-rosters.md#field-battle-balance)).

### Restage this battle (the bridge between modes)
After a Conquest match, the post-game screen lists its biggest battles (by models killed). Pick one and the exact armies that fought it, with their veterancy and heroes, are staged on a Field Battle map generated from the same patch of terrain. Rematch the fight that decided the game. Because both modes share one simulation and replays record everything, this is cheap to build: it is a snapshot of regiments at the moment the battle began, loaded into a battle map.

## Campaign (planned)

A world map of the whole Reach, played over several game nights by 1 to 4 players (and AI players). Each season everyone plans at once: march your hosts, send your Pathfinder into the fog, raise regiments, deal with the peoples you meet. When hostile armies meet, the battle is fought as a Field Battle with the armies exactly as they are, on a map made from that region, or handed to the battle AI on both sides, which plays it out in the real simulation in seconds. One relations ledger runs the whole campaign, so the Reach remembers what you did on the first night. Full design in [19 · Campaign and Exploration](19-campaign.md); it is built as its own track starting after M4.

## Practice and learning

- **Tutorial: "First Harvest"** (≈ 10 min, solo). Teaches camera, selection, workers and building, one fight with formations and a flank charge, a hero ability, and ends by destroying a training camp. Uses the Ember Clans.
- **Faction primers** (≈ 3 min each): one short scenario per faction that teaches its signature mechanic (Brood: cut and regrow a vein, evolve mid-fight; Choir: build a chord and refract a beam; Concord: run and defend a caravan, raise the dead).
- **Sandbox** (above).
- **Replays** of every match, with free camera, speed control and a timeline of battles.

## Later modes (not in first release)

AI players are no longer a later mode: they arrive in stages with Field Battle (M2) and Conquest (M3), as [18 · AI](18-ai.md#build-plan) describes.

| Mode | Idea | Needs |
|---|---|---|
| **Co-op Siege** | Up to 4 players defend a hamlet against Titan-spawn waves | Wave scripting |
| **Naval maps** | Water lanes, ships, coastal raids | Naval units, water pathing |
| **King of the Corpse** | Short 4-player FFA where the Heart moves | A variant map script |
