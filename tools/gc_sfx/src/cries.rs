//! Battle cries: what a regiment shouts when it charges, breaks, rallies
//! and when its champion wins the duel between the lines. Each faction has
//! its own voice, built from real recordings (CC0, plus one CC-BY clip;
//! sources in `assets-src/MANIFEST.md`) layered with small offsets and
//! detunes so one clip sounds like a whole regiment:
//!
//! - Ember Clans: human war shouts (a deep warcry and a men's kiai pack).
//! - Brood: insect and alien screams, pitched up, clicking like mandibles,
//!   over a hiss.
//! - Shard Choir: a men's chant pitched into a sung chord, with struck glass
//!   and bells for the ringing.
//! - Carrion Concord: undead moans and zombie voices, pitched down, over a
//!   rattle of bone.

use crate::synth::{Buf, Rendered, SFX_RATE};
use crate::voices::{Layer, Take, crowd, glass, knock, ratio};
use gc_audio::{Cry, SoundId};

macro_rules! adv {
    ($f:literal) => {
        concat!("rpg-male-adventurer-wolfwoot/", $f, ".wav")
    };
}
macro_rules! snab {
    ($f:literal) => {
        concat!("zombie-skeleton-monster-voices-arcadeparty/", $f, ".wav")
    };
}
macro_rules! ghost {
    ($f:literal) => {
        concat!("ghost-moans-qubodup/", $f, ".wav")
    };
}
macro_rules! undead {
    ($n:literal) => {
        concat!("undead-moans-antumdeluge/wav/undead-", $n, ".wav")
    };
}
macro_rules! crea {
    ($f:literal) => {
        concat!("creature-sfx-80-rubberduck/wav/", $f, ".wav")
    };
}
macro_rules! bell {
    ($n:literal) => {
        concat!("bell-dings-pwl/bell_ding", $n, ".wav")
    };
}

const BATTLECRY: &str = "battlecry-spookymodem/Battlecry.wav";
const INSECT: &str = "insect-or-alien-scream-qubodup/wav/insectoralien.wav";
const CHANT_A: &str = "gregorian-chant-christislord/wav/chant-a.wav";
const CHANT_B: &str = "gregorian-chant-christislord/wav/chant-b.wav";

/// The cries, in [`SoundId`] order: for each faction its charge, rout,
/// rally and victory.
pub fn all() -> Vec<Rendered> {
    let mut out = Vec::new();
    for (faction, voice) in [ember, brood, choir, carrion].into_iter().enumerate() {
        for cry in [Cry::Charge, Cry::Rout, Cry::Rally, Cry::Victory] {
            let (layer, peak) = voice(cry);
            out.push(Rendered {
                id: SoundId::cry(faction as u8, cry),
                rate: SFX_RATE,
                looped: false,
                pcm: Buf::from_samples(layer.s, SFX_RATE).finish(peak, true),
            });
        }
    }
    out
}

/// Flatten several groups of takes into one list.
fn all_of(groups: Vec<Vec<Take>>) -> Vec<Take> {
    groups.into_iter().flatten().collect()
}

// ------------------------------------------------------------ Ember Clans

