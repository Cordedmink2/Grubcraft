//! The sounds themselves: small additive/subtractive synthesis recipes.

use gc_audio::SoundId;
use std::f32::consts::TAU;

pub const SFX_RATE: u32 = 22_050;
pub const MUSIC_RATE: u32 = 16_000;
/// The ambient beds are filtered noise with little above 5 kHz.
pub const BED_RATE: u32 = 16_000;
/// How long each bed loops, in seconds.
pub const BED_SECONDS: f32 = 8.0;

pub struct Rendered {
    pub id: SoundId,
    pub rate: u32,
    pub looped: bool,
    pub pcm: Vec<i16>,
}

/// Every clip in the bank, in [`SoundId`] order.
pub fn all() -> Vec<Rendered> {
    let sfx = |id, peak: f32, buf: Buf| Rendered {
        id,
        rate: SFX_RATE,
        looped: false,
        pcm: buf.finish(peak, true),
    };
    let music = |id, buf: Buf| Rendered {
        id,
        rate: MUSIC_RATE,
        looped: true,
        pcm: buf.finish(0.8, false),
    };
    let mut clips = vec![
        sfx(SoundId::ArrowRelease, 0.55, arrow_release()),
        sfx(SoundId::ArrowImpact, 0.7, arrow_impact()),
        sfx(SoundId::Clash, 0.85, clash()),
        sfx(SoundId::Charge, 0.9, charge()),
        sfx(SoundId::Waver, 0.6, waver()),
        sfx(SoundId::RoutHorn, 0.85, rout_horn()),
        sfx(SoundId::Shatter, 0.9, shatter()),
        sfx(SoundId::Rally, 0.8, rally()),
        sfx(SoundId::UiSelect, 0.45, ui_select()),
        sfx(SoundId::UiMove, 0.45, ui_move()),
        sfx(SoundId::UiAttack, 0.5, ui_attack()),
        music(SoundId::MusicCalm, music_calm()),
        music(SoundId::MusicBattle, music_battle()),
    ];
    // The battle cries come from recordings (see `cries`).
    clips.extend(crate::cries::all());
    // The ambient beds (loops the game plays continuously under everything).
    let bed = |id, buf: Buf| Rendered {
        id,
        rate: BED_RATE,
        looped: true,
        pcm: buf.finish(0.8, false),
    };
    clips.push(bed(SoundId::BedWind, bed_wind()));
    clips.push(bed(SoundId::BedWater, bed_water()));
    clips.push(bed(SoundId::BedWoods, bed_woods()));
    clips.push(bed(SoundId::BedBattle, bed_battle()));
    // The sounds of attacking buildings.
    clips.push(sfx(SoundId::WallHit, 0.85, wall_hit()));
    clips.push(sfx(SoundId::SiegeImpact, 0.9, siege_impact()));
    clips.push(sfx(SoundId::SiegeLaunch, 0.85, siege_launch()));
    clips.push(sfx(SoundId::Collapse, 0.9, collapse()));
    clips.push(sfx(SoundId::Thunder, 0.9, thunder()));
    // The beds of the lands, the rain and the fire.
    clips.push(bed(SoundId::BedMeadow, bed_meadow()));
    clips.push(bed(SoundId::BedForest, bed_forest()));
    clips.push(bed(SoundId::BedFen, bed_fen()));
    clips.push(bed(SoundId::BedHighland, bed_highland()));
    clips.push(bed(SoundId::BedWaste, bed_waste()));
    clips.push(bed(SoundId::BedRain, bed_rain()));
    clips.push(bed(SoundId::BedFire, bed_fire()));
    // Blows on soldiers, by what they land on, and the weather's one-shots.
    clips.push(sfx(SoundId::HitParry, 0.8, hit_parry()));
    clips.push(sfx(SoundId::HitBlock, 0.8, hit_block()));
    clips.push(sfx(SoundId::HitFlesh, 0.8, hit_flesh()));
    clips.push(sfx(SoundId::HitIchor, 0.8, hit_ichor()));
    clips.push(sfx(SoundId::HitCrystal, 0.75, hit_crystal()));
    clips.push(sfx(SoundId::HitBone, 0.8, hit_bone()));
    clips.push(sfx(SoundId::HitArmour, 0.75, hit_armour()));
    clips.push(sfx(SoundId::HitHeavy, 0.9, hit_heavy()));
    clips.push(sfx(SoundId::Kindle, 0.8, kindle()));
    clips.push(sfx(SoundId::FloodRush, 0.85, flood_rush()));
    clips
}

// ------------------------------------------------------------------ beds
//
// Ambient loops: filtered noise shaped by slow random swells. Each is built
// longer than the loop and its tail faded into its head, so the join cannot
// be heard.

/// `len` samples of noise from `seed`, run through `f` (which owns any
/// filter state), then made to loop: the extra `fade` samples `f` produced
/// past `len` are crossfaded (equal power, since the noise is unrelated)
/// into the start.
fn looped_noise(len: usize, seed: u32, mut f: impl FnMut(f32, usize) -> f32) -> Vec<f32> {
    let fade = len / 8;
    let mut rng = Rng(seed);
    let raw: Vec<f32> = (0..len + fade).map(|i| f(rng.noise(), i)).collect();
    (0..len)
        .map(|i| {
            if i < fade {
                let w = (i as f32 / fade as f32) * (TAU / 4.0);
                raw[i] * w.sin() + raw[len + i] * w.cos()
            } else {
                raw[i]
            }
        })
        .collect()
}

/// A slow random swell between -1 and 1, for gusts and surges: noise below
/// `cutoff` Hz, scaled to fill the range.
fn swell(len: usize, rate: f32, cutoff: f32, seed: u32) -> Vec<f32> {
    let mut lp = OnePole::default();
    let mut track = looped_noise(len, seed, |n, _| lp.low(n, cutoff, rate));
    let top = track.iter().fold(1e-6f32, |m, v| m.max(v.abs()));
    track.iter_mut().for_each(|v| *v /= top);
    track
}

fn bed_buf(samples: Vec<f32>) -> Buf {
    let mut b = Buf::looping(BED_SECONDS, BED_RATE);
    b.s = samples;
    b
}

/// Wind: a low rush with a breathy upper band, swelling in gusts.
fn bed_wind() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let gust = swell(len, rate, 0.35, 11);
    let flutter = swell(len, rate, 3.0, 12);
    let (mut low, mut hi, mut band) = (Pole2::default(), OnePole::default(), Pole2::default());
    let samples = looped_noise(len, 13, |n, i| {
        let g = (gust[i % len] + 1.0) * 0.5;
        let fl = (flutter[i % len] + 1.0) * 0.5;
        let rush = low.low(n, 180.0 + 220.0 * g, rate);
        let breath = band.low(hi.high(n, 600.0, rate), 1100.0 + 700.0 * g, rate);
        (rush * 2.2 + breath * (0.5 + 0.5 * fl)) * (0.25 + 0.75 * g * g)
    });
    bed_buf(samples)
}

