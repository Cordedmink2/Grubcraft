# Grubcraft

A four-player LAN real-time strategy game where Warcraft III's base-building meets Shogun: Total War's regiment battles, fought over the corpse of a fallen god. Built on a lean custom engine for Mac and Windows, with a 4 GB memory budget for a full 4-player war.

- **Conquest** (main mode): gather Grub and Marrow, build a base, tech up, and command regiments with formations, flanks and morale. 2 to 4 players, 20 to 60 minutes.
- **Field Battle**: pre-built armies, one battlefield, 5 to 20 minutes.
- **Four factions** with four different economies: the Ember Clans, the Brood, the Choir and the Carrion Concord.

## Status

M0 (foundations) is done apart from a test run on a Windows PC. All eight M1 steps (regiments on a field) are built: formations, soldiers on screen, controls, melee and arrows, morale, pathfinding, the sandbox, real art and the Commander View, plus the menus and lobby screens. Golden replays (recorded battles that every change must still play exactly the same, on Mac, Windows and Linux) and the 6,000-soldier big-battle benchmark are in; what's left for the M1 exit is a 3,000-soldier, 60 fps check on real machines. M2 (LAN battle) has started: up to four machines on one network can play a Field Battle together (see Playing with friends below). Soldiers in the front rank pair off and duel, and blocks move like bodies, easing into step and swaying when they stand. The first battle AI is in: you can fight computer opponents at three levels in a Field Battle from the lobby, or hand any sandbox side to the computer. It reads the ground, anchoring its line on cliffs and waiting behind rivers, and `L` shows what it is thinking. The ground matters in a fight: regiments fighting downhill hit harder, archers on high ground shoot further, and regiments wading a ford are slow, can't charge and take heavier blows. In fords and passes, friendly regiments queue behind each other instead of piling into one lump. See the [roadmap](docs/13-roadmap.md). Start with the [design documents](docs/README.md).

## Building and running