fn ember(cry: Cry) -> (Layer, f32) {
    let layer = match cry {
        // A deep warcry with a line of men's shouts roaring under and over
        // it, then short kiai from the ranks behind.
        Cry::Charge => Layer::new(1.5)
            .lay(&all_of(vec![
                vec![Take::new(BATTLECRY, 0.0, 0.0, 2.6).cut(0.46, 1.4)],
                crowd(
                    Take::new(adv!("attackbig1"), 0.0, -1.0, 0.55),
                    4,
                    0.25,
                    1.5,
                    1,
                ),
                crowd(
                    Take::new(adv!("attackbig5"), 0.08, -2.0, 0.5),
                    3,
                    0.3,
                    1.5,
                    2,
                ),
                crowd(
                    Take::new(adv!("attackbig2"), 0.2, 0.0, 0.45),
                    3,
                    0.3,
                    1.5,
                    3,
                ),
                crowd(
                    Take::new(adv!("attackbig0"), 0.12, -1.5, 0.4).cut(0.3, 1.2),
                    2,
                    0.3,
                    1.0,
                    4,
                ),
                vec![
                    Take::new(adv!("attack3"), 0.5, -1.0, 0.45),
                    Take::new(adv!("attack6"), 0.62, 0.0, 0.4),
                    Take::new(adv!("attack1"), 0.78, -2.0, 0.4),
                    Take::new(adv!("attack4"), 0.95, 1.0, 0.35),
                ],
            ]))
            .band(110.0, 9500.0)
            .bloom(0.55)
            .shape(0.012, 0.3)
            .squash(1.6),
        // Men crying out in fear as they turn and run.
        Cry::Rout => Layer::new(1.2)
            .lay(&all_of(vec![
                crowd(
                    Take::new(adv!("hurt2"), 0.0, 2.0, 0.6).glide(-1.5),
                    3,
                    0.35,
                    1.5,
                    5,
                ),
                crowd(
                    Take::new(adv!("hurt3"), 0.05, 3.0, 0.55).glide(-1.0),
                    3,
                    0.35,
                    1.5,
                    6,
                ),
                crowd(Take::new(adv!("hurt6"), 0.1, 1.0, 0.55), 3, 0.4, 1.5, 7),
                crowd(Take::new(adv!("hurt8"), 0.15, 3.0, 0.5), 2, 0.4, 1.5, 8),
                vec![
                    Take::new(snab!("humanDeath1"), 0.2, 1.0, 1.2),
                    Take::new(snab!("humanYell4"), 0.35, 4.0, 1.8).glide(-3.0),
                ],
            ]))
            .band(150.0, 9500.0)
            .bloom(0.5)
            .shape(0.01, 0.3)
            .squash(1.4),
        // "Hold! Back to the line!": a rising, ragged shout.
        Cry::Rally => Layer::new(1.2)
            .lay(&all_of(vec![
                crowd(
                    Take::new(adv!("yes0"), 0.0, 0.0, 0.6).glide(1.5),
                    4,
                    0.3,
                    1.5,
                    9,
                ),
                crowd(
                    Take::new(adv!("victory1"), 0.05, -1.0, 0.5),
                    3,
                    0.3,
                    1.5,
                    10,
                ),
                crowd(Take::new(adv!("attack4"), 0.3, 1.0, 0.5), 3, 0.3, 1.5, 11),
                crowd(Take::new(adv!("attack8"), 0.4, 0.0, 0.5), 3, 0.3, 1.5, 12),
                crowd(
                    Take::new(adv!("attackbig4"), 0.45, -1.0, 0.5),
                    3,
                    0.25,
                    1.5,
                    13,
                ),
                vec![Take::new(snab!("humanYell1"), 0.55, 0.0, 1.5)],
            ]))
            .band(120.0, 9500.0)
            .bloom(0.5)
            .shape(0.01, 0.25)
            .squash(1.5),
        // A roar of triumph from the whole regiment.
        Cry::Victory => Layer::new(1.6)
            .lay(&all_of(vec![
                crowd(Take::new(adv!("victory0"), 0.0, -1.0, 0.6), 4, 0.3, 1.5, 14),
                crowd(Take::new(adv!("victory1"), 0.1, 0.0, 0.55), 4, 0.3, 1.5, 15),
                crowd(Take::new(adv!("yes1"), 0.25, -1.0, 0.45), 3, 0.3, 1.5, 16),
                crowd(
                    Take::new(adv!("attackbig3"), 0.3, 0.0, 0.45),
                    3,
                    0.3,
                    1.5,
                    17,
                ),
                vec![Take::new(adv!("jump0"), 0.6, -2.0, 0.5)],
                vec![Take::new(BATTLECRY, 0.1, 2.0, 1.4).cut(0.46, 1.0)],
            ]))
            .band(110.0, 9500.0)
            .bloom(0.6)
            .shape(0.015, 0.35)
            .squash(1.6),
    };
    (layer, 0.9)
}

// ----------------------------------------------------------------- Brood

/// A hiss of air through a thin gap: noise in the top of the range.
fn hiss(seconds: f32, at: f32, dur: f32, amp: f32, seed: u32) -> Layer {
    Layer::new(seconds)
        .noise(at, dur, amp, seed)
        .band(3500.0, 9500.0)
}