/// Water: a steady hiss with a bubbling, chuckling band over it and a few
/// glugs.
fn bed_water() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let surge = swell(len, rate, 0.5, 21);
    let bubble = swell(len, rate, 14.0, 22);
    let (mut hp_a, mut lp_a, mut hp_b, mut lp_b) = (
        OnePole::default(),
        Pole2::default(),
        OnePole::default(),
        Pole2::default(),
    );
    let samples = looped_noise(len, 23, |n, i| {
        let s = (surge[i % len] + 1.0) * 0.5;
        let b = bubble[i % len].max(0.0);
        let hiss = lp_a.low(hp_a.high(n, 700.0, rate), 4200.0, rate);
        let chuckle = lp_b.low(hp_b.high(n, 350.0, rate), 1100.0, rate);
        hiss * (0.55 + 0.45 * s) + chuckle * b * b * 3.0
    });
    let mut buf = bed_buf(samples);
    // Glugs: short rising notes, scattered (the buffer wraps).
    let mut rng = Rng(24);
    for k in 0..30u32 {
        let at = (rng.noise() * 0.5 + 0.5) * BED_SECONDS;
        let base = 380.0 + 420.0 * (rng.noise() * 0.5 + 0.5);
        let amp = 0.12 + 0.12 * (rng.noise() * 0.5 + 0.5);
        let mut phase = 0.0f32;
        buf.at(
            at,
            |t, _| {
                phase += TAU * base * (1.0 + 2.2 * t) / rate;
                phase.sin() * amp * (t / 0.004).min(1.0) * decay(t, 0.018)
            },
            0.08,
            25 + k,
        );
    }
    buf
}

/// Woods: leaves and branches, a hiss that breaks into rustling bursts over
/// a faint hush.
fn bed_woods() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let gust = swell(len, rate, 0.4, 31);
    let rustle = swell(len, rate, 9.0, 32);
    let (mut hp, mut lp, mut hush) = (OnePole::default(), OnePole::default(), OnePole::default());
    let samples = looped_noise(len, 33, |n, i| {
        let g = (gust[i % len] + 1.0) * 0.5;
        let r = (rustle[i % len] + 1.0) * 0.5;
        let leaves = lp.low(hp.high(n, 2200.0, rate), 6500.0, rate);
        let low = hush.low(n, 420.0, rate);
        leaves * r * r * (0.3 + 0.7 * g) * 2.0 + low * (0.25 + 0.5 * g)
    });
    bed_buf(samples)
}

/// The roar of a battle heard from a distance: a crowd's voices in four
/// vowel-like bands, surging, with a low rumble of drums and boots and the
/// odd ring of steel.
fn bed_battle() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let surge = swell(len, rate, 0.45, 41);
    let bands: [(f32, f32); 4] = [(250.0, 1.0), (520.0, 0.9), (900.0, 0.6), (1500.0, 0.3)];
    let mut shaped: Vec<(Vec<f32>, OnePole, Pole2)> = bands
        .iter()
        .enumerate()
        .map(|(k, _)| {
            (
                swell(len, rate, 3.0 + k as f32 * 1.7, 42 + k as u32),
                OnePole::default(),
                Pole2::default(),
            )
        })
        .collect();
    let samples = looped_noise(len, 46, |n, i| {
        let s = (surge[i % len] + 1.0) * 0.5;
        let mut sum = 0.0;
        for ((modulator, hp, lp), (centre, weight)) in shaped.iter_mut().zip(bands) {
            let v = lp.low(hp.high(n, centre * 0.7, rate), centre * 1.2, rate);
            sum += v * weight * (0.55 + 0.45 * modulator[i % len]);
        }
        sum * (0.5 + 0.5 * s)
    });
    let mut buf = bed_buf(samples);
    // Drums and boots under it, and steel ringing here and there.
    let mut rng = Rng(47);
    for k in 0..12u32 {
        let at = (rng.noise() * 0.5 + 0.5) * BED_SECONDS;
        drum(
            &mut buf,
            at,
            0.12 + 0.1 * (rng.noise() * 0.5 + 0.5),
            55.0,
            50 + k,
        );
    }
    for k in 0..40u32 {
        let at = (rng.noise() * 0.5 + 0.5) * BED_SECONDS;
        let f = 900.0 + 1400.0 * (rng.noise() * 0.5 + 0.5);
        let amp = 0.05 + 0.06 * (rng.noise() * 0.5 + 0.5);
        buf.at(
            at,
            |t, _| (TAU * f * t).sin() * amp * decay(t, 0.05),
            0.2,
            100 + k,
        );
    }
    buf
}

/// Short hard ticks scattered through a loop (the buffer wraps): `count` of
/// them between `lo` and `hi` Hz, amplitude up to `amp`, each ringing for
/// about `tau` seconds.
fn scatter_ticks(buf: &mut Buf, count: u32, band: (f32, f32), amp: f32, tau: f32, seed: u32) {
    let mut rng = Rng(seed);
    let mut unit = move || rng.noise() * 0.5 + 0.5;
    for k in 0..count {
        let at = unit() * BED_SECONDS;
        let f = band.0 + (band.1 - band.0) * unit();
        tick(buf, at, f, amp * (0.3 + 0.7 * unit()), tau, seed + 1 + k);
    }
}

/// A meadow: soft moving air and the trills of insects in the grass.
fn bed_meadow() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let breeze = swell(len, rate, 0.3, 61);
    let mut air = Pole2::default();
    let samples = looped_noise(len, 62, |n, i| {
        let g = (breeze[i % len] + 1.0) * 0.5;
        air.low(n, 750.0, rate) * (0.4 + 0.6 * g) * 1.4
    });
    let mut buf = bed_buf(samples);
    // Trills: a high tone chopped fast, for a moment, now and then.
    let mut rng = Rng(63);
    for k in 0..22u32 {
        let at = (rng.noise() * 0.5 + 0.5) * BED_SECONDS;
        let f = 3200.0 + 1500.0 * (rng.noise() * 0.5 + 0.5);
        let chop = 38.0 + 30.0 * (rng.noise() * 0.5 + 0.5);
        let len = 0.25 + 0.5 * (rng.noise() * 0.5 + 0.5);
        buf.at(
            at,
            |t, _| {
                let gate = (TAU * chop * t).sin().max(0.0);
                (TAU * f * t).sin() * gate * adsr(t, 0.03, 0.08, len) * 0.07
            },
            len,
            64 + k,
        );
    }
    buf
}

/// A forest: a deep, close hush under the canopy, a faint leaf rustle and
/// the odd creak of a trunk.
fn bed_forest() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let gust = swell(len, rate, 0.25, 71);
    let (mut hush, mut hp, mut leaf) = (Pole2::default(), OnePole::default(), Pole2::default());
    let samples = looped_noise(len, 72, |n, i| {
        let g = (gust[i % len] + 1.0) * 0.5;
        let deep = hush.low(n, 380.0, rate) * 1.7;
        let rustle = leaf.low(hp.high(n, 2500.0, rate), 5000.0, rate) * 0.5;
        (deep + rustle * g * g) * (0.5 + 0.5 * g)
    });
    let mut buf = bed_buf(samples);
    let mut rng = Rng(73);
    for k in 0..4u32 {
        let at = (rng.noise() * 0.5 + 0.5) * BED_SECONDS;
        let base = 90.0 + 50.0 * (rng.noise() * 0.5 + 0.5);
        let mut phase = 0.0f32;
        buf.at(
            at,
            |t, r| {
                phase += TAU * base * (1.0 - 0.25 * t) / rate;
                let wobble = 1.0 + 0.5 * r.noise();
                (phase.sin() + 0.5 * (phase * 2.0).sin()) * adsr(t, 0.12, 0.3, 0.9) * 0.08 * wobble
            },
            0.9,
            74 + k,
        );
    }
    buf
}

