//! Turns what happens in the match into sound.
//!
//! The simulation knows nothing about audio. After each tick this module
//! reads what the simulation says the tick did (`World::events`) and plays a
//! sound for each: volleys leaving and landing, blows in melee, a regiment
//! wavering, breaking, shattering or rallying. Regiments also shout in their
//! faction's voice (the Ember Clans' war cries, the Brood's shrieks, the Shard
//! Choir's sung chords, the Carrion Concord's moans) as they charge, break
//! and rally, and when their champion wins the duel between the lines.
//! Sounds are placed relative to the listener (the bird's-eye camera's
//! ground point, or the eye when riding with a regiment): quieter with
//! distance (less so when zoomed out), duller with distance (the highs go
//! first) and panned left or right.
//!
//! Under the one-shot sounds run four ambient beds: the roar of the fighting
//! (louder and nearer the more regiments are engaged, panned towards them,
//! muffled when they are far), and wind, water and woods taken from the
//! ground around the camera, so a river on one side or a forest on the other
//! can be heard.

use crate::camera::Listener;
use crate::from_fx_vec;
use gc_audio::{Audio, Bank, Cry, SoundId};
use gc_platform::logln;
use gc_sim::champions::Outcome;
use gc_sim::events::{Event, VolleyId};
use gc_sim::{
    MoraleState, PlayerId, RegimentHandle, Shooter, StructureHandle, TICKS_PER_SECOND, UnitId,
    World,
};
use glam::Vec2;
use std::time::{Duration, Instant};

/// The sound bank written by `cargo run -p gc_sfx`.
static BANK: &[u8] = include_bytes!("../../../assets/audio/sounds.gcsnd");

/// How far the camera "hears" at the nearest zoom, and extra per unit of
/// camera distance, in tiles.
const HEAR_BASE: f32 = 50.0;
const HEAR_PER_ZOOM: f32 = 0.8;
/// Engaged regiments near the camera that bring the battle music to full.
const FULL_BATTLE: f32 = 4.0;
/// Ticks between blows heard from one regiment.
const BLOW_COOLDOWN: u32 = 3;
/// Every this many ticks an engaged regiment is heard even without a death.
const AMBIENT_EVERY: u32 = 10;

/// How far (as a multiple of the hearing range) the roar of a fight
/// carries: further than any one blow.
const ROAR_REACH: f32 = 1.6;
/// How quickly the roar fills out as regiments join in (per unit of summed
/// nearness), and the most it reaches: the one-shot sounds stay on top.
const ROAR_FILL: f32 = 0.9;
const ROAR_MAX: f32 = 0.9;
/// Even a roar from right beside the camera is a little dulled; a far one is
/// dull up to this.
const ROAR_DULL_MIN: f32 = 0.15;

/// The ground is listened to every this many ticks.
const LAND_EVERY: u32 = 10;
/// The ground around the camera is sampled on a grid of this many points
/// along a side, out to [`LAND_REACH`] of the hearing range (at most
/// [`LAND_MAX`] tiles): fine enough that a river a few tiles wide is not
/// stepped over.
const LAND_GRID: i32 = 25;
const LAND_REACH: f32 = 0.35;
const LAND_MAX: f32 = 45.0;
/// How much water or woods in earshot (the weighted share of the ground
/// round the camera) fills the bed: a river across part of the view must
/// be heard, so a small share counts for a lot.
const WATER_FILL: f32 = 5.0;
const WOODS_FILL: f32 = 1.6;
/// Wind: a breeze everywhere, more over open ground, more still on a rise
/// (the camera this many tiles above the ground round it is fully exposed).
const WIND_CALM: f32 = 0.2;
const WIND_OPEN: f32 = 0.35;
const WIND_EXPOSED: f32 = 0.3;
const WIND_RISE: f32 = 6.0;
/// The beds fade with zoom: full close to the ground, a hush at the far end.
const BED_ZOOM_FAR: f32 = 300.0;
const BED_ZOOM_FLOOR: f32 = 0.3;

/// Battle cries: how long one is counted as sounding (they run 1 to 1.6
/// s), and how many may sound at once, so a big battle is a roar and not a
/// wall of noise.
const CRY_TICKS: u32 = 32;
const MAX_CRIES: usize = 4;
/// Ticks before the same regiment is heard to cry the same cry again, by
/// [`Cry`]: a charge, a rout, a rally (a champion's victory is no
/// regiment's, and is cheered once a duel).
const CRY_COOLDOWN: [u32; 4] = [
    10 * TICKS_PER_SECOND,
    8 * TICKS_PER_SECOND,
    8 * TICKS_PER_SECOND,
    0,
];
/// A cry carries this much further than the hearing range's one.
const CRY_REACH: f32 = 1.3;
/// A regiment on the march starts its charge cry when it is this close
/// (tiles, centre to centre) to an enemy it is heading for: about a
/// second before the blocks meet.
const CHARGE_LEAD: f32 = 7.0;
/// How far off its heading (cosine) an enemy may be to count as headed for.
const CHARGE_AHEAD: f32 = 0.77;
/// Regiments are checked for a charge every this many ticks.
const CHARGE_SCAN_EVERY: u32 = 2;

// Who is cut off first when all 32 voices are busy.
const PRI_ARROWS: u8 = 70;
const PRI_BLOWS: u8 = 100;
const PRI_CHARGE: u8 = 170;
const PRI_CRY: u8 = 200;
const PRI_MORALE: u8 = 220;
const PRI_UI: u8 = 255;
/// How loud the nearest thunder is, of full, with the volume knob at 1.
const THUNDER_GAIN: f32 = 0.7;

/// How long to wait before the first try to reopen a lost sound output, and
/// the most between tries. The wait doubles after each failure, because
/// probing for a device blocks the frame loop for a moment.
const REOPEN_FIRST: Duration = Duration::from_secs(3);
const REOPEN_MAX: Duration = Duration::from_secs(60);

