# Golden replays

Recorded battles that the simulation must keep playing exactly as recorded. Each `.gcr` file holds a match's setup (seed, unit table, teams, map, and since format 3 its woods and tall grass) and every command of the battle, plus the simulation's state hash once a second. Playing one back must give the same hashes, on every machine. See [10 · Architecture](../docs/10-architecture.md#testing-strategy) and the `gc_replay` crate.

| Replay | What it is |
|---|---|
| `field-1v1.gcr` | Ember Clans (Veteran AI) against Brood (General AI), ten regiments each, on a generated 256-tile map |
| `field-2v2.gcr` | Ember Clans and Carrion Concord against Brood and Choir, all computer players, ten regiments each |
| `field-woods-1v1.gcr` | Brood against Choir (both General AI) on a generated map with woods and tall grass: regiments hide and are spotted, and six ambushes are sprung ([20 · Ambushes](../docs/20-ambushes.md)) |
| `sandbox-tuning.gcr` | Ember Clans against Carrion Concord, with a stat change at 1:00 and a regiment removed at 1:30 (the sandbox's commands) |
| `field-deploy-1v1.gcr` | Ember Clans against Carrion Concord, both Veteran AI, opening with the deployment phase people play: both sides get a zone, Ember places by hand (a regiment moved and turned inside the zone; a move outside it, an order to an enemy regiment, a shield wall without shields, an attack, a second zone and a regiment raised outside the zone all refused; a change to close order taken), Concord is ready at once, Ember never readies so the 90 seconds run out, then a paced advance |
| `field-champions-1v1.gcr` | Ember Clans against Choir, both Veteran AI, with champions allowed: two champions duel between the lines and one kills the other before the armies close (replay format 4) |
| `field-8000pt-2v2.gcr` | The `gc_bench` grand battle: four armies of about 1,500 soldiers on a 384-tile map, every regiment charging the nearest enemy |

`cargo run --release -p gc_replay -- info replays` describes each one. Between them they give every kind of command; `the_goldens_cover_every_kind_of_command` in `gc_replay/tests/goldens.rs` checks it, so a new kind of command needs a golden that gives it.

## When a check fails

`cargo test` (`gc_replay/tests/goldens.rs`) and CI (`gc_replay verify` on macOS, Windows and Linux) play every replay back. A failure names the first second where the state differs.

- **The change wasn't meant to alter how battles play out:** it's a bug (or a determinism leak). Fix it.
- **It was** (new combat rules, a movement change): bless the replays in the same pull request and say why in its description. Blessing keeps the recorded commands and stores the hashes the simulation produces now:

  ```text
  cargo run --release -p gc_replay -- bless replays
  ```

- **Every bless also bumps `SIM_VERSION`** in `gc_sim/src/lib.rs` and updates the `PINNED` table in `gc_replay/tests/goldens.rs` (the test prints the new table). The sim version is in the build's name, so the lobby keeps builds with different rules apart, and it is stamped in each replay's header (format 7; format 8 adds the battle's result, `World::verdict`, as the recording ended), so the replay list marks replays made with other rules, and the viewer pauses at the first checkpoint where a replay plays differently and says so. A test fails if the goldens change without the bump.
- **Mac and Windows disagree:** a real cross-platform determinism bug; this is what the check exists to catch. Never bless to make it go away.

Retuning `data/units.ron` or changing the map generator doesn't touch these files: each replay keeps the unit table and the map it was recorded with.

## Recording them again

Only needed to add a golden, or when a battle no longer makes sense after big rule changes (blessing keeps the old orders, which may no longer fit). The recorder is an ignored test in `gc_game` that plays the battles headless with the computer players:

```text
cargo test --release -p gc_game record_golden_replays -- --ignored --nocapture
GOLDEN=field-1v1 cargo test --release -p gc_game record_golden_replays -- --ignored --nocapture
```
