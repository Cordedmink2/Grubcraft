# Audio assets

`sounds.gcsnd` is the game's whole sound bank: 32 sound effects (16 synthesised, 16 battle cries layered from recordings), 4 ambient beds and 2 music loops, compressed as IMA ADPCM (about 972 KB). The game embeds it with `include_bytes!` (`crates/gc_game/src/sounds.rs`).

## Where the sounds come from

The music, the battle sounds and the UI clicks are **synthesised by `tools/gc_sfx`** (`tools/gc_sfx/src/synth.rs`): original work, released with the rest of the game, with no licence to track.

The 16 **battle cries** are the exception. They are layered by `tools/gc_sfx/src/cries.rs` from real recorded voices kept under `assets-src/voices/` (CC0, plus one CC-BY clip whose author the credits screen credits). See the sources table below, and `assets-src/MANIFEST.md` for each source's page, author, licence evidence and file hashes.

To change a sound, edit its recipe and rewrite the bank from the repository root:

```text
cargo run -p gc_sfx                 # rewrites assets/audio/sounds.gcsnd
cargo run -p gc_sfx -- --wav out/   # also writes each sound as a WAV file to listen to
```

Commit the rewritten `sounds.gcsnd` and its `sounds.manifest` (baker version, bank hash and size) with the change. The tool reads the recordings from `assets-src/voices/` wherever it is run from. CI runs `cargo run -p gc_sfx -- --check`, which re-synthesises the bank and fails if it differs from the committed bank or manifest, so a recipe edit without a rewrite is caught. If a change to the `gc_sfx` tool itself (not a recipe) changes the bytes, bump `BAKER_VERSION` in `tools/gc_sfx/src/main.rs`.

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
| `EmberCharge` / `EmberRout` / `EmberRally` / `EmberVictory` | 1.5 / 1.2 / 1.2 / 1.6 s | Ember Clans' voice: a deep warcry with a roaring line of men and kiai (charge); men crying out in fear (rout); a ragged rising shout (rally); a roar of triumph (champion wins) |
| `BroodCharge` / `BroodRout` / `BroodRally` / `BroodVictory` | 1.4 / 1.1 / 1.1 / 1.6 s | Brood's voice: a swarm squealing over clicking mandibles and a hiss (charge); a falling shriek (rout); climbing chitter (rally); a shriek over a buzzing roar (champion wins) |
| `ChoirCharge` / `ChoirRout` / `ChoirRally` / `ChoirVictory` | 1.3 / 1.0 / 1.2 / 1.6 s | Shard Choir's voice: a sung open-fifth chord struck like a shout, glass and bells ringing (charge); a chord that sours and falls (rout); a rising A major chord and a climb of glass notes (rally); a wide bright chord with bells ringing out (champion wins) |
| `CarrionCharge` / `CarrionRout` / `CarrionRally` / `CarrionVictory` | 1.5 / 1.2 / 1.3 / 1.6 s | Carrion Concord's voice: hollow moans, zombie shouts and a rattle of bone (charge); dying wails and clatter (rout); a long climbing moan (rally); a deep chorus of the grave with bones beating time (champion wins) |
| `BedWind` | 8 s loop | Ambient bed: a low rush with a breathy upper band, swelling in gusts |
| `BedWater` | 8 s loop | Ambient bed: a hiss with a chuckling, bubbling band and scattered glugs |
| `BedWoods` | 8 s loop | Ambient bed: leaves and branches rustling over a faint hush |
| `BedBattle` | 8 s loop | Ambient bed: the roar of a crowd in four vowel-like bands, surging, with low drum thumps and now and then steel ringing |
| `WallHit` | 0.40 s | A melee blow lands on a building (a dull woody thunk, a short splintery crack, a little grit falling; dry and brief so many can overlap) |
| `SiegeImpact` | 0.90 s | A siege engine's or artillery shot hits a building (a deep boom, a crunch of timber and stone, rubble trickling down) |
| `SiegeLaunch` | 0.70 s | A siege engine or artillery piece fires at a building (a wooden creak, the heavy whump of the arm, a rushing whoosh) |
| `Collapse` | 2.60 s | A building is brought down (a cracking groan, a rumbling roar of masonry and timber with heavy thuds, settling into trickling debris) |
| `Thunder` | 2.45 s | Lightning in a storm (four sharp cracks and a darkening hiss, a deep thump, then a rolling rumble with a mid-range growl, dying away); played 0.3 to 3 s after the flash, centred, softer and duller the further the stroke |
| `BedMeadow` / `BedForest` / `BedFen` / `BedHighland` / `BedWaste` | 8 s loops | Land beds, mixed by each land's share of the ground round the camera: soft air with insect trills; a deep hush with leaves and the odd creak; a low drone with croaking frogs; thin whistling wind; dry hollow air with a rattle of small bones |
| `BedRain` / `BedFire` | 8 s loops | Rain on ground and leaves; the roar, sizzle and pops of burning woods |
| `HitParry` / `HitBlock` | 0.35 / 0.25 s | A blow turned aside (ring of steel); a blow on a shield (wood and a crack) |
| `HitFlesh` / `HitIchor` / `HitCrystal` / `HitBone` | 0.25-0.4 s | A blow into flesh; into the Brood's chitin and ichor; into the Choir's glass; into the Concord's bone |
| `HitArmour` / `HitHeavy` | 0.4 s | Under a blow on armour (clang); under a heavy or killing blow (deep thump) |
| `Kindle` / `FloodRush` | 1.0 / 1.9 s | A tile of woods catching fire; the rivers rising over the fords |
| `UiSelect` / `UiMove` / `UiAttack` | 0.1-0.24 s | Selecting a regiment; a move order; an attack order |
| `MusicCalm` | 19.2 s loop | Shakuhachi-style flute over a drone, sparse taiko, plucked string (D hirajoshi, 100 bpm) |
| `MusicBattle` | 19.2 s loop | Taiko on every beat with fills, plucked ostinato, brass pad. Same length as the calm loop; the two play in step and crossfade with battle intensity |