fn brood(cry: Cry) -> (Layer, f32) {
    let layer = match cry {
        // A swarm screaming as it comes: a long squeal, alien screams
        // stacked on it, and clicking underneath.
        Cry::Charge => {
            let squeal = Layer::new(1.4)
                .lay(&all_of(vec![
                    vec![Take::new(INSECT, 0.0, 4.0, 0.7).glide(-1.0)],
                    crowd(
                        Take::new(crea!("alien_03"), 0.05, 3.0, 0.7),
                        3,
                        0.35,
                        2.0,
                        18,
                    ),
                    crowd(
                        Take::new(crea!("alien_04"), 0.15, 2.0, 0.6),
                        3,
                        0.35,
                        2.0,
                        19,
                    ),
                    vec![Take::new(crea!("scream_01"), 0.2, 6.0, 0.4).glide(2.0)],
                ]))
                .band(600.0, 10_000.0);
            let clicks = Layer::new(1.4)
                .lay(&all_of(vec![
                    crowd(Take::new(crea!("bug_02"), 0.1, 0.0, 0.8), 4, 0.7, 2.0, 20),
                    crowd(Take::new(crea!("bug_04"), 0.3, 1.0, 0.7), 3, 0.7, 2.0, 21),
                ]))
                .chatter(26.0, 0.55)
                .band(500.0, 10_000.0);
            squeal
                .plus(&clicks)
                .plus(&hiss(1.4, 0.0, 1.3, 0.12, 22))
                .bloom(0.4)
                .shape(0.01, 0.3)
                .squash(1.5)
        }
        // A rising, falling shriek: the swarm scatters.
        Cry::Rout => Layer::new(1.1)
            .lay(&all_of(vec![
                crowd(
                    Take::new(crea!("scream_02"), 0.0, 7.0, 0.6).glide(-4.0),
                    3,
                    0.3,
                    2.0,
                    23,
                ),
                crowd(
                    Take::new(crea!("scream_01"), 0.1, 5.0, 0.5).glide(-3.0),
                    2,
                    0.3,
                    2.0,
                    24,
                ),
                crowd(
                    Take::new(crea!("alien_05"), 0.15, 4.0, 0.7),
                    3,
                    0.4,
                    2.0,
                    25,
                ),
                crowd(
                    Take::new(crea!("alien_06"), 0.25, 3.0, 0.9),
                    3,
                    0.4,
                    2.0,
                    26,
                ),
                crowd(Take::new(crea!("bug_03"), 0.3, 2.0, 0.6), 3, 0.5, 2.0, 27),
            ]))
            .band(700.0, 10_000.0)
            .plus(&hiss(1.1, 0.0, 1.0, 0.1, 28))
            .bloom(0.4)
            .shape(0.01, 0.3)
            .squash(1.4),
        // Quick chitter, climbing: the brood closes up again.
        Cry::Rally => {
            let chitter = Layer::new(1.1)
                .lay(&all_of(vec![
                    crowd(Take::new(crea!("bug_01"), 0.0, 3.0, 0.9), 5, 0.6, 2.0, 29),
                    crowd(
                        Take::new(crea!("bug_03"), 0.15, 1.0, 0.8).glide(2.0),
                        4,
                        0.6,
                        2.0,
                        30,
                    ),
                    crowd(Take::new(crea!("bug_02"), 0.3, 2.0, 0.6), 3, 0.5, 2.0, 31),
                ]))
                .chatter(19.0, 0.6);
            let call = Layer::new(1.1).lay(&all_of(vec![
                crowd(
                    Take::new(crea!("alien_01"), 0.35, 2.0, 0.7).glide(4.0),
                    3,
                    0.3,
                    2.0,
                    32,
                ),
                vec![Take::new(crea!("alien_06"), 0.55, 4.0, 0.9).glide(3.0)],
            ]));
            chitter
                .plus(&call)
                .band(500.0, 10_000.0)
                .plus(&hiss(1.1, 0.0, 0.9, 0.08, 33))
                .bloom(0.4)
                .shape(0.01, 0.25)
                .squash(1.4)
        }
        // A triumphant shriek over a low, buzzing roar.
        Cry::Victory => Layer::new(1.6)
            .lay(&all_of(vec![
                crowd(Take::new(crea!("roar_02"), 0.0, 5.0, 0.7), 3, 0.3, 2.0, 34),
                crowd(
                    Take::new(crea!("monster_03"), 0.1, 8.0, 0.6),
                    3,
                    0.3,
                    2.0,
                    35,
                ),
                vec![Take::new(INSECT, 0.4, 1.0, 0.8).glide(3.0)],
                crowd(Take::new(crea!("alien_02"), 0.5, 3.0, 0.7), 3, 0.4, 2.0, 36),
                crowd(
                    Take::new(crea!("scream_01"), 0.6, 4.0, 0.4).glide(2.0),
                    2,
                    0.3,
                    2.0,
                    37,
                ),
            ]))
            .band(400.0, 10_000.0)
            .plus(&hiss(1.6, 0.2, 1.3, 0.1, 38))
            .bloom(0.45)
            .shape(0.01, 0.4)
            .squash(1.5),
    };
    (layer, 0.9)
}