/// Short clicks for the interface.
#[derive(Clone, Copy, Debug)]
pub enum UiSound {
    Select,
    Move,
    Attack,
}

/// What a regiment type sounds like: its faction and whether it charges
/// (rather than only shooting).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Voice {
    pub faction: gc_ai::Faction,
    pub charges: bool,
}

/// The battle cries still sounding, by the tick each started on: at most
/// [`MAX_CRIES`] at once.
#[derive(Default)]
struct CryCap {
    starts: Vec<u32>,
}

impl CryCap {
    /// Whether [`MAX_CRIES`] are still sounding at `tick`.
    fn full(&mut self, tick: u32) -> bool {
        self.starts.retain(|&start| tick < start + CRY_TICKS);
        self.starts.len() >= MAX_CRIES
    }

    /// Room for one more cry at `tick`? If so, it is counted.
    fn admit(&mut self, tick: u32) -> bool {
        if self.full(tick) {
            return false;
        }
        self.starts.push(tick);
        true
    }
}

/// What one ambient bed should be doing: how loud, where, how muffled.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct BedMix {
    level: f32,
    pan: f32,
    dull: f32,
}

impl BedMix {
    fn scaled(self, by: f32) -> BedMix {
        BedMix {
            level: self.level * by,
            ..self
        }
    }
}

/// The ambient beds, in [`gc_audio::SoundId::BEDS`] order.
const WIND: usize = 0;
const WATER: usize = 1;
const WOODS: usize = 2;
const BATTLE: usize = 3;

/// The fights in earshot, summed into the battle bed.
#[derive(Default)]
struct Roar {
    /// Sum of how near each fight is (0 at the edge of earshot, 1 on top).
    near: f32,
    /// The same, weighted by how far to the right each is.
    side: f32,
    /// The same, weighted by how far away each is.
    far: f32,
}

impl Roar {
    /// A fight `distance` tiles off, `side` tiles to the right of the
    /// listener (negative: left), heard out to `radius`.
    fn add(&mut self, distance: f32, side: f32, radius: f32) {
        let near = 1.0 - distance / radius;
        if near > 0.0 {
            self.near += near;
            self.side += near * side;
            self.far += near * (1.0 - near);
        }
    }

    fn mix(&self, radius: f32) -> BedMix {
        if self.near <= 0.0 {
            return BedMix::default();
        }
        BedMix {
            level: (1.0 - (-ROAR_FILL * self.near).exp()).min(ROAR_MAX),
            pan: (self.side / self.near / (radius * 0.5)).clamp(-1.0, 1.0),
            dull: (ROAR_DULL_MIN + (1.0 - ROAR_DULL_MIN) * self.far / self.near).clamp(0.0, 1.0),
        }
    }
}

/// What the ground is at a point, for the beds.
#[derive(Clone, Copy, Debug)]
struct Probe {
    wet: bool,
    wood: bool,
    /// Height above the sea level of the map, in tiles.
    height: f32,
}