## Ambient beds

The four beds are not one-shots: the mixer plays each continuously at a level, pan and muffling the game sets (`Command::Bed`), gliding to new values so nothing clicks. They are synthesised by `tools/gc_sfx/src/synth.rs` from filtered noise shaped by slow random swells, each built longer than the loop and its tail crossfaded into its head so the join cannot be heard; all four are original work with no licence to track. `crates/gc_game/src/sounds.rs` drives them: the battle roar from the regiments in contact that the player can see, and wind, water and woods from the ground (cover, water line and height) round the camera. Distance also takes the highs off every placed one-shot (`Command::Play` has a `dull` byte), and a `pitch` byte plays it faster or slower (blows are pitched by weapon). Beds are indexed by `SoundId::BEDS`: wind, water, woods, battle roar, the five lands, rain and fire.

## Battle cries

Regiments shout in their faction's voice (`crates/gc_game/src/sounds.rs`, from what the simulation reports in `World::events` and `World::challenge`). The sounds are presentation only: nothing here touches the simulation.

| Event | Cry | Notes |
|---|---|---|
| A regiment charges | `*Charge` | Starts a moment before contact: a regiment on the march that is facing an enemy within 7 tiles (centre to centre) and can charge (not a shield wall, not a missile-only regiment). Hitting home at speed (`Contact` with a charge bonus) cries too, if it hasn't just done so |
| A regiment breaks (`Morale`: any state to Routing) | `*Rout` | |
| A regiment rallies (`Morale`: Routing to a state that isn't broken) | `*Rally` | |
| A champion wins the duel between the lines (`World::challenge`, outcome `Killed`) | `*Victory` | Once per duel, heard where it was fought, in the winning champion's regiment's faction voice |

A regiment's faction comes from its unit type. Cries are placed at the regiment and follow the same rules as the other battle sounds: quieter with distance, panned, and silent for a regiment the player cannot see or while a rejoining player catches up. A cry carries 1.3 times as far as the hearing range. The same regiment is not heard to cry the same cry again for 10 s (charge) or 8 s (rout, rally), and at most 4 cries sound at once; a cry that is left out does not use up the regiment's cool-down. Units with no morale (the Concord's undead) never rout, so the Concord's rout and rally cries only sound for a unit that has morale.

## Recorded sources (the battle cries)

All downloaded from OpenGameArt.org on 2026-10-16. The files are in `assets-src/voices/<directory>/`, each download in its own directory with the originals unmodified (and a `wav/` folder of the mono 16-bit WAV that `gc_sfx` reads for each `.ogg` or `.flac`). Licence evidence and SHA-256 hashes are in `assets-src/MANIFEST.md`; authors are on the credits screen (`C`) through `CREDITS.md`.

| Directory | Source (OpenGameArt page) | Author | Licence | Used for |
|---|---|---|---|---|
| `rpg-male-adventurer-wolfwoot/` | Voice Clip Pack - Male Adventurer RPG | wolfwoot / 2DPIXX | CC0 | Ember Clans: shouts, kiai, victory and fear cries |
| `battlecry-spookymodem/` | Battlecry | spookymodem | **CC-BY 3.0** (credit required) | Ember Clans: the deep warcry in the charge and victory cries |
| `zombie-skeleton-monster-voices-arcadeparty/` | Zombie / Skeleton / Monster Voice Effects | ArcadeParty / Snabisch | CC0 | Carrion Concord: zombie yells and death cries; a few human yells for the Ember Clans |
| `ghost-moans-qubodup/` | Ghost Monster Voice Moaning & Growling | qubodup | CC0 | Carrion Concord: hollow moans |
| `undead-moans-antumdeluge/` | Undead Moans | AntumDeluge | CC0 | Carrion Concord: the moans under every cry |
| `creature-sfx-80-rubberduck/` | 80 CC0 creature SFX | rubberduck / Chasersgaming | CC0 | Brood: alien, bug, scream, roar and monster clips |
| `insect-or-alien-scream-qubodup/` | Insect or alien scream | qubodup | CC0 | Brood: the long squeal |
| `gregorian-chant-christislord/` | Chant | christislord | CC0 (public domain) | Shard Choir: two steady stretches of a men's chant, pitched into a sung chord |
| `bell-dings-pwl/` | Bell dings/chimes | PWL | CC0 (public domain) | Shard Choir: bells under the chord |

How a cry is made (`tools/gc_sfx/src/voices.rs` and `cries.rs`): each take is cut, pitched (the speed changes with it, like a tape) and sometimes bent over its length, then several copies of each shout are layered with small differences in start (up to 0.25-0.7 s), pitch (up to 1.5-2 semitones) and loudness, so a regiment sounds like many soldiers and not one. The layer is filtered, thickened with quick echoes and squashed gently. The Brood's takes are pitched up and chopped at 19-28 Hz like mandibles over a hiss; the Shard Choir's chant is pitched up to A4 and above in an open or major chord with a few voices a hair apart, over real bell recordings and a synthesised glass ring; the Carrion Concord's are pitched down, band-limited and laid over bone knocks (synthesised).

## CC0 recordings on hand

The Kenney sound packs (Impact Sounds 130, RPG Audio 50, Interface Sounds 100, all CC0, `.ogg`) are downloaded into `assets-src/kenney/` for a later swap. **None are in the bank yet:** the synthesised clips stay until someone can listen to the candidates, because swapping blind could make things worse. Licence evidence is in `assets-src/MANIFEST.md`.

## Swapping in recorded sounds later

The bank only holds mono 16-bit clips with a sample rate and a loop flag. `gc_sfx` reads 16-bit PCM WAV (convert `.ogg`, `.flac` and `.mp3` once with `ffmpeg -i X -ac 1 -ar 44100 -c:a pcm_s16le X.wav` and commit the WAV beside the original; `voices.rs` has the reader, the pitching and layering). To use a recording (a CC0 pack, a friend's voice), decode it to samples in `gc_sfx`, put it in place of the synthesised clip, and record its source and licence in a table here. Music loops must stay the same length as each other so the layers remain in step.