You need [Rust](https://rustup.rs) (stable). Then, from this folder:

```sh
cargo run --release -p gc_game          # play the current build
cargo test --workspace                  # all tests, including determinism
cargo run --release -p gc_bench         # simulation benchmark with budget checks
cargo run -p gc_bake                    # check data/*.ron and bake target/grubcraft.pak
cargo run --release -p gc_game --features profile   # build with Tracy profiling
./scripts/bundle-macos.sh               # macOS: build Grubcraft.app (universal) and zip it
```

**Menus:** the game opens on the main menu. **Sandbox** starts the battle below. **Play** lets you host a lobby (map, mode, tempo, up to four players; add AI players and pick each one's level, Recruit, Veteran or General; pick faction, colour and team, then Start) or join one: games hosted on your network show up in the Join list, or type the host's IP address (port 47820). Up to four machines can play together, with AI players filling any slot; the host's machine runs the AI. Start opens a **Field Battle**: every side gets ten regiments from its faction, set out on its own start plateau (you and your allies at the south, facing north), and the computer players command theirs. The first side left holding the field for ten seconds wins (the panel says VICTORY or DEFEAT). The four factions are balanced for Field Battles: any pairing is close to an even fight. **Settings** holds your name, colour and faction, UI scale and the audio, graphics and accessibility choices (saved to a `settings.ron` file in your user config folder; only name, colour, faction and UI scale do anything yet).

**Playing with friends (LAN):** everyone runs the same build (the lobby shows each player's build and won't start until they match). One player chooses Play, then Host. The others choose Play and pick the game from "Games on your network", or type the host's IP address and press Join. On a Mac, allow Grubcraft to find devices on your local network and to accept incoming connections when macOS asks. Each joiner ticks Ready, the host adds AI players if wanted and presses Start. If someone's connection drops during the battle, the game pauses for up to 90 seconds while they come back (the host can press P to carry on without them); set **On drop** to Play on in the lobby to keep playing instead. The lag between giving an order and seeing it adjusts itself to the slowest connection. If the machines ever disagree about the battle, each one saves a desync report in a `desync-reports` folder next to its settings file; send those to the developers. If a game doesn't show up in the list, type the join code the host's lobby shows (like `K7QD-M2XA`) or the host's address; that usually still works (some Wi-Fi networks block the broadcast that finds games). The host can set a password on the Play screen; friends type it in the Password box. Everything sent between the machines is encrypted. From the terminal, `--host` opens a lobby straight away and `--join ADDRESS` (or a join code) joins one; add `--password WORD` for a locked lobby.

**Controls:** you are red. Left-click a red regiment to select it, or left-drag a box around several (Shift adds or removes). Right-click the ground to march the selection there side by side, facing the way they travel; right-drag to draw the line they should form instead (its length sets how wide they stand, and they face away from you). A big selection forms several lines. Right-click an enemy regiment to attack it. Your back two rows are Yumi archers, who shoot at enemies in range on their own; blocks that touch fight, and flank, rear and charging hits hurt more. Regiments that lose heart waver (a blinking amber banner), then rout (a white banner): they break ranks and run, ignoring orders, and rally once the enemy is gone; a regiment that routs twice is shattered (a grey banner) for good. Regiments find their own way around cliffs and deep water: a selected regiment shows its route as a dotted line, and **G** shows the ground no regiment can cross. Ctrl (Cmd on a Mac) + a number key saves the selection as a control group; the number recalls it (Shift adds it), and pressing it twice centres the camera on it. Pan with the arrow keys, screen edges or a two-finger trackpad scroll; zoom with the mouse wheel or a pinch; rotate with Alt + middle-drag or a two-finger twist; Home or a middle-click resets the rotation. **`** (backquote) or **F11** shows the dev overlay with frame, tick and memory stats (and prints memory to the terminal); **Esc** opens the menu (Resume, Play, New sandbox, Settings, Exit). Start with `--overlay` to show it from the first frame, `--no-menu` to skip the main menu, or `--battle recruit|veteran|general` to go straight into a Field Battle against one computer player of that level.

**Computer players:** **I** hands the side you command to the computer (press again to step it up from Recruit to Veteran to General, and once more to take the side back). The panel shows each computer player's personality, level, battle plan and what it expects ("Hold the line, advancing, expects a close fight"), and the latest thing it decided for the side you command. With Tab and I you can set two computer players against each other and watch.

To measure memory on your machine: `cargo run --release -p gc_game -- --frames 600` runs for 600 frames and prints the numbers. Measurement flags: `--latency 1|2|3` (frames in flight), `--render-scale 0.25..1` (render fewer pixels than the window has) and `--draw all|terrain|blocks|none` (turn parts of the frame off).

## Layout

| Path | What |
|---|---|
| `crates/gc_fixed` | Deterministic fixed-point maths (no floats) |
| `crates/gc_assets` | Asset pack format: writer and zero-copy reader |
| `crates/gc_data` | Game data tables (units, factions) as the game reads them |
| `crates/gc_sim` | The deterministic simulation; runs identically on every machine |
| `crates/gc_audio` | Sound: ADPCM bank, 32-voice mixer, lock-free command queue, `cpal` output |
| `crates/gc_platform` | Memory accounting and OS process stats |
| `crates/gc_render` | wgpu renderer (Metal, DirectX 12, Vulkan) |
| `crates/gc_ui` | Menus, lobby and settings screens (immediate-mode UI, no GPU code) |
| `crates/gc_net` | LAN play: UDP transport, finding games on the network, the lobby protocol and host-sequenced lockstep turns |
| `crates/gc_ai` | Computer players: battle tactics, difficulty levels and personalities; they read the sim and send ordinary commands |
| `crates/gc_game` | The game application |
| `tools/gc_bake` | Baker: compiles `data/*.ron` (and later art and audio) into the asset pack |
| `tools/gc_sfx` | Sound generator: synthesises every effect and music loop into `assets/audio/sounds.gcsnd` |
| `tools/gc_bench` | Headless benchmarks with budget gates |
| `assets/audio/` | The sound bank and its catalogue |
| `data/` | Game data in RON: edit these to change units, and `data/ai/` the computer players' personalities (rebuilt automatically) |
| `docs/` | Design documents |