/// Wind, water and woods from the ground round `focus`, sampled out to
/// `radius` tiles: each is louder the more of it there is and the nearer it
/// is (weighted by nearness squared), panned towards it. `right` is the
/// camera's right on the ground.
fn land_mix(probe: impl Fn(Vec2) -> Probe, focus: Vec2, right: Vec2, radius: f32) -> [BedMix; 3] {
    let step = 2.0 * radius / (LAND_GRID - 1) as f32;
    let (mut all, mut wet, mut wood, mut wet_side, mut wood_side, mut height) =
        (0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for iy in 0..LAND_GRID {
        for ix in 0..LAND_GRID {
            let offset = Vec2::new(ix as f32, iy as f32) * step - Vec2::splat(radius);
            let near = 1.0 - offset.length() / radius;
            if near <= 0.0 {
                continue;
            }
            let w = near * near;
            let p = probe(focus + offset);
            let side = offset.dot(right);
            all += w;
            height += w * p.height;
            if p.wet {
                wet += w;
                wet_side += w * side;
            }
            if p.wood {
                wood += w;
                wood_side += w * side;
            }
        }
    }
    if all <= 0.0 {
        return [BedMix::default(); 3];
    }
    let place = |share: f32, side: f32, fill: f32| {
        let level = (share / all * fill).min(1.0);
        BedMix {
            level,
            pan: if share > 0.0 {
                (side / share / (radius * 0.5)).clamp(-1.0, 1.0)
            } else {
                0.0
            },
            // What there is less of is further away.
            dull: (1.0 - level) * 0.7,
        }
    };
    let water = place(wet, wet_side, WATER_FILL);
    let woods = place(wood, wood_side, WOODS_FILL);
    let exposure = ((probe(focus).height - height / all) / WIND_RISE).clamp(0.0, 1.0);
    let wind = BedMix {
        level: WIND_CALM + WIND_OPEN * (1.0 - woods.level) + WIND_EXPOSED * exposure,
        pan: 0.0,
        dull: 0.1,
    };
    [wind, water, woods]
}

/// How much the beds are heard at this zoom (`distance` of the camera).
fn zoom_hush(distance: f32) -> f32 {
    (1.15 - distance / BED_ZOOM_FAR).clamp(BED_ZOOM_FLOOR, 1.0)
}

/// The beds as last sent to the mixer, in coarse steps, so a bed that has
/// not changed audibly costs no message.
#[derive(Default)]
struct BedsSent([Option<[i16; 3]>; 4]);

impl BedsSent {
    /// Whether `mix` differs audibly from what bed `bed` was last set to
    /// (and is noted as sent if so).
    fn changed(&mut self, bed: usize, mix: BedMix) -> bool {
        let q = |v: f32, steps: f32| (v.clamp(-1.0, 1.0) * steps).round() as i16;
        let now = [q(mix.level, 50.0), q(mix.pan, 25.0), q(mix.dull, 25.0)];
        if self.0[bed] == Some(now) {
            return false;
        }
        self.0[bed] = Some(now);
        true
    }

    fn forget(&mut self) {
        self.0 = Default::default();
    }
}

#[derive(Clone, Copy)]
struct Seen {
    handle: RegimentHandle,
    /// Tick before which no further blow is played for it.
    quiet_until: u32,
    /// Ticks before which it is not heard to cry each [`Cry`] again.
    cry_quiet: [u32; 4],
}

impl Seen {
    /// Whether this regiment may be heard to cry `kind` at `tick`.
    fn may_cry(&self, kind: Cry, tick: u32) -> bool {
        tick >= self.cry_quiet[kind as usize]
    }

    fn cried(&mut self, kind: Cry, tick: u32) {
        self.cry_quiet[kind as usize] = tick + CRY_COOLDOWN[kind as usize];
    }
}

/// How many arrows a volley in the air carries (`Landed` doesn't say), so
/// the loudness of its landing can follow.
type Shots = (VolleyId, u16);

pub struct Sounds {
    audio: Audio,
    seen: Vec<Option<Seen>>,
    flights: Vec<Shots>,
    focus: Vec2,
    right: Vec2,
    hearing: f32,
    last_intensity: u8,
    muted: bool,
    music_on: bool,
    /// The Master and Music sliders from Settings, 0..=100.
    master: u8,
    music: u8,
    /// The battle cries still sounding.
    cries: CryCap,
    /// The duel (by the tick it was called) whose winner has been cheered.
    duel_cried: Option<u32>,
    /// True while `observe` only records what it sees: no sound plays.
    hushed: bool,
    /// The ambient beds as last sent, and the ground's three (wind, water,
    /// woods) as last heard, kept between the slower ground listens.
    beds: BedsSent,
    land: [BedMix; 3],
    /// The tick the ground was last listened to.
    land_at: Option<u32>,
    /// False while no match is on screen (the menu is up): the beds are off.
    live: bool,
    /// When the output may next be reopened, and how long to wait after
    /// that if it fails again. Kept after a success, so a stream that dies
    /// again at once still waits.
    reopen: Option<(Instant, Duration)>,
}

fn forget_match(
    seen: &mut [Option<Seen>],
    flights: &mut Vec<Shots>,
    cries: &mut CryCap,
    duel_cried: &mut Option<u32>,
) {
    seen.fill(None);
    flights.clear();
    cries.starts.clear();
    *duel_cried = None;
}

/// Whether a regiment at `at` marching along `heading` (a unit vector) is
/// close enough on a foe at `foe`, and facing it, to start its charge cry.
fn closing_on(at: Vec2, heading: Vec2, foe: Vec2) -> bool {
    let offset = foe - at;
    let d2 = offset.length_squared();
    let ahead = offset.dot(heading);
    // Squared, so no square root for the many far away.
    d2 > 1e-4
        && d2 < CHARGE_LEAD * CHARGE_LEAD
        && ahead >= 0.0
        && ahead * ahead >= CHARGE_AHEAD * CHARGE_AHEAD * d2
}

/// A Settings slider (0..=100) as a volume, 0.0..=1.0.
fn slider_gain(slider: u8) -> f32 {
    f32::from(slider.min(100)) / 100.0
}

/// The master and music volumes to send to the mixer: the sliders, with M
/// (mute) silencing everything and N (music off) the music alone.
fn levels(master: u8, music: u8, muted: bool, music_on: bool) -> (f32, f32) {
    (
        if muted { 0.0 } else { slider_gain(master) },
        if music_on { slider_gain(music) } else { 0.0 },
    )
}

/// Open the default output with the embedded sound bank.
fn open_audio() -> Result<Audio, gc_audio::AudioError> {
    Bank::parse(BANK)
        .map_err(gc_audio::AudioError::from)
        .and_then(Audio::start)
}

impl Sounds {
    /// Open the sound card and start the music, at the Master and Music
    /// sliders' settings (0..=100). None (with a note on stderr) if there
    /// is no usable output; the game then runs silently.
    pub fn start(regiment_capacity: usize, master: u8, music: u8) -> Option<Sounds> {
        match open_audio() {
            Ok(audio) => {
                logln!(
                    "audio: {} at {} Hz",
                    audio.device_name(),
                    audio.sample_rate()
                );
                let sounds = Sounds {
                    audio,
                    seen: vec![None; regiment_capacity],
                    flights: Vec::with_capacity(256),
                    focus: Vec2::ZERO,
                    right: Vec2::X,
                    hearing: HEAR_BASE,
                    last_intensity: 0,
                    muted: false,
                    music_on: true,
                    master,
                    music,
                    cries: CryCap::default(),
                    duel_cried: None,
                    hushed: false,
                    beds: BedsSent::default(),
                    land: [BedMix::default(); 3],
                    land_at: None,
                    live: true,
                    reopen: None,
                };
                sounds.apply_state();
                Some(sounds)
            }
            Err(e) => {
                logln!("audio off: {e}");
                None
            }
        }
    }

    /// A new match is starting: forget the last one. A fresh world reissues
    /// the same regiment handles, so a leftover cool-down would mute the new
    /// match's blows, and volleys still in the air will never land.
    pub fn new_match(&mut self) {
        forget_match(
            &mut self.seen,
            &mut self.flights,
            &mut self.cries,
            &mut self.duel_cried,
        );
        self.land = [BedMix::default(); 3];
        self.land_at = None;
        self.silence_beds();
    }

    /// Fade every bed out (and forget it was ever sent, so it comes back).
    fn silence_beds(&mut self) {
        for bed in 0..gc_audio::SoundId::BEDS.len() {
            self.audio.set_bed(bed, 0.0, 0.0, 0.0);
        }
        self.beds.forget();
    }

    /// Set a bed, unless it is already (about) what it was last set to.
    fn set_bed(&mut self, bed: usize, mix: BedMix) {
        if self.beds.changed(bed, mix) {
            self.audio.set_bed(bed, mix.level, mix.pan, mix.dull);
        }
    }

    /// Whether a match is on screen. Called once a frame: the beds are
    /// faded out while the menu is up and come back with the next tick.
    pub fn set_live(&mut self, live: bool) {
        if self.live && !live {
            self.silence_beds();
        }
        self.live = live;
    }

    /// Tell a freshly opened output what the player has set: mute, music
    /// on or off, and how much of the battle layer is playing.
    fn apply_state(&self) {
        let (master, music) = levels(self.master, self.music, self.muted, self.music_on);
        self.audio.set_master_volume(master);
        self.audio.set_music_volume(music);
        self.audio
            .set_music_intensity(f32::from(self.last_intensity) / 20.0);
    }

    /// If the sound card went away (headphones unplugged), open the default
    /// one again, waiting longer after each failed try, keeping the player's
    /// mute and music settings. Call once a frame.
    pub fn reopen_if_lost(&mut self) {
        if !self.audio.stream_lost() {
            return;
        }
        let (at, wait) = self
            .reopen
            .unwrap_or_else(|| (Instant::now(), REOPEN_FIRST));
        if Instant::now() < at {
            return;
        }
        logln!("audio: the output was lost, trying to reopen it");
        match open_audio() {
            Ok(audio) => {
                logln!(
                    "audio: reopened {} at {} Hz",
                    audio.device_name(),
                    audio.sample_rate()
                );
                self.audio = audio;
                // The new output knows none of the beds.
                self.beds.forget();
                self.apply_state();
                // A stream that dies again at once waits too, instead of
                // being rebuilt every frame.
                self.reopen = Some((Instant::now() + REOPEN_FIRST, REOPEN_FIRST));
            }
            Err(e) => {
                logln!("audio: could not reopen it ({e}); trying again in {wait:?}");
                self.reopen = Some((Instant::now() + wait, (wait * 2).min(REOPEN_MAX)));
            }
        }
    }

    /// Call after every simulation tick. `heard` is false while a rejoining
    /// player replays the match: the tick is still remembered, so nothing
    /// bursts out when play catches up, but it makes no noise. `shown` says
    /// whether the player can see a regiment: one they can't gives nothing
    /// away, though its state is still remembered for when it is revealed.
    /// `voice` says what a regiment type sounds like (None for a type it
    /// doesn't know: it makes no cries). `known` says whether the player
    /// has seen a building: an unseen tower shoots and tolls unheard.
    pub fn observe(
        &mut self,
        world: &World,
        listener: Listener,
        heard: bool,
        shown: impl Fn(RegimentHandle) -> bool,
        known: impl Fn(StructureHandle) -> bool,
        voice: impl Fn(UnitId) -> Option<Voice>,
    ) {
        self.focus = listener.position;
        self.right = listener.right;
        self.hearing = HEAR_BASE + listener.distance * HEAR_PER_ZOOM;
        let tick = world.tick_count();

        self.volleys(world, heard, &shown, &known);
        self.regiments_told(world, heard, &shown, &voice);
        self.charges(world, heard, &shown, &voice);
        self.duel(world, heard, &shown, &voice);

        let mut fighting = 0.0f32;
        let mut roar = Roar::default();
        let roar_radius = self.hearing * ROAR_REACH;
        for r in world.regiments.iter() {
            let i = r.handle.index as usize;
            let at = from_fx_vec(r.position);
            self.hushed = !heard || !shown(r.handle);
            let quiet_until = self.seen_mut(r.handle).quiet_until;
            // Even a regiment losing nobody is heard now and then.
            if r.engaged
                && tick >= quiet_until
                && (tick + i as u32 * 7).is_multiple_of(AMBIENT_EVERY)
            {
                self.play_at(SoundId::Clash, at, 0.4, 1.0, PRI_BLOWS);
            }
            if self.hushed {
                continue;
            }
            if r.engaged && self.in_earshot(at, 1.5) {
                fighting += 1.0;
            }
            if r.engaged {
                let offset = at - self.focus;
                roar.add(offset.length(), offset.dot(self.right), roar_radius);
            }
        }
        self.hushed = false;
        self.beds_told(world, heard, listener.distance, roar.mix(roar_radius));

        let intensity = (fighting / FULL_BATTLE).min(1.0);
        let byte = (intensity * 20.0).round() as u8;
        if byte != self.last_intensity {
            self.last_intensity = byte;
            self.audio.set_music_intensity(intensity);
        }
    }

    /// The ambient beds for this tick: the roar of the fights (`roar`, from
    /// the regiments the player can see) and, every [`LAND_EVERY`] ticks,
    /// the wind, water and woods of the ground around the camera. Off while
    /// a rejoining player catches up or no match is on screen.
    fn beds_told(&mut self, world: &World, heard: bool, zoom: f32, roar: BedMix) {
        let tick = world.tick_count();
        let on = heard && self.live;
        // The ground is not scanned while the beds are off.
        if on
            && self
                .land_at
                .is_none_or(|at| tick.wrapping_sub(at) >= LAND_EVERY)
        {
            self.land_at = Some(tick);
            let terrain = world.terrain();
            let radius = (self.hearing * LAND_REACH).min(LAND_MAX);
            self.land = land_mix(
                |p| {
                    let at = crate::fx_vec(p);
                    Probe {
                        wet: terrain.wet(at),
                        wood: terrain.cover(at) == gc_sim::Cover::Wood,
                        height: terrain.height(at) as f32 / gc_sim::terrain::HEIGHT_UNITS as f32,
                    }
                },
                self.focus,
                self.right,
                radius,
            );
        }
        let hush = zoom_hush(zoom);
        let [wind, water, woods] = self.land;
        for (bed, mix) in [
            (WIND, wind.scaled(hush)),
            (WATER, water.scaled(hush)),
            (WOODS, woods.scaled(hush)),
            (BATTLE, roar),
        ] {
            self.set_bed(bed, if on { mix } else { BedMix::default() });
        }
    }

    /// The slot noting `handle`'s cool-down: a new one when a regiment has
    /// taken the slot of another.
    fn seen_mut(&mut self, handle: RegimentHandle) -> &mut Seen {
        let slot = &mut self.seen[handle.index as usize];
        if slot.is_none_or(|s| s.handle != handle) {
            *slot = Some(Seen {
                handle,
                quiet_until: 0,
                cry_quiet: [0; 4],
            });
        }
        slot.as_mut().expect("just filled")
    }

    /// What the tick did to regiments: first contact charges, wavering,
    /// routing and rallying horns, and blows (heard at most once in
    /// [`BLOW_COOLDOWN`] ticks from a regiment). A regiment that is gone (it
    /// was wiped out) makes no sound.
    fn regiments_told(
        &mut self,
        world: &World,
        heard: bool,
        shown: &impl Fn(RegimentHandle) -> bool,
        voice: &impl Fn(UnitId) -> Option<Voice>,
    ) {
        let tick = world.tick_count();
        for e in world.events() {
            match *e {
                Event::Contact { regiment, .. } => {
                    if let Some(r) = world.regiments.get(regiment) {
                        self.hushed = !heard || !shown(regiment);
                        let at = from_fx_vec(r.position);
                        self.play_at(SoundId::Charge, at, 1.0, 1.4, PRI_CHARGE);
                        // Hitting home at speed: a charge, whose cry usually
                        // began as the blocks closed (see `charges`).
                        // (Braced spears that turned a charge back are
                        // given its weight, but they didn't charge.)
                        if r.charge > 0
                            && !r.braced
                            && let Some(v) = voice(r.unit)
                        {
                            self.cry(tick, Some(regiment), v, Cry::Charge, at);
                        }
                    }
                }
                Event::Morale { regiment, from, to } => {
                    if let Some(r) = world.regiments.get(regiment) {
                        self.hushed = !heard || !shown(regiment);
                        let at = from_fx_vec(r.position);
                        self.morale_change(from, to, at);
                        if let Some(v) = voice(r.unit) {
                            match (from, to) {
                                // Breaking, the first time (routing) or the
                                // second (shattered).
                                (_, MoraleState::Routing)
                                | (
                                    MoraleState::Steady | MoraleState::Wavering,
                                    MoraleState::Shattered,
                                ) => {
                                    self.cry(tick, Some(regiment), v, Cry::Rout, at);
                                }
                                (MoraleState::Routing, to) if !to.broken() => {
                                    self.cry(tick, Some(regiment), v, Cry::Rally, at);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::Losses { regiment, lost } => {
                    let Some(r) = world.regiments.get(regiment).filter(|r| r.engaged) else {
                        continue;
                    };
                    let seen = self.seen_mut(regiment);
                    if tick < seen.quiet_until {
                        continue;
                    }
                    seen.quiet_until = tick + BLOW_COOLDOWN;
                    self.hushed = !heard || !shown(regiment);
                    let gain = 0.55 + 0.15 * f32::from(lost).min(3.0);
                    self.play_at(
                        SoundId::Clash,
                        from_fx_vec(r.position),
                        gain,
                        1.0,
                        PRI_BLOWS,
                    );
                }
                _ => {}
            }
        }
        self.hushed = false;
    }

    /// A regiment's shout: its faction's cry for `kind`, placed at `at`,
    /// unless it is out of earshot or unseen, the same regiment was heard
    /// to cry the same thing not long ago (`regiment`: none for a cry that
    /// is no one regiment's, such as a champion's), or [`MAX_CRIES`] are
    /// already sounding. Returns whether it was played. `hushed` must say
    /// whether the shouter is unheard.
    fn cry(
        &mut self,
        tick: u32,
        regiment: Option<RegimentHandle>,
        voice: Voice,
        kind: Cry,
        at: Vec2,
    ) -> bool {
        if self.hushed || !self.in_earshot(at, CRY_REACH) {
            return false;
        }
        if let Some(h) = regiment
            && !self.seen_mut(h).may_cry(kind, tick)
        {
            return false;
        }
        if !self.cries.admit(tick) {
            return false;
        }
        if let Some(h) = regiment {
            self.seen_mut(h).cried(kind, tick);
        }
        let gain = match kind {
            Cry::Charge => 0.9,
            Cry::Rout | Cry::Rally => 0.8,
            Cry::Victory => 1.0,
        };
        self.play_at(
            SoundId::cry(voice.faction.index() as u8, kind),
            at,
            gain,
            CRY_REACH,
            PRI_CRY,
        );
        true
    }

    /// A regiment on the march that is closing on an enemy it is facing
    /// starts its charge cry a moment before the blocks meet (the events
    /// only tell of the contact itself). A regiment that shouts then is not
    /// heard again at contact: see [`CRY_COOLDOWN`].
    fn charges(
        &mut self,
        world: &World,
        heard: bool,
        shown: &impl Fn(RegimentHandle) -> bool,
        voice: &impl Fn(UnitId) -> Option<Voice>,
    ) {
        let tick = world.tick_count();
        // Nothing a cry could come of: not heard, between scans, or as many
        // cries as may sound already sounding.
        if !heard || !tick.is_multiple_of(CHARGE_SCAN_EVERY) || self.cries.full(tick) {
            return;
        }
        // Who could be charged, gathered once, on the first regiment that
        // might charge.
        let mut foes: Option<Vec<(Vec2, PlayerId)>> = None;
        for r in world.regiments.iter() {
            if !r.moving
                || r.engaged
                || r.falling_back
                || r.morale.broken()
                || !r.formation.can_charge()
            {
                continue;
            }
            let Some(v) = voice(r.unit).filter(|v| v.charges) else {
                continue;
            };
            let at = from_fx_vec(r.position);
            // Only a regiment near the camera, seen, and not just heard to
            // charge is worth the search.
            if !self.in_earshot(at, CRY_REACH)
                || !shown(r.handle)
                || !self.seen_mut(r.handle).may_cry(Cry::Charge, tick)
            {
                continue;
            }
            let foes = foes.get_or_insert_with(|| {
                world
                    .regiments
                    .iter()
                    // Only foes the player can see, so a cry gives none away.
                    .filter(|q| !q.morale.broken() && shown(q.handle))
                    .map(|q| (from_fx_vec(q.position), q.owner))
                    .collect()
            });
            let heading = from_fx_vec(r.facing.to_vec());
            let closing = foes
                .iter()
                .any(|&(foe, owner)| closing_on(at, heading, foe) && world.hostile(r.owner, owner));
            if closing
                && self.cry(tick, Some(r.handle), v, Cry::Charge, at)
                && self.cries.full(tick)
            {
                break;
            }
        }
    }

    /// The winner of the champions' duel has his regiment's side roar, once,
    /// where the duel was fought.
    fn duel(
        &mut self,
        world: &World,
        heard: bool,
        shown: &impl Fn(RegimentHandle) -> bool,
        voice: &impl Fn(UnitId) -> Option<Voice>,
    ) {
        let tick = world.tick_count();
        let Some(c) = world.challenge() else {
            return;
        };
        let Outcome::Killed { winner } = c.outcome(tick) else {
            return;
        };
        if self.duel_cried == Some(c.called) {
            return;
        }
        let champion = c.champions[usize::from(winner)];
        if let Some(v) = world.regiments.get(champion).and_then(|r| voice(r.unit)) {
            self.hushed = !heard || !shown(champion);
            // Marked cheered only once heard, so a camera that comes near
            // in time (or a cap that frees) still gets it.
            if self.cry(tick, None, v, Cry::Victory, from_fx_vec(c.spot)) {
                self.duel_cried = Some(c.called);
            }
            self.hushed = false;
        }
    }

    /// A volley loosed this tick sounds a twang, if its shooter can be seen
    /// (archers loose from the middle of their ranks); one that came down
    /// sounds its impact, louder for more arrows, wherever it fell.
    fn volleys(
        &mut self,
        world: &World,
        heard: bool,
        shown: &impl Fn(RegimentHandle) -> bool,
        known: &impl Fn(StructureHandle) -> bool,
    ) {
        for e in world.events() {
            match *e {
                Event::Loosed {
                    volley, shooter, ..
                } => {
                    let flying = world.volleys().find(|v| v.id == volley);
                    self.flights.push((volley, flying.map_or(0, |v| v.shots)));
                    let regiment = shooter.regiment();
                    self.hushed = !heard
                        || match shooter {
                            Shooter::Regiment(h) => !shown(h),
                            Shooter::Tower(t) => !known(t),
                        };
                    let from = flying.map(|v| from_fx_vec(v.from)).or_else(|| {
                        regiment
                            .and_then(|h| world.regiments.get(h))
                            .map(|r| from_fx_vec(r.position))
                    });
                    if let Some(from) = from {
                        self.play_at(SoundId::ArrowRelease, from, 0.8, 1.0, PRI_ARROWS);
                    }
                }
                // A Bell Tower tolls: no bell is recorded, so the low tone
                // that marks a wavering regiment stands in, heard far off.
                Event::Tolled { structure } => {
                    let Some(st) = world.structures().and_then(|s| s.get(structure)) else {
                        continue;
                    };
                    self.hushed = !heard || !known(structure);
                    let at = crate::buildings::centre_of(st.tile, st.width, st.depth);
                    self.play_at(SoundId::Waver, at, 1.0, 1.5, PRI_MORALE);
                }
                Event::Landed { volley, at, .. } => {
                    let shots = match self.flights.iter().position(|f| f.0 == volley) {
                        Some(k) => self.flights.swap_remove(k).1,
                        None => 0,
                    };
                    self.hushed = !heard;
                    let gain = 0.5 + 0.5 * (f32::from(shots) / 16.0).min(1.0);
                    self.play_at(SoundId::ArrowImpact, from_fx_vec(at), gain, 1.0, PRI_ARROWS);
                }
                _ => {}
            }
        }
        self.hushed = false;
    }

    fn morale_change(&self, from: MoraleState, to: MoraleState, at: Vec2) {
        match (from, to) {
            (_, MoraleState::Wavering) if !from.broken() => {
                self.play_at(SoundId::Waver, at, 0.8, 1.2, PRI_MORALE);
            }
            (_, MoraleState::Routing) => self.play_at(SoundId::RoutHorn, at, 1.0, 1.8, PRI_MORALE),
            (_, MoraleState::Shattered) => self.play_at(SoundId::Shatter, at, 1.0, 2.0, PRI_MORALE),
            (MoraleState::Routing, _) => self.play_at(SoundId::Rally, at, 1.0, 1.6, PRI_MORALE),
            _ => {}
        }
    }

    /// Whether `at` is within `reach` times the hearing range.
    fn in_earshot(&self, at: Vec2, reach: f32) -> bool {
        (at - self.focus).length() < self.hearing * reach
    }

    /// Play a sound at a map position: quieter with distance from the
    /// camera's ground point, panned by which side of the screen it is on.
    /// `reach` stretches how far it carries (horns carry further than blows).
    fn play_at(&self, sound: SoundId, at: Vec2, gain: f32, reach: f32, priority: u8) {
        if self.hushed {
            return;
        }
        let radius = self.hearing * reach;
        let offset = at - self.focus;
        let distance = offset.length();
        if distance >= radius {
            return;
        }
        let near = 1.0 - distance / radius;
        let pan = offset.dot(self.right) / (radius * 0.5);
        // The highs go first with distance.
        self.audio
            .play_muffled(sound, gain * near * near, pan, priority, 1.0 - near);
    }

    /// A siege's sound, where it happened (see [`crate::siege`]): blows
    /// on walls, siege engines firing and their shots landing, a building
    /// coming down, which carries far.
    pub fn siege(&self, noise: crate::siege::Noise) {
        use crate::siege::Sound;
        let (sound, gain, reach, priority) = match noise.sound {
            Sound::WallHit => (SoundId::WallHit, 0.6, 1.0, PRI_BLOWS),
            Sound::Arrows => (SoundId::ArrowImpact, 0.6, 1.0, PRI_ARROWS),
            Sound::Launch => (SoundId::SiegeLaunch, 0.8, 1.3, PRI_ARROWS),
            Sound::Impact => (SoundId::SiegeImpact, 1.0, 1.5, PRI_MORALE),
            Sound::Collapse => (SoundId::Collapse, 1.0, 2.2, PRI_MORALE),
        };
        self.play_at(sound, noise.at, gain, reach, priority);
    }

    /// Thunder, heard everywhere: `loud` (0 to 1) is how near the stroke
    /// was; the further ones come duller (the muffling distance gives).
    /// `volume` is the tuning panel's multiplier: 1 plays the nearest
    /// stroke at `THUNDER_GAIN` of full, leaving room to turn it up.
    pub fn thunder(&self, loud: f32, volume: f32) {
        let loud = loud.clamp(0.0, 1.0);
        self.audio.play_muffled(
            SoundId::Thunder,
            loud * THUNDER_GAIN * volume,
            0.0,
            PRI_CRY,
            1.0 - loud,
        );
    }

    /// An ambush sprung on or by the side you command: a war-drum roll and
    /// a horn, loud wherever the camera is, panned towards it.
    pub fn ambush(&self, at: Vec2) {
        let pan = ((at - self.focus).dot(self.right) / self.hearing.max(1.0)).clamp(-1.0, 1.0);
        self.audio.play(SoundId::Charge, 1.0, pan, PRI_MORALE);
        self.audio.play(SoundId::RoutHorn, 0.6, pan, PRI_MORALE);
    }

    /// Interface click: full volume, centred.
    pub fn ui(&self, sound: UiSound) {
        let id = match sound {
            UiSound::Select => SoundId::UiSelect,
            UiSound::Move => SoundId::UiMove,
            UiSound::Attack => SoundId::UiAttack,
        };
        self.audio.play(id, 0.8, 0.0, PRI_UI);
    }

    /// The Master and Music sliders moved (0..=100): applies at once.
    pub fn set_volumes(&mut self, master: u8, music: u8) {
        // Any settings change comes here, a typed name included.
        if (master, music) == (self.master, self.music) {
            return;
        }
        self.master = master;
        self.music = music;
        self.apply_state();
    }

    /// M: silence or restore everything.
    pub fn toggle_mute(&mut self) {
        self.muted = !self.muted;
        self.apply_state();
    }

    /// N: music off or on, effects stay.
    pub fn toggle_music(&mut self) {
        self.music_on = !self.music_on;
        self.apply_state();
    }

    /// One line for the dev overlay.
    pub fn overlay_line(&self) -> String {
        format!(
            "{} {} Hz   voices {}/{}   dropped {}   {}{}",
            self.audio.device_name(),
            self.audio.sample_rate(),
            self.audio.voices_active(),
            gc_audio::MAX_VOICES,
            self.audio.sounds_dropped(),
            if self.muted { "MUTED  " } else { "" },
            if self.music_on {
                "music on"
            } else {
                "music off"
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_match_hears_nothing_of_the_last() {
        let handle = RegimentHandle {
            index: 0,
            generation: 1,
        };
        let mut seen = vec![
            Some(Seen {
                handle,
                quiet_until: 900,
                cry_quiet: [900; 4],
            });
            4
        ];
        let mut flights = vec![(
            VolleyId {
                slot: 1,
                launch_tick: 2,
            },
            8,
        )];
        let mut cries = CryCap::default();
        assert!(cries.admit(5));
        let mut duel = Some(40);
        forget_match(&mut seen, &mut flights, &mut cries, &mut duel);
        assert!(seen.iter().all(Option::is_none));
        assert!(cries.starts.is_empty() && duel.is_none());
        assert_eq!(seen.len(), 4, "one slot per regiment stays");
        assert!(flights.is_empty());
    }

    #[test]
    fn only_so_many_cries_sound_at_once() {
        let mut cap = CryCap::default();
        for _ in 0..MAX_CRIES {
            assert!(cap.admit(100));
        }
        assert!(!cap.admit(100), "a fifth at once is left out");
        assert!(!cap.admit(100 + CRY_TICKS - 1), "they are still sounding");
        assert!(cap.admit(100 + CRY_TICKS), "room again once they are over");
    }

    #[test]
    fn a_regiment_is_not_heard_to_cry_the_same_cry_again_at_once() {
        let handle = RegimentHandle {
            index: 3,
            generation: 1,
        };
        let mut seen = Seen {
            handle,
            quiet_until: 0,
            cry_quiet: [0; 4],
        };
        assert!(seen.may_cry(Cry::Charge, 50));
        seen.cried(Cry::Charge, 50);
        assert!(!seen.may_cry(Cry::Charge, 50 + CRY_COOLDOWN[0] - 1));
        assert!(seen.may_cry(Cry::Charge, 50 + CRY_COOLDOWN[0]));
        assert!(seen.may_cry(Cry::Rout, 51), "a rout is another cry");
    }

    #[test]
    fn a_charge_cry_starts_on_the_foe_ahead_and_close() {
        let (at, east) = (Vec2::new(10.0, 10.0), Vec2::X);
        assert!(closing_on(at, east, Vec2::new(15.0, 10.0)), "dead ahead");
        assert!(closing_on(at, east, Vec2::new(15.0, 12.0)), "a little off");
        assert!(!closing_on(at, east, Vec2::new(25.0, 10.0)), "too far");
        assert!(!closing_on(at, east, Vec2::new(5.0, 10.0)), "behind");
        assert!(
            !closing_on(at, east, Vec2::new(10.0, 15.0)),
            "off to the side"
        );
        assert!(!closing_on(at, east, at), "on top of itself");
    }

    fn probe_of(wet: fn(Vec2) -> bool, wood: fn(Vec2) -> bool) -> impl Fn(Vec2) -> Probe {
        move |p| Probe {
            wet: wet(p),
            wood: wood(p),
            height: 0.0,
        }
    }

    const NONE: fn(Vec2) -> bool = |_| false;

    #[test]
    fn a_distant_fight_is_a_dull_quiet_roar_and_a_near_one_is_loud() {
        let radius = 100.0;
        let mut far = Roar::default();
        far.add(90.0, 90.0, radius);
        let mut near = Roar::default();
        near.add(5.0, 5.0, radius);
        let (far, near) = (far.mix(radius), near.mix(radius));
        assert!(near.level > 3.0 * far.level, "{near:?} against {far:?}");
        assert!(far.dull > 0.8 && near.dull < 0.3, "{near:?} {far:?}");
        assert!(far.pan > 0.0, "the fight is to the right");
    }

    #[test]
    fn more_fights_make_a_bigger_roar_that_never_passes_the_cap() {
        let radius = 100.0;
        let level = |n: usize| {
            let mut roar = Roar::default();
            for _ in 0..n {
                roar.add(40.0, -40.0, radius);
            }
            roar.mix(radius)
        };
        assert!(level(1).level < level(3).level);
        assert!(level(50).level <= ROAR_MAX);
        assert!(level(1).pan < 0.0);
        // Out of earshot, or nothing fighting: silence.
        let mut out = Roar::default();
        out.add(150.0, 0.0, radius);
        assert_eq!(out.mix(radius), BedMix::default());
        assert_eq!(Roar::default().mix(radius), BedMix::default());
    }

    #[test]
    fn a_river_to_the_right_is_heard_on_the_right() {
        // A stream six tiles wide, 12 tiles east of the camera.
        let probe = probe_of(|p| (p.x - 12.0).abs() < 3.0, NONE);
        let [_, water, woods] = land_mix(probe, Vec2::ZERO, Vec2::X, 40.0);
        assert!(water.level > 0.3, "{water:?}");
        assert!(water.pan > 0.1, "it is on the right: {water:?}");
        assert_eq!(woods.level, 0.0);
        // From the other side of the river it is on the left.
        let [_, flipped, _] = land_mix(
            probe_of(|p| (p.x - 12.0).abs() < 3.0, NONE),
            Vec2::new(24.0, 0.0),
            Vec2::X,
            40.0,
        );
        assert!(flipped.pan < -0.1, "{flipped:?}");
    }

    #[test]
    fn standing_in_woods_is_louder_than_standing_beside_them() {
        let in_wood = land_mix(probe_of(NONE, |_| true), Vec2::ZERO, Vec2::X, 40.0)[2];
        let beside = land_mix(probe_of(NONE, |p| p.x > 20.0), Vec2::ZERO, Vec2::X, 40.0)[2];
        assert!(in_wood.level > beside.level && beside.level > 0.0);
        assert!(in_wood.dull < beside.dull);
    }

    #[test]
    fn wind_is_stronger_in_the_open_and_on_a_rise() {
        let flat = |wood: bool| {
            move |_: Vec2| Probe {
                wet: false,
                wood,
                height: 0.0,
            }
        };
        let open = land_mix(flat(false), Vec2::ZERO, Vec2::X, 40.0)[0].level;
        let wooded = land_mix(flat(true), Vec2::ZERO, Vec2::X, 40.0)[0].level;
        // A hill: the camera is at the top.
        let hill = land_mix(
            |p: Vec2| Probe {
                wet: false,
                wood: false,
                height: 10.0 - p.length() * 0.3,
            },
            Vec2::ZERO,
            Vec2::X,
            40.0,
        )[0]
        .level;
        assert!(open > wooded && hill > open, "{wooded} {open} {hill}");
        assert!(wooded > 0.0, "there is always a breeze");
    }

    #[test]
    fn the_beds_fade_as_the_camera_pulls_back() {
        assert_eq!(zoom_hush(14.0), 1.0);
        assert!(zoom_hush(110.0) < 1.0 && zoom_hush(110.0) > zoom_hush(260.0));
        assert_eq!(zoom_hush(10_000.0), BED_ZOOM_FLOOR);
    }

    #[test]
    fn a_bed_is_only_resent_when_it_changes_audibly() {
        let mut sent = BedsSent::default();
        let mix = BedMix {
            level: 0.5,
            pan: 0.2,
            dull: 0.1,
        };
        assert!(sent.changed(1, mix));
        assert!(!sent.changed(
            1,
            BedMix {
                level: 0.505,
                ..mix
            }
        ));
        assert!(sent.changed(1, BedMix { level: 0.6, ..mix }));
        assert!(sent.changed(2, mix), "each bed is its own");
        sent.forget();
        assert!(sent.changed(1, mix), "a new output knows nothing");
    }

    #[test]
    fn sliders_become_volumes() {
        assert_eq!(slider_gain(0), 0.0);
        assert_eq!(slider_gain(50), 0.5);
        assert_eq!(slider_gain(100), 1.0);
        // A hand-edited setting past the end does not boost.
        assert_eq!(slider_gain(200), 1.0);
    }

    #[test]
    fn mute_and_music_off_override_the_sliders() {
        assert_eq!(levels(80, 60, false, true), (0.8, 0.6));
        assert_eq!(levels(80, 60, true, true), (0.0, 0.6));
        assert_eq!(levels(80, 60, false, false), (0.8, 0.0));
        assert_eq!(levels(0, 0, false, true), (0.0, 0.0));
    }

    #[test]
    fn embedded_sound_bank_is_complete() {
        let bank = Bank::parse(BANK).expect("embedded sound bank parses");
        bank.require_all().expect("every sound is present");
    }
}