/// A fen: a low drone over still water and the croaking of frogs.
fn bed_fen() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let surge = swell(len, rate, 0.2, 81);
    let mut mud = Pole2::default();
    let samples = looped_noise(len, 82, |n, i| {
        let g = (surge[i % len] + 1.0) * 0.5;
        mud.low(n, 480.0, rate) * (0.5 + 0.5 * g) * 1.3
    });
    let mut buf = bed_buf(samples);
    // The drone: two low tones whole cycles long in the loop, so it joins.
    let (f1, f2) = (56.0, 83.0);
    for (f, a) in [(f1, 0.1), (f2, 0.06)] {
        buf.at(0.0, |t, _| (TAU * f * t).sin() * a, BED_SECONDS, 83);
    }
    // Frogs: a croak is a low tone chopped at about 25 Hz; they call in pairs.
    let mut rng = Rng(84);
    for k in 0..9u32 {
        let at = (rng.noise() * 0.5 + 0.5) * BED_SECONDS;
        let f = 130.0 + 120.0 * (rng.noise() * 0.5 + 0.5);
        for r in 0..(2 + (k % 2)) {
            buf.at(
                at + 0.2 * r as f32,
                |t, _| {
                    let gate = (TAU * 26.0 * t).sin().max(0.0);
                    ((TAU * f * t).sin() + 0.5 * (TAU * f * 2.0 * t).sin())
                        * gate
                        * adsr(t, 0.01, 0.06, 0.16)
                        * 0.1
                },
                0.16,
                85 + k,
            );
        }
    }
    buf
}

/// Highland: thin wind over moor and rock, a narrow whistling howl in gusts.
fn bed_highland() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let gust = swell(len, rate, 0.3, 91);
    let (mut hp, mut band) = (OnePole::default(), Pole2::default());
    let samples = looped_noise(len, 92, |n, i| {
        let g = (gust[i % len] + 1.0) * 0.5;
        let howl = band.low(hp.high(n, 500.0, rate), 700.0 + 700.0 * g, rate);
        howl * (0.15 + 0.85 * g * g) * 2.6
    });
    let mut buf = bed_buf(samples);
    // A whistle in the gusts: a tone with a slow +-25 Hz waver, whole cycles
    // per loop so it joins.
    let f = 880.0;
    buf.at(
        0.0,
        |t, _| {
            let g = (gust[((t * rate) as usize) % len] + 1.0) * 0.5;
            let phase = TAU * f * t - 50.0 * (TAU * 0.5 * t).cos();
            phase.sin() * g.powi(3) * 0.05
        },
        BED_SECONDS,
        93,
    );
    buf
}

/// The bone waste: dry hollow air, a low moan in the hollows and the rattle
/// of small bones.
fn bed_waste() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let gust = swell(len, rate, 0.2, 101);
    let (mut hollow, mut dry, mut hp) = (Pole2::default(), Pole2::default(), OnePole::default());
    let samples = looped_noise(len, 102, |n, i| {
        let g = (gust[i % len] + 1.0) * 0.5;
        let moan = hollow.low(n, 260.0 + 140.0 * g, rate) * 2.0;
        let grit = dry.low(hp.high(n, 2800.0, rate), 6000.0, rate) * 0.25;
        (moan + grit) * (0.35 + 0.65 * g)
    });
    let mut buf = bed_buf(samples);
    scatter_ticks(&mut buf, 28, (1400.0, 3200.0), 0.12, 0.006, 103);
    buf
}

/// Rain: a steady patter on ground and leaves, thicker in the heavy bursts.
fn bed_rain() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let burst = swell(len, rate, 0.4, 111);
    let patter = swell(len, rate, 40.0, 112);
    let (mut hp, mut lp, mut low) = (OnePole::default(), Pole2::default(), Pole2::default());
    let samples = looped_noise(len, 113, |n, i| {
        let b = (burst[i % len] + 1.0) * 0.5;
        let p = (patter[i % len] + 1.0) * 0.5;
        let hiss = lp.low(hp.high(n, 1400.0, rate), 7000.0, rate);
        let body = low.low(n, 500.0, rate) * 0.5;
        (hiss * (0.55 + 0.45 * p) + body) * (0.7 + 0.3 * b)
    });
    bed_buf(samples)
}

/// Burning woods: a low roar of flame, a steady sizzle and a hail of pops.
fn bed_fire() -> Buf {
    let rate = BED_RATE as f32;
    let len = (BED_SECONDS * rate) as usize;
    let flare = swell(len, rate, 0.6, 121);
    let (mut roar, mut hp, mut sizz) = (Pole2::default(), OnePole::default(), Pole2::default());
    let samples = looped_noise(len, 122, |n, i| {
        let f = (flare[i % len] + 1.0) * 0.5;
        let low = roar.low(n, 260.0 + 300.0 * f, rate) * 2.0;
        let hiss = sizz.low(hp.high(n, 3500.0, rate), 7500.0, rate) * 0.3;
        (low + hiss) * (0.5 + 0.5 * f)
    });
    let mut buf = bed_buf(samples);
    scatter_ticks(&mut buf, 140, (700.0, 4200.0), 0.3, 0.004, 123);
    buf
}

/// Steel turned aside on steel: a bright ring with a sharp tick.
fn hit_parry() -> Buf {
    let mut b = Buf::new(0.35, SFX_RATE);
    crack(&mut b, 0.0, 0.8, 130);
    for (ratio, amp, tau) in [(1.0, 0.7, 0.12), (2.76, 0.5, 0.08), (5.4, 0.3, 0.05)] {
        let f = 1700.0 * ratio;
        b.at(
            0.0,
            |t, _| (TAU * f * t).sin() * amp * decay(t, tau),
            0.35,
            131,
        );
    }
    b
}

/// A blow taken on a shield: a dull knock of wood and a crack.
fn hit_block() -> Buf {
    let mut b = Buf::new(0.25, SFX_RATE);
    thud(&mut b, 0.0, 0.8, 150.0, 0.035, 140);
    knock(&mut b, 0.0, 420.0, 0.7, 141);
    crack(&mut b, 0.0, 0.5, 142);
    b
}

/// A blow into flesh: a dull thump and a breath of a slice.
fn hit_flesh() -> Buf {
    let mut b = Buf::new(0.25, SFX_RATE);
    let rate = b.rate;
    thud(&mut b, 0.0, 0.8, 140.0, 0.045, 150);
    let (mut hp, mut lp) = (OnePole::default(), Pole2::default());
    b.at(
        0.0,
        |t, rng| lp.low(hp.high(rng.noise(), 2000.0, rate), 5000.0, rate) * decay(t, 0.04) * 1.3,
        0.15,
        151,
    );
    b
}

/// A blow into the Brood: a wet crunch and squelch.
fn hit_ichor() -> Buf {
    let mut b = Buf::new(0.3, SFX_RATE);
    let rate = b.rate;
    let mut phase = 0.0f32;
    let mut lp = Pole2::default();
    b.at(
        0.0,
        |t, rng| {
            phase += TAU * (230.0 - 140.0 * t.min(0.2) / 0.2) / rate;
            let squelch = lp.low(rng.noise(), 900.0, rate) * (0.5 + 0.5 * (TAU * 38.0 * t).sin());
            (squelch * 0.9 + phase.sin() * 0.5) * decay(t, 0.07)
        },
        0.3,
        160,
    );
    crack(&mut b, 0.0, 0.5, 161);
    b
}

/// A blow into the Choir: glass chipping and a short, high chime.
fn hit_crystal() -> Buf {
    let mut b = Buf::new(0.4, SFX_RATE);
    crack(&mut b, 0.0, 0.9, 170);
    for (f, amp, tau) in [(2350.0, 0.5, 0.1), (3150.0, 0.4, 0.07), (4400.0, 0.3, 0.05)] {
        b.at(
            0.0,
            |t, _| (TAU * f * t).sin() * amp * decay(t, tau),
            0.4,
            171,
        );
    }
    tick(&mut b, 0.02, 5200.0, 0.4, 0.01, 172);
    b
}