// ------------------------------------------------------------ Shard Choir

/// A sung chord from the chant, `notes` semitones above its own pitch
/// (222 Hz, about A3), each note a few voices a hair apart, so it swells
/// like a choir. `bend` raises (or lowers) it over its length.
fn chord(
    seconds: f32,
    file: &'static str,
    at: f32,
    notes: &[f32],
    bend: f32,
    gain: f32,
    seed: u32,
) -> Layer {
    let mut takes = Vec::new();
    for (i, &note) in notes.iter().enumerate() {
        let start = at + 0.02 * i as f32;
        let take = Take::new(file, start, note, gain)
            .glide(bend)
            .hold(seconds - start);
        takes.extend(crowd(take, 3, 0.04, 0.25, seed + i as u32));
    }
    Layer::new(seconds).lay(&takes)
}

/// Strike glass: a bell recording up high, and a struck-glass ring.
fn chime(layer: Layer, strikes: &[(f32, f32, f32)]) -> Layer {
    let mut layer = layer;
    for &(at, note, amp) in strikes {
        // `note` is semitones above A4 (440 Hz).
        glass(&mut layer, at, 440.0 * ratio(note), amp, 0.55);
    }
    layer
}

fn choir(cry: Cry) -> (Layer, f32) {
    let layer = match cry {
        // A sung chord struck like a shout: open fifths in A, glass rung
        // on top.
        Cry::Charge => {
            let voices = chord(1.3, CHANT_A, 0.0, &[12.0, 19.0, 24.0, 28.0], 0.0, 0.9, 40)
                .band(250.0, 10_000.0)
                .shape(0.02, 0.5);
            let bells = Layer::new(1.3).lay(&[
                Take::new(bell!("2"), 0.0, 12.0, 0.5),
                Take::new(bell!("3"), 0.06, 7.0, 0.4),
            ]);
            chime(
                voices.plus(&bells),
                &[(0.0, 12.0, 0.18), (0.08, 19.0, 0.14), (0.18, 24.0, 0.1)],
            )
            .bloom(0.5)
            .squash(1.3)
        }
        // A chord that sours and falls: a semitone apart, bent down, glass
        // cracking off it.
        Cry::Rout => {
            let voices = chord(1.0, CHANT_A, 0.0, &[12.0, 13.0, 18.0], -6.0, 0.9, 50)
                .band(250.0, 9000.0)
                .shape(0.015, 0.4);
            let cracks = Layer::new(1.0).lay(&[
                Take::new(bell!("4"), 0.0, 6.0, 0.6).glide(-4.0),
                Take::new(bell!("1"), 0.15, 9.0, 0.6),
            ]);
            chime(voices.plus(&cracks), &[(0.0, 13.0, 0.12), (0.12, 7.0, 0.1)])
                .bloom(0.4)
                .squash(1.3)
        }
        // A rising A major chord, and a quick climb of glass notes.
        Cry::Rally => {
            let voices = chord(1.2, CHANT_B, 0.0, &[12.0, 16.0, 19.0, 24.0], 4.0, 1.0, 60)
                .band(250.0, 10_000.0)
                .shape(0.04, 0.45);
            let bells = Layer::new(1.2).lay(&[Take::new(bell!("1"), 0.3, 12.0, 0.7)]);
            chime(
                voices.plus(&bells),
                &[
                    (0.25, 0.0, 0.14),
                    (0.4, 4.0, 0.14),
                    (0.55, 7.0, 0.14),
                    (0.7, 12.0, 0.16),
                ],
            )
            .bloom(0.5)
            .squash(1.3)
        }
        // The whole choir on a bright, wide chord, with bells ringing out.
        Cry::Victory => {
            let voices = chord(
                1.6,
                CHANT_A,
                0.0,
                &[12.0, 16.0, 19.0, 24.0, 28.0],
                0.5,
                0.8,
                70,
            )
            .plus(&chord(1.6, CHANT_B, 0.1, &[7.0, 12.0, 19.0], 0.0, 0.6, 80))
            .band(250.0, 10_000.0)
            .shape(0.05, 0.6);
            let bells = Layer::new(1.6).lay(&[
                Take::new(bell!("3"), 0.0, 5.0, 0.7),
                Take::new(bell!("2"), 0.2, 12.0, 0.5),
            ]);
            chime(
                voices.plus(&bells),
                &[
                    (0.0, 0.0, 0.16),
                    (0.15, 7.0, 0.14),
                    (0.3, 12.0, 0.14),
                    (0.45, 16.0, 0.12),
                ],
            )
            .bloom(0.6)
            .squash(1.3)
        }
    };
    (layer, 0.88)
}

