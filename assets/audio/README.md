# Audio assets

`sounds.gcsnd` is the game's whole sound bank: 11 sound effects and 2 music loops, compressed as IMA ADPCM (about 400 KB). The game embeds it with `include_bytes!` (`crates/gc_game/src/sounds.rs`).

## Where the sounds come from

Every sound is **synthesised by `tools/gc_sfx`** (`tools/gc_sfx/src/synth.rs`). There are no recordings and no third-party samples, so there is no licence to track: the sounds are original work, released with the rest of the game.

To change a sound, edit its recipe and rewrite the bank from the repository root:

```text
cargo run -p gc_sfx                 # rewrites assets/audio/sounds.gcsnd
cargo run -p gc_sfx -- --wav out/   # also writes each sound as a WAV file to listen to
```

Commit the rewritten `sounds.gcsnd` with the change.

## Catalogue

| Sound | Length | When it plays |
|---|---|---|
| `ArrowRelease` | 0.55 s | A volley leaves the archers (string twang and rush) |
| `ArrowImpact` | 0.35 s | A volley lands (a ragged handful of thunks) |
| `Clash` | 0.70 s | Melee blows: when soldiers fall, and a soft ambient clash from regiments in contact |
| `Charge` | 1.40 s | Two regiments come into contact (accelerating war-drum roll) |
| `Waver` | 1.00 s | A regiment starts to waver (low, uneasy tremolo) |
| `RoutHorn` | 1.25 s | A regiment breaks and runs (short-long horn signal) |
| `Shatter` | 1.60 s | A regiment shatters for good (crash, falling metal, boom) |
| `Rally` | 1.15 s | A routed regiment rallies (two rising horn notes) |
| `UiSelect` / `UiMove` / `UiAttack` | 0.1-0.24 s | Selecting a regiment; a move order; an attack order |
| `MusicCalm` | 19.2 s loop | Shakuhachi-style flute over a drone, sparse taiko, plucked string (D hirajoshi, 100 bpm) |
| `MusicBattle` | 19.2 s loop | Taiko on every beat with fills, plucked ostinato, brass pad. Same length as the calm loop; the two play in step and crossfade with battle intensity |

## CC0 recordings on hand

The Kenney sound packs (Impact Sounds 130, RPG Audio 50, Interface Sounds 100, all CC0, `.ogg`) are downloaded into `assets-src/kenney/` for a later swap. **None are in the bank yet:** the synthesised clips stay until someone can listen to the candidates, because swapping blind could make things worse. Licence evidence is in `assets-src/MANIFEST.md`.

## Swapping in recorded sounds later

The bank only holds mono 16-bit clips with a sample rate and a loop flag. To use a recording (a CC0 pack, a friend's voice), decode it to samples in `gc_sfx`, put it in place of the synthesised clip, and record its source and licence in a table here. Music loops must stay the same length as each other so the layers remain in step.