/// A blow into the Concord: a dry crack of bone and a small clatter.
fn hit_bone() -> Buf {
    let mut b = Buf::new(0.3, SFX_RATE);
    crack(&mut b, 0.0, 1.0, 180);
    knock(&mut b, 0.0, 320.0, 0.6, 181);
    tick(&mut b, 0.05, 1800.0, 0.3, 0.008, 182);
    tick(&mut b, 0.09, 2400.0, 0.2, 0.008, 183);
    b
}

/// Under a blow on armour: a short, low clang.
fn hit_armour() -> Buf {
    let mut b = Buf::new(0.4, SFX_RATE);
    for (ratio, amp, tau) in [(1.0, 0.8, 0.16), (2.4, 0.6, 0.1), (4.1, 0.4, 0.06)] {
        let f = 540.0 * ratio;
        b.at(
            0.0,
            |t, _| (TAU * f * t).sin() * amp * decay(t, tau),
            0.4,
            190,
        );
    }
    crack(&mut b, 0.0, 0.5, 191);
    b
}

/// Under a heavy or crushing blow: a deep thump.
fn hit_heavy() -> Buf {
    let mut b = Buf::new(0.4, SFX_RATE);
    thud(&mut b, 0.0, 1.0, 52.0, 0.12, 200);
    thud(&mut b, 0.01, 0.5, 90.0, 0.06, 201);
    b
}

/// A tile of woods catching: a gathering whoosh and a few pops.
fn kindle() -> Buf {
    let mut b = Buf::new(1.0, SFX_RATE);
    let rate = b.rate;
    let (mut hp, mut lp) = (OnePole::default(), Pole2::default());
    b.at(
        0.0,
        |t, rng| {
            let n = lp.low(
                hp.high(rng.noise(), 120.0, rate),
                250.0 + 2600.0 * (t / 0.5).min(1.0),
                rate,
            );
            n * adsr(t, 0.18, 0.45, 1.0) * 1.5
        },
        1.0,
        210,
    );
    for k in 0..6u32 {
        tick(
            &mut b,
            0.25 + 0.1 * k as f32,
            900.0 + 400.0 * (k % 3) as f32,
            0.25,
            0.004,
            211 + k,
        );
    }
    b
}

/// The rivers rising: a long surge of rushing water with a deep rumble.
fn flood_rush() -> Buf {
    let mut b = Buf::new(1.9, SFX_RATE);
    let rate = b.rate;
    let (mut hp, mut lp, mut rumble) = (OnePole::default(), Pole2::default(), Pole2::default());
    b.at(
        0.0,
        |t, rng| {
            let n = rng.noise();
            let sweep = 500.0 + 1500.0 * (t / 1.0).min(1.0);
            let rush = lp.low(hp.high(n, 200.0, rate), sweep, rate);
            let low = rumble.low(n, 140.0, rate) * 1.5;
            (rush + low) * adsr(t, 0.6, 0.8, 1.9)
        },
        1.9,
        220,
    );
    b
}

// ---------------------------------------------------------------- toolkit

/// A mono float buffer being composed.
pub(crate) struct Buf {
    pub(crate) rate: f32,
    pub(crate) s: Vec<f32>,
    /// Sounds written near the end wrap around to the start (for loops).
    wrap: bool,
}

impl Buf {
    /// A finished mix at `rate`.
    pub(crate) fn from_samples(s: Vec<f32>, rate: u32) -> Buf {
        Buf {
            rate: rate as f32,
            s,
            wrap: false,
        }
    }

    pub(crate) fn new(seconds: f32, rate: u32) -> Buf {
        Buf {
            rate: rate as f32,
            s: vec![0.0; (seconds * rate as f32).round() as usize],
            wrap: false,
        }
    }

    fn looping(seconds: f32, rate: u32) -> Buf {
        Buf {
            wrap: true,
            ..Buf::new(seconds, rate)
        }
    }

    fn at(&mut self, start: f32, mut f: impl FnMut(f32, &mut Rng) -> f32, dur: f32, seed: u32) {
        let first = (start * self.rate).round() as usize;
        let n = (dur * self.rate) as usize;
        let mut rng = Rng(seed);
        for k in 0..n {
            let idx = first + k;
            let idx = if self.wrap {
                idx % self.s.len()
            } else if idx < self.s.len() {
                idx
            } else {
                break;
            };
            self.s[idx] += f(k as f32 / self.rate, &mut rng);
        }
    }

    /// Normalise to `peak`, fade the last few milliseconds, quantise.
    pub(crate) fn finish(mut self, peak: f32, fade: bool) -> Vec<i16> {
        let max = self.s.iter().fold(1e-6f32, |m, v| m.max(v.abs()));
        let n = self.s.len();
        let fade_len = (self.rate * 0.012) as usize;
        for (i, v) in self.s.iter_mut().enumerate() {
            *v *= peak / max;
            if fade && i + fade_len > n {
                *v *= (n - i) as f32 / fade_len as f32;
            }
        }
        self.s.iter().map(|v| (v * 32767.0) as i16).collect()
    }
}

/// A tiny xorshift noise source; every sound seeds its own so the output
/// never depends on render order.
pub(crate) struct Rng(pub(crate) u32);