// ------------------------------------------------------- Carrion Concord

/// Bones knocking: a loose rattle of short, dry clicks at `times`.
fn rattle(layer: &mut Layer, times: &[f32], amp: f32, seed: u32) {
    for (i, &t) in times.iter().enumerate() {
        let freq = 1100.0 + 380.0 * ((i * 5 + seed as usize) % 7) as f32;
        knock(
            layer,
            t,
            freq,
            amp * (0.6 + 0.1 * (i % 4) as f32),
            seed * 31 + i as u32,
        );
    }
}

fn carrion(cry: Cry) -> (Layer, f32) {
    let layer = match cry {
        // Moans rising from the ground, zombie shouts and bones rattling.
        Cry::Charge => {
            let mut bones = Layer::new(1.5);
            rattle(
                &mut bones,
                &[
                    0.05, 0.12, 0.22, 0.31, 0.37, 0.52, 0.6, 0.69, 0.8, 0.9, 1.02, 1.1, 1.2,
                ],
                0.5,
                1,
            );
            Layer::new(1.5)
                .lay(&all_of(vec![
                    crowd(
                        Take::new(undead!("1"), 0.0, -2.0, 1.0).glide(-1.0),
                        3,
                        0.3,
                        1.5,
                        90,
                    ),
                    crowd(Take::new(undead!("2"), 0.15, -4.0, 1.0), 3, 0.3, 1.5, 91),
                    vec![Take::new(ghost!("qubodup-GhostMoan05"), 0.2, -1.0, 0.6)],
                    crowd(
                        Take::new(snab!("zombieYell1"), 0.45, -3.0, 2.0),
                        3,
                        0.3,
                        1.5,
                        92,
                    ),
                    crowd(
                        Take::new(snab!("zombieYell6"), 0.6, -5.0, 1.0),
                        3,
                        0.3,
                        1.5,
                        93,
                    ),
                    vec![Take::new(snab!("zombieYell2"), 0.75, -3.0, 2.0)],
                ]))
                .band(70.0, 6500.0)
                .plus(&bones)
                .bloom(0.6)
                .shape(0.03, 0.3)
                .squash(1.4)
        }
        // Dying wails and a clatter, falling away.
        Cry::Rout => {
            let mut bones = Layer::new(1.2);
            rattle(
                &mut bones,
                &[0.1, 0.2, 0.28, 0.4, 0.47, 0.6, 0.75, 0.9],
                0.45,
                2,
            );
            Layer::new(1.2)
                .lay(&all_of(vec![
                    crowd(
                        Take::new(snab!("zombieDeath1"), 0.0, -2.0, 2.0),
                        2,
                        0.2,
                        1.5,
                        94,
                    ),
                    crowd(
                        Take::new(snab!("zombieDeath2"), 0.1, -1.0, 1.5),
                        2,
                        0.2,
                        1.5,
                        95,
                    ),
                    crowd(
                        Take::new(snab!("zombieDeath3"), 0.2, -3.0, 1.5),
                        2,
                        0.3,
                        1.5,
                        96,
                    ),
                    vec![Take::new(snab!("zombieDeath4"), 0.3, -2.0, 1.5)],
                    crowd(
                        Take::new(undead!("4"), 0.0, -2.0, 1.2).glide(-5.0),
                        3,
                        0.3,
                        1.5,
                        97,
                    ),
                ]))
                .band(70.0, 6500.0)
                .plus(&bones)
                .bloom(0.6)
                .shape(0.02, 0.4)
                .squash(1.4)
        }
        // A long moan climbing, as the dead pull themselves together.
        Cry::Rally => {
            let mut bones = Layer::new(1.3);
            rattle(
                &mut bones,
                &[0.3, 0.4, 0.5, 0.58, 0.7, 0.8, 0.88, 0.97],
                0.45,
                3,
            );
            Layer::new(1.3)
                .lay(&all_of(vec![
                    crowd(
                        Take::new(undead!("3"), 0.0, 0.0, 1.8).glide(4.0),
                        3,
                        0.3,
                        1.5,
                        98,
                    ),
                    vec![Take::new(ghost!("qubodup-GhostMoan05"), 0.1, 2.0, 0.6).glide(3.0)],
                    crowd(
                        Take::new(snab!("zombieYell7"), 0.5, -2.0, 2.5),
                        2,
                        0.2,
                        1.5,
                        99,
                    ),
                    crowd(
                        Take::new(snab!("zombieYell3"), 0.6, -3.0, 3.0),
                        2,
                        0.2,
                        1.5,
                        100,
                    ),
                ]))
                .band(70.0, 6500.0)
                .plus(&bones)
                .bloom(0.6)
                .shape(0.04, 0.3)
                .squash(1.4)
        }
        // A deep, hollow chorus of the grave, bones beating time.
        Cry::Victory => {
            let mut bones = Layer::new(1.6);
            rattle(
                &mut bones,
                &[
                    0.2, 0.35, 0.5, 0.62, 0.72, 0.82, 0.9, 0.97, 1.03, 1.09, 1.14, 1.19,
                ],
                0.55,
                4,
            );
            Layer::new(1.6)
                .lay(&all_of(vec![
                    vec![Take::new(ghost!("qubodup-GhostMoan01"), 0.0, -2.0, 0.7).cut(0.0, 1.8)],
                    crowd(
                        Take::new(undead!("1"), 0.05, -6.0, 1.2).glide(2.0),
                        3,
                        0.3,
                        1.5,
                        101,
                    ),
                    crowd(
                        Take::new(snab!("zombieYell6"), 0.4, -4.0, 1.2),
                        3,
                        0.4,
                        1.5,
                        102,
                    ),
                    vec![Take::new(crea!("monster_04"), 0.2, -3.0, 0.5)],
                ]))
                .band(60.0, 6500.0)
                .plus(&bones)
                .bloom(0.6)
                .shape(0.04, 0.4)
                .squash(1.4)
        }
    };
    (layer, 0.9)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_faction_has_four_cries_of_a_sensible_length_and_loudness() {
        let cries = all();
        assert_eq!(cries.len(), 16);
        for c in &cries {
            let seconds = c.pcm.len() as f32 / c.rate as f32;
            assert!((0.6..=1.6).contains(&seconds), "{:?} is {seconds}s", c.id);
            let rms = (c.pcm.iter().map(|&s| f32::from(s).powi(2)).sum::<f32>()
                / c.pcm.len() as f32)
                .sqrt()
                / 32768.0;
            assert!(rms > 0.08, "{:?} is too thin ({rms})", c.id);
            assert!(!c.looped);
        }
        for (i, a) in cries.iter().enumerate() {
            assert!(
                cries[i + 1..].iter().all(|b| a.pcm != b.pcm),
                "{:?} repeats another cry",
                a.id
            );
        }
    }
}