impl Rng {
    pub(crate) fn noise(&mut self) -> f32 {
        let mut x = self.0.max(1);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn decay(t: f32, tau: f32) -> f32 {
    (-t / tau).exp()
}

/// Rise over `attack` seconds, hold, fall over `release` seconds before `dur`.
fn adsr(t: f32, attack: f32, release: f32, dur: f32) -> f32 {
    let a = (t / attack).min(1.0);
    let r = ((dur - t) / release).clamp(0.0, 1.0);
    a * a * (3.0 - 2.0 * a) * r
}

/// One-pole filter state, usable as low-pass or high-pass.
#[derive(Default)]
struct OnePole {
    y: f32,
}

impl OnePole {
    fn low(&mut self, x: f32, cutoff: f32, rate: f32) -> f32 {
        let a = 1.0 - (-TAU * cutoff / rate).exp();
        self.y += a * (x - self.y);
        self.y
    }

    fn high(&mut self, x: f32, cutoff: f32, rate: f32) -> f32 {
        x - self.low(x, cutoff, rate)
    }
}

/// Two one-pole low-passes in a row: a steeper roll-off, so filtered noise
/// loses its hiss instead of keeping it at 6 dB an octave.
#[derive(Default)]
struct Pole2 {
    a: OnePole,
    b: OnePole,
}

impl Pole2 {
    fn low(&mut self, x: f32, cutoff: f32, rate: f32) -> f32 {
        let y = self.a.low(x, cutoff, rate);
        self.b.low(y, cutoff, rate)
    }
}

/// Band-limited-ish sum of harmonics with a 1/k^power rolloff.
fn harmonics(phase: f32, count: u32, power: f32) -> f32 {
    (1..=count)
        .map(|k| (phase * k as f32).sin() / (k as f32).powf(power))
        .sum()
}

/// A drum hit: pitched thump with a short noise attack.
fn drum(buf: &mut Buf, at: f32, amp: f32, base: f32, seed: u32) {
    let rate = buf.rate;
    let mut phase = 0.0f32;
    let mut lp = OnePole::default();
    buf.at(
        at,
        |t, rng| {
            let f = base * (1.0 + 1.2 * decay(t, 0.05));
            phase += TAU * f / rate;
            let body = phase.sin() * decay(t, 0.22);
            let skin = lp.low(rng.noise(), 500.0, rate) * decay(t, 0.03) * 1.5;
            amp * (body + skin)
        },
        0.9,
        seed,
    );
}

/// Karplus-Strong plucked string.
fn pluck(buf: &mut Buf, at: f32, freq: f32, amp: f32, damping: f32, seed: u32) {
    let len = (buf.rate / freq).round().max(2.0) as usize;
    let mut rng = Rng(seed);
    let mut line: Vec<f32> = (0..len).map(|_| rng.noise()).collect();
    let mut i = 0;
    let dur = 1.2;
    buf.at(
        at,
        |_, _| {
            let next = line[(i + 1) % len];
            let v = line[i];
            line[i] = damping * 0.5 * (v + next);
            i = (i + 1) % len;
            v * amp
        },
        dur,
        seed,
    );
}

/// A brassy horn note with a little vibrato and breath.
fn horn(buf: &mut Buf, at: f32, freq: f32, dur: f32, amp: f32, seed: u32) {
    let rate = buf.rate;
    let mut phase = 0.0f32;
    let mut lp = OnePole::default();
    buf.at(
        at,
        |t, rng| {
            // Notes start a touch flat and slide up, like a real lip.
            let slide = 1.0 - 0.04 * decay(t, 0.05);
            let vib = 1.0 + 0.006 * (TAU * 5.2 * t).sin() * (t * 4.0).min(1.0);
            phase += TAU * freq * slide * vib / rate;
            let tone = harmonics(phase, 9, 1.1);
            let breath = lp.low(rng.noise(), 2500.0, rate) * 0.12 * decay(t, 0.15);
            amp * (tone * 0.55 + breath) * adsr(t, 0.07, 0.25, dur)
        },
        dur,
        seed,
    );
}

// ---------------------------------------------------------------- effects

fn arrow_release() -> Buf {
    let mut b = Buf::new(0.55, SFX_RATE);
    let rate = b.rate;
    // The string: a quick, slightly sagging twang.
    let mut phase = 0.0;
    b.at(
        0.0,
        |t, _| {
            let f = 196.0 * (1.0 + 0.06 * decay(t, 0.03));
            phase += TAU * f / rate;
            (phase.sin() + 0.5 * (phase * 2.0).sin() + 0.25 * (phase * 3.0).sin()) * decay(t, 0.09)
        },
        0.5,
        1,
    );
    // The arrows rushing away: noise sweeping upward.
    let (mut hp, mut lp) = (OnePole::default(), OnePole::default());
    b.at(
        0.01,
        |t, rng| {
            let cutoff = 700.0 + 5000.0 * (t / 0.3).min(1.0);
            let n = lp.low(hp.high(rng.noise(), 600.0, rate), cutoff, rate);
            n * 0.7 * (t / 0.02).min(1.0) * decay(t, 0.09)
        },
        0.4,
        2,
    );
    b
}

fn arrow_impact() -> Buf {
    let mut b = Buf::new(0.35, SFX_RATE);
    let rate = b.rate;
    // A volley lands as a ragged handful of thunks.
    for (i, (at, amp, f)) in [
        (0.0, 1.0, 150.0),
        (0.05, 0.7, 120.0),
        (0.11, 0.8, 175.0),
        (0.16, 0.5, 130.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut phase = 0.0;
        let mut lp = OnePole::default();
        b.at(
            at,
            |t, rng| {
                phase += TAU * f * (1.0 + decay(t, 0.02)) / rate;
                let thump = phase.sin() * decay(t, 0.045);
                let crack = lp.low(rng.noise(), 2200.0, rate) * decay(t, 0.012) * 0.9;
                amp * (thump + crack)
            },
            0.2,
            10 + i as u32,
        );
    }
    b
}

fn clash() -> Buf {
    let mut b = Buf::new(0.7, SFX_RATE);
    let rate = b.rate;
    let mut hp = OnePole::default();
    // The strike.
    b.at(
        0.0,
        |t, rng| hp.high(rng.noise(), 1500.0, rate) * decay(t, 0.012) * 1.2,
        0.1,
        3,
    );
    // The ring: inharmonic partials of struck steel.
    let partials = [
        (1.0, 0.7, 0.30),
        (2.76, 0.55, 0.20),
        (5.4, 0.35, 0.12),
        (8.93, 0.2, 0.07),
    ];
    for (ratio, amp, tau) in partials {
        let f = 640.0 * ratio;
        b.at(
            0.0,
            |t, _| (TAU * f * t).sin() * amp * decay(t, tau),
            0.7,
            4,
        );
    }
    // Body behind it.
    b.at(
        0.0,
        |t, _| (TAU * 95.0 * t).sin() * decay(t, 0.05) * 0.5,
        0.2,
        5,
    );
    b
}

fn charge() -> Buf {
    let mut b = Buf::new(1.4, SFX_RATE);
    // A drum roll that speeds up and swells into a final heavy beat.
    for (i, (at, amp)) in [
        (0.0, 0.55),
        (0.30, 0.6),
        (0.52, 0.65),
        (0.68, 0.7),
        (0.80, 0.75),
        (0.90, 0.8),
        (0.98, 0.9),
        (1.06, 1.0),
    ]
    .into_iter()
    .enumerate()
    {
        drum(&mut b, at, amp, 82.0, 20 + i as u32);
    }
    b
}

fn waver() -> Buf {
    let mut b = Buf::new(1.0, SFX_RATE);
    let dur = 1.0;
    for (f, amp) in [(98.0, 1.0), (103.5, 0.9), (147.0, 0.35)] {
        b.at(
            0.0,
            |t, _| {
                let sink = 1.0 - 0.06 * t;
                let tremolo = 0.65 + 0.35 * (TAU * 5.5 * t).sin();
                (TAU * f * sink * t).sin() * amp * tremolo * adsr(t, 0.2, 0.5, dur)
            },
            dur,
            6,
        );
    }
    b
}

fn rout_horn() -> Buf {
    let mut b = Buf::new(1.25, SFX_RATE);
    // Short, then long: the signal for a broken line.
    horn(&mut b, 0.0, 233.1, 0.34, 1.0, 7);
    horn(&mut b, 0.42, 233.1, 0.8, 1.0, 8);
    b
}

fn shatter() -> Buf {
    let mut b = Buf::new(1.6, SFX_RATE);
    let rate = b.rate;
    let mut lp = OnePole::default();
    // A crash of noise whose brightness falls away.
    b.at(
        0.0,
        |t, rng| {
            let cutoff = 300.0 + 6500.0 * decay(t, 0.25);
            lp.low(rng.noise(), cutoff, rate) * decay(t, 0.45) * 1.3
        },
        1.5,
        9,
    );
    // Falling bits of metal.
    let mut rng = Rng(77);
    for i in 0..9 {
        let f = 450.0 + (rng.noise() * 0.5 + 0.5) * 2800.0;
        let start = 0.02 + i as f32 * 0.055 + (rng.noise() * 0.5 + 0.5) * 0.03;
        let tau = 0.12 + (rng.noise() * 0.5 + 0.5) * 0.35;
        b.at(
            start,
            |t, _| (TAU * f * t).sin() * decay(t, tau) * 0.22,
            1.0,
            30 + i,
        );
    }
    // The boom under it.
    b.at(
        0.0,
        |t, _| (TAU * 52.0 * (1.0 + 0.5 * decay(t, 0.1)) * t).sin() * decay(t, 0.3) * 1.1,
        0.9,
        11,
    );
    b
}

// ---------------------------------------------------------- siege effects
//
// Building-attack sounds. They share a few small parts: a dull thud, a woody
// knock, a sharp crack, a stick-slip creak and a sprinkle of debris.

/// A dull, heavy blow: a sine that drops in pitch (`freq` is where it
/// settles) over a low-passed knock of noise.
fn thud(buf: &mut Buf, at: f32, amp: f32, freq: f32, tau: f32, seed: u32) {
    let rate = buf.rate;
    let mut phase = 0.0f32;
    let mut lp = Pole2::default();
    buf.at(
        at,
        |t, rng| {
            phase += TAU * freq * (1.0 + 0.9 * decay(t, 0.05)) / rate;
            let body = phase.sin() * decay(t, tau);
            let knock = lp.low(rng.noise(), freq * 6.0, rate) * decay(t, 0.03) * 0.8;
            amp * (body + knock)
        },
        tau * 6.0,
        seed,
    );
}

/// A woody "tok": two inharmonic partials of struck timber and a breath of
/// noise.
fn knock(buf: &mut Buf, at: f32, freq: f32, amp: f32, seed: u32) {
    let rate = buf.rate;
    let mut lp = OnePole::default();
    buf.at(
        at,
        |t, rng| {
            let tone = (TAU * freq * t).sin() * decay(t, 0.045)
                + 0.5 * (TAU * freq * 1.58 * t).sin() * decay(t, 0.03);
            let air = lp.low(rng.noise(), freq * 3.0, rate) * decay(t, 0.012);
            amp * (tone * 0.8 + air * 0.6)
        },
        0.3,
        seed,
    );
}

/// A sharp crack of wood or stone giving way: a burst of bright noise.
fn crack(buf: &mut Buf, at: f32, amp: f32, seed: u32) {
    let rate = buf.rate;
    let mut hp = OnePole::default();
    let mut lp = Pole2::default();
    buf.at(
        at,
        |t, rng| {
            let n = lp.low(hp.high(rng.noise(), 1500.0, rate), 6500.0, rate);
            amp * n * decay(t, 0.008) * 1.6
        },
        0.06,
        seed,
    );
}

/// One small hard fragment landing: a click of noise over a very short ring.
fn tick(buf: &mut Buf, at: f32, freq: f32, amp: f32, tau: f32, seed: u32) {
    let rate = buf.rate;
    let mut lp = OnePole::default();
    buf.at(
        at,
        |t, rng| {
            let ring = (TAU * freq * t).sin() * decay(t, tau);
            let click = lp.low(rng.noise(), freq * 2.0, rate) * decay(t, tau * 0.5);
            amp * (ring * 0.5 + click)
        },
        tau * 8.0,
        seed,
    );
}

/// `count` fragments falling between `from` and `to` seconds, thick at first
/// and thinning out, their pitch picked between `band.0` and `band.1` Hz.
fn debris(buf: &mut Buf, from: f32, to: f32, count: u32, amp: f32, band: (f32, f32), seed: u32) {
    let mut rng = Rng(seed);
    let mut unit = move || rng.noise() * 0.5 + 0.5;
    for i in 0..count {
        let u = unit();
        let when = from + (to - from) * u.powf(1.7);
        let freq = band.0 * (band.1 / band.0).powf(unit());
        let a = amp * (0.35 + 0.65 * unit()) * (1.0 - 0.6 * u);
        let tau = 0.004 + 0.01 * unit();
        tick(buf, when, freq, a, tau, seed + 1 + i);
    }
}

/// Timber groaning under load: a rough tone sliding from `f0` to `f1` Hz,
/// scraping in stick-slip pulses (`pulses` per second) with a wandering pitch.
fn creak(buf: &mut Buf, at: f32, dur: f32, (f0, f1): (f32, f32), amp: f32, pulses: f32, seed: u32) {
    let rate = buf.rate;
    let mut phase = 0.0f32;
    let mut wander = OnePole::default();
    let mut lp = Pole2::default();
    buf.at(
        at,
        |t, rng| {
            let j = wander.low(rng.noise(), 25.0, rate) * 6.0;
            let f = (f0 + (f1 - f0) * (t / dur)) * (1.0 + j);
            phase += TAU * f / rate;
            let slip = 1.0 - ((pulses * t + 0.3 * j) % 1.0).abs();
            let scrape = 0.3 + 0.7 * slip * slip;
            let tone = lp.low(harmonics(phase, 12, 0.9) + rng.noise() * 0.4, f * 9.0, rate);
            amp * tone * scrape * adsr(t, dur * 0.15, dur * 0.3, dur)
        },
        dur,
        seed,
    );
}

/// A blade or club striking a wall: short, dry and neutral, so several
/// soldiers' blows a second can overlap without smearing.
fn wall_hit() -> Buf {
    let mut b = Buf::new(0.4, SFX_RATE);
    // The dull body of the blow and the woody note of the planks.
    thud(&mut b, 0.0, 1.0, 95.0, 0.07, 40);
    knock(&mut b, 0.0, 330.0, 0.5, 41);
    // The splintery crack, and a few splinters flicking off.
    crack(&mut b, 0.0, 0.8, 42);
    for (i, (at, amp)) in [(0.014, 0.5), (0.031, 0.35), (0.052, 0.4), (0.078, 0.22)]
        .into_iter()
        .enumerate()
    {
        tick(
            &mut b,
            at,
            1800.0 + 450.0 * i as f32,
            amp,
            0.004,
            43 + i as u32,
        );
    }
    // A little grit falling.
    debris(&mut b, 0.06, 0.27, 14, 0.25, (1200.0, 3200.0), 60);
    b
}

/// A boulder or siege shot slamming into a building.
fn siege_impact() -> Buf {
    let mut b = Buf::new(0.9, SFX_RATE);
    let rate = b.rate;
    // The deep boom, and the structure answering a moment later.
    thud(&mut b, 0.0, 1.3, 48.0, 0.2, 70);
    thud(&mut b, 0.0, 0.7, 85.0, 0.09, 71);
    thud(&mut b, 0.14, 0.35, 62.0, 0.12, 72);
    // The slam of the shot itself.
    let mut slam = Pole2::default();
    b.at(
        0.0,
        |t, rng| slam.low(rng.noise(), 1500.0, rate) * decay(t, 0.03) * 1.1,
        0.15,
        73,
    );
    // The crunch: timber snapping and stone breaking, sputtering.
    let mut hp = OnePole::default();
    let mut lp = Pole2::default();
    let (mut gate, mut held) = (1.0f32, 0u32);
    b.at(
        0.0,
        |t, rng| {
            if held == 0 {
                gate = if rng.noise() > 0.1 { 1.0 } else { 0.15 };
                held = 180;
            }
            held -= 1;
            let n = lp.low(hp.high(rng.noise(), 300.0, rate), 3200.0, rate);
            n * gate * decay(t, 0.1) * 0.55
        },
        0.4,
        74,
    );
    for (i, at) in [0.012, 0.045, 0.08, 0.13].into_iter().enumerate() {
        crack(&mut b, at, 0.7 - 0.12 * i as f32, 75 + i as u32);
    }
    for (i, (at, f, amp)) in [(0.03, 220.0, 0.5), (0.09, 150.0, 0.4), (0.17, 280.0, 0.3)]
        .into_iter()
        .enumerate()
    {
        knock(&mut b, at, f, amp, 80 + i as u32);
    }
    debris(&mut b, 0.01, 0.38, 22, 0.5, (350.0, 2500.0), 100);
    // Rubble trickling down after.
    debris(&mut b, 0.25, 0.8, 10, 0.4, (180.0, 500.0), 200);
    debris(&mut b, 0.3, 0.88, 40, 0.26, (500.0, 3200.0), 300);
    b
}

/// A catapult or siege engine firing: the creak of the frame, the heavy
/// whump of the arm and the whoosh of the shot rushing away.
fn siege_launch() -> Buf {
    let mut b = Buf::new(0.7, SFX_RATE);
    let rate = b.rate;
    // The frame groans as the arm is loosed.
    creak(&mut b, 0.0, 0.3, (170.0, 260.0), 0.5, 24.0, 120);
    // The whump of the arm, and its clunk against the stop.
    thud(&mut b, 0.27, 1.0, 68.0, 0.13, 121);
    knock(&mut b, 0.29, 190.0, 0.55, 122);
    knock(&mut b, 0.29, 120.0, 0.4, 123);
    // The whoosh: noise whose band sweeps upwards as the shot flies off.
    let mut hp = OnePole::default();
    let mut lp = Pole2::default();
    b.at(
        0.27,
        |t, rng| {
            let c = 500.0 + 2000.0 * (t / 0.43);
            let n = lp.low(hp.high(rng.noise(), c, rate), c * 2.5, rate);
            n * adsr(t, 0.08, 0.25, 0.43) * 1.5
        },
        0.43,
        124,
    );
    b
}

/// Thunder: the sky tearing open (a few sharp cracks and a hiss that darkens
/// as it goes), a deep thump, then a long roll that swells and dies a few
/// times as it travels, with a mid-range growl in it so small speakers have
/// something to play.
fn thunder() -> Buf {
    let mut b = Buf::new(2.45, SFX_RATE);
    let rate = b.rate;
    for (i, (at, amp)) in [(0.0, 1.0), (0.03, 0.8), (0.08, 0.6), (0.15, 0.45)]
        .into_iter()
        .enumerate()
    {
        crack(&mut b, at, amp, 500 + i as u32);
    }
    let mut hp = OnePole::default();
    let mut tear = Pole2::default();
    b.at(
        0.0,
        |t, rng| {
            let n = tear.low(
                hp.high(rng.noise(), 300.0, rate),
                2600.0 - 1600.0 * t.min(1.0),
                rate,
            );
            n * decay(t, 0.14) * 0.9
        },
        0.8,
        510,
    );
    thud(&mut b, 0.03, 0.8, 70.0, 0.22, 530);
    thud(&mut b, 0.05, 0.5, 110.0, 0.15, 531);
    let mut rumble = Pole2::default();
    let mut growl_hi = Pole2::default();
    let mut growl_lo = Pole2::default();
    b.at(
        0.1,
        |t, rng| {
            let n = rng.noise();
            let swell = |at: f32, width: f32, peak: f32| peak * (-((t - at) / width).powi(2)).exp();
            let env = (swell(0.05, 0.2, 1.0)
                + swell(0.5, 0.17, 0.75)
                + swell(0.95, 0.2, 0.6)
                + swell(1.45, 0.22, 0.45)
                + 0.12 * decay(t, 1.2))
                * ((2.3 - t) / 0.6).clamp(0.0, 1.0);
            // Two slow, unrelated wobbles, so the roll never settles into a beat.
            let wob = 1.0 + 0.15 * (TAU * 3.1 * t).sin() + 0.12 * (TAU * 4.7 * t + 2.0).sin();
            let low = rumble.low(n, 90.0 + 60.0 * decay(t, 1.0), rate) * 5.0;
            let growl = (growl_hi.low(n, 700.0, rate) - growl_lo.low(n, 160.0, rate)) * 2.2;
            (low + growl * decay(t, 0.9)) * env * wob
        },
        2.3,
        520,
    );
    b
}

/// A building coming down.
fn collapse() -> Buf {
    let mut b = Buf::new(2.6, SFX_RATE);
    let rate = b.rate;
    // The groan: beams complaining deep down, and a higher one over it, with
    // cracks as they give.
    creak(&mut b, 0.0, 0.9, (62.0, 48.0), 0.9, 11.0, 400);
    creak(&mut b, 0.15, 0.7, (150.0, 105.0), 0.45, 17.0, 401);
    for (i, (at, amp)) in [
        (0.06, 0.5),
        (0.17, 0.6),
        (0.24, 0.5),
        (0.33, 0.7),
        (0.42, 1.2),
    ]
    .into_iter()
    .enumerate()
    {
        crack(&mut b, at, amp, 410 + i as u32);
    }
    knock(&mut b, 0.42, 160.0, 0.5, 420);
    // The roar of masonry and timber: a rumble and a darker and darker rush,
    // swelling fast, then dying away.
    let mut rumble = Pole2::default();
    let mut rush = Pole2::default();
    let mut hp = OnePole::default();
    b.at(
        0.35,
        |t, rng| {
            let n = rng.noise();
            let swell = (t / 0.35).min(1.0);
            let env = swell
                * swell
                * (3.0 - 2.0 * swell)
                * decay((t - 0.2).max(0.0), 0.75)
                * ((2.2 - t) / 0.8).clamp(0.0, 1.0);
            let wob = 1.0 + 0.25 * (TAU * 7.3 * t).sin() + 0.15 * (TAU * 11.9 * t + 1.0).sin();
            let low = rumble.low(n, 110.0, rate) * 4.0;
            let top = rush.low(
                hp.high(n, 120.0, rate),
                250.0 + 1800.0 * decay(t, 0.8),
                rate,
            );
            (low * 0.8 + top * 1.1) * env * wob
        },
        2.2,
        430,
    );
    // Heavy thuds inside it, with timbers knocking, each smaller than the last.
    for (i, (at, amp, f, tau)) in [
        (0.50, 1.0, 52.0, 0.22),
        (0.82, 0.85, 44.0, 0.25),
        (1.12, 0.95, 58.0, 0.20),
        (1.48, 0.60, 48.0, 0.20),
        (1.85, 0.45, 55.0, 0.18),
        (2.20, 0.30, 50.0, 0.15),
    ]
    .into_iter()
    .enumerate()
    {
        thud(&mut b, at, amp, f, tau, 440 + i as u32);
    }
    for (i, (at, f)) in [(0.95, 140.0), (1.25, 210.0), (1.65, 170.0), (2.0, 250.0)]
        .into_iter()
        .enumerate()
    {
        knock(&mut b, at, f, 0.35, 450 + i as u32);
    }
    // Settling: stones and splinters trickling down.
    debris(&mut b, 0.7, 2.0, 16, 0.5, (150.0, 600.0), 500);
    debris(&mut b, 1.0, 2.55, 120, 0.3, (500.0, 3500.0), 600);
    b
}

fn rally() -> Buf {
    let mut b = Buf::new(1.15, SFX_RATE);
    // Two rising notes: a fifth up, held.
    horn(&mut b, 0.0, 196.0, 0.36, 0.9, 12);
    horn(&mut b, 0.4, 293.7, 0.75, 1.0, 13);
    b
}

fn ui_select() -> Buf {
    let mut b = Buf::new(0.1, SFX_RATE);
    for (f, amp) in [(1180.0, 1.0), (1770.0, 0.4)] {
        b.at(
            0.0,
            |t, _| (TAU * f * t).sin() * amp * decay(t, 0.025),
            0.1,
            14,
        );
    }
    b
}

fn ui_move() -> Buf {
    let mut b = Buf::new(0.18, SFX_RATE);
    for (at, f) in [(0.0, 660.0), (0.075, 880.0)] {
        b.at(
            at,
            |t, _| {
                let tone = (TAU * f * t).sin() + 0.3 * (TAU * f * 2.0 * t).sin();
                tone * decay(t, 0.03) * (t / 0.004).min(1.0)
            },
            0.1,
            15,
        );
    }
    b
}

fn ui_attack() -> Buf {
    let mut b = Buf::new(0.24, SFX_RATE);
    let rate = b.rate;
    let mut phase = 0.0;
    b.at(
        0.0,
        |t, _| {
            let f = 220.0 + 330.0 * (t / 0.14).min(1.0);
            phase += TAU * f / rate;
            harmonics(phase, 6, 1.0) * decay(t, 0.1) * 0.7
        },
        0.24,
        16,
    );
    let mut hp = OnePole::default();
    b.at(
        0.0,
        |t, rng| hp.high(rng.noise(), 1800.0, rate) * decay(t, 0.015),
        0.05,
        17,
    );
    b
}

// ------------------------------------------------------------------ music

const BPM: f32 = 100.0;
const BEAT: f32 = 60.0 / BPM;
const BEATS: f32 = 32.0; // eight bars of four
const LOOP_SECONDS: f32 = BEAT * BEATS;

/// Hirajoshi in D, the Clans' scale (Hz).
mod note {
    pub const D2: f32 = 73.42;
    pub const A2: f32 = 110.0;
    pub const EB3: f32 = 155.56;
    pub const D3: f32 = 146.83;
    pub const A3: f32 = 220.0;
    pub const D4: f32 = 293.66;
    pub const EB4: f32 = 311.13;
    pub const G4: f32 = 392.0;
    pub const A4: f32 = 440.0;
    pub const BB4: f32 = 466.16;
}

/// A breathy flute note, shakuhachi-style.
fn flute(b: &mut Buf, beat: f32, beats: f32, freq: f32, amp: f32, seed: u32) {
    let rate = b.rate;
    let dur = beats * BEAT;
    let mut phase = 0.0f32;
    let mut lp = OnePole::default();
    b.at(
        beat * BEAT,
        |t, rng| {
            let vib = 1.0 + 0.005 * (TAU * 5.0 * t).sin() * ((t - 0.25) * 3.0).clamp(0.0, 1.0);
            phase += TAU * freq * vib / rate;
            let tone = phase.sin() + 0.18 * (phase * 2.0).sin() + 0.06 * (phase * 3.0).sin();
            let breath = lp.low(rng.noise(), freq * 3.0, rate) * 0.35;
            amp * (tone + breath) * adsr(t, 0.14, 0.35, dur)
        },
        dur,
        seed,
    );
}

/// A sustained low pad: a few harmonics with a slow swell.
fn pad(b: &mut Buf, beat: f32, beats: f32, freq: f32, amp: f32) {
    let rate = b.rate;
    let dur = beats * BEAT;
    let mut phase = 0.0f32;
    b.at(
        beat * BEAT,
        |t, _| {
            phase += TAU * freq / rate;
            harmonics(phase, 5, 1.4) * amp * adsr(t, dur * 0.4, dur * 0.4, dur)
        },
        dur,
        0,
    );
}

fn music_calm() -> Buf {
    let mut b = Buf::looping(LOOP_SECONDS, MUSIC_RATE);
    // Drone: root and fifth under everything, swelling over four bars.
    for bar in [0.0, 16.0] {
        pad(&mut b, bar, 16.0, note::D2, 0.45);
        pad(&mut b, bar, 16.0, note::A2, 0.30);
        pad(&mut b, bar, 16.0, note::D3, 0.12);
    }
    // The melody, in beats from the start of the loop.
    let melody = [
        (0.0, 3.0, note::D4),
        (4.0, 2.0, note::G4),
        (6.0, 2.0, note::A4),
        (8.0, 3.0, note::BB4),
        (11.0, 1.0, note::A4),
        (12.0, 4.0, note::G4),
        (16.0, 3.0, note::A4),
        (19.0, 1.0, note::G4),
        (20.0, 2.0, note::EB4),
        (22.0, 2.0, note::D4),
        (24.0, 6.0, note::D4),
    ];
    for (i, (beat, len, f)) in melody.into_iter().enumerate() {
        flute(&mut b, beat, len, f, 0.55, 100 + i as u32);
    }
    // A string answering at the end of each phrase.
    for (i, beat) in [3.5, 11.5, 19.5, 27.0].into_iter().enumerate() {
        pluck(&mut b, beat * BEAT, note::D3, 0.35, 0.997, 200 + i as u32);
    }
    // Sparse drum: the downbeat of every second bar.
    for (i, beat) in [0.0, 8.0, 16.0, 24.0].into_iter().enumerate() {
        drum(&mut b, beat * BEAT, 0.5, 65.0, 300 + i as u32);
    }
    b
}

fn music_battle() -> Buf {
    let mut b = Buf::looping(LOOP_SECONDS, MUSIC_RATE);
    // Drums: a pulse on every beat, hard on the bar, with fills at the end
    // of each four-bar phrase.
    for beat in 0..32 {
        let accent = if beat % 4 == 0 { 1.0 } else { 0.55 };
        drum(&mut b, beat as f32 * BEAT, accent * 0.9, 72.0, 400 + beat);
    }
    for (i, start) in [14.0f32, 30.0].into_iter().enumerate() {
        for k in 0..4 {
            drum(
                &mut b,
                (start + 0.25 * k as f32 * 2.0) * BEAT,
                0.6 + 0.1 * k as f32,
                95.0,
                500 + (i * 4 + k) as u32,
            );
        }
        drum(&mut b, (start + 1.5) * BEAT, 0.9, 80.0, 520 + i as u32);
    }
    // A driving plucked ostinato, in eighth notes.
    let pattern = [
        note::D3,
        0.0,
        note::D3,
        note::EB3,
        note::D3,
        0.0,
        note::A2,
        note::EB3,
    ];
    for step in 0..64 {
        let f = pattern[step % 8];
        if f > 0.0 {
            pluck(
                &mut b,
                step as f32 * 0.5 * BEAT,
                f,
                0.5,
                0.995,
                600 + step as u32,
            );
        }
    }
    // Brass pad holding the root and fifth, swelling every four bars.
    for bar in [0.0, 16.0] {
        pad(&mut b, bar, 16.0, note::D3, 0.5);
        pad(&mut b, bar, 16.0, note::A3, 0.3);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_present_in_order() {
        let clips = all();
        assert_eq!(clips.len(), SoundId::COUNT);
        for (clip, id) in clips.iter().zip(SoundId::ALL) {
            assert_eq!(clip.id, id);
        }
    }

    #[test]
    fn sounds_are_audible_not_clipped_and_sized_sensibly() {
        for c in all() {
            let peak = c.pcm.iter().map(|s| s.unsigned_abs()).max().unwrap();
            assert!(peak > 8000, "{:?} is too quiet", c.id);
            assert!(peak < 32767, "{:?} clips", c.id);
            let seconds = c.pcm.len() as f32 / c.rate as f32;
            if c.id.is_music() {
                assert!(c.looped);
                assert!((seconds - LOOP_SECONDS).abs() < 0.01, "{:?}", c.id);
            } else if c.id.is_bed() {
                assert!(c.looped);
                assert!((seconds - BED_SECONDS).abs() < 0.01, "{:?}", c.id);
            } else {
                assert!(!c.looped);
                assert!((0.05..3.0).contains(&seconds), "{:?} is {seconds}s", c.id);
            }
        }
    }

    #[test]
    fn music_layers_are_the_same_length() {
        let clips = all();
        assert_eq!(
            clips[SoundId::MusicCalm as usize].pcm.len(),
            clips[SoundId::MusicBattle as usize].pcm.len()
        );
    }

    #[test]
    fn loops_join_without_a_click() {
        for c in all().into_iter().filter(|c| c.looped) {
            let last = i32::from(*c.pcm.last().unwrap());
            let first = i32::from(c.pcm[0]);
            // Neighbouring samples in the middle of the music differ by this much.
            let typical = c
                .pcm
                .windows(2)
                .map(|w| (i32::from(w[1]) - i32::from(w[0])).abs())
                .sum::<i32>()
                / c.pcm.len() as i32;
            assert!(
                (first - last).abs() < typical * 12 + 1500,
                "{:?} jumps {} at the loop point (typical step {typical})",
                c.id,
                (first - last).abs()
            );
        }
    }
}
