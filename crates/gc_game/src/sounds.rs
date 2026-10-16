//! Turns what happens in the match into sound.
//!
//! The simulation knows nothing about audio. After each tick this module
//! reads what the simulation says the tick did (`World::events`) and plays a
//! sound for each: volleys leaving and landing, blows in melee, a regiment
//! wavering, breaking, shattering or rallying. Sounds are placed relative to the
//! camera's ground point: quieter with distance (less so when zoomed out)
//! and panned left or right.

use crate::camera::RtsCamera;
use crate::from_fx_vec;
use gc_audio::{Audio, Bank, SoundId};
use gc_platform::logln;
use gc_sim::events::{Event, VolleyId};
use gc_sim::{MoraleState, RegimentHandle, World};
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

// Who is cut off first when all 32 voices are busy.
const PRI_ARROWS: u8 = 70;
const PRI_BLOWS: u8 = 100;
const PRI_CHARGE: u8 = 170;
const PRI_MORALE: u8 = 220;
const PRI_UI: u8 = 255;

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

#[derive(Clone, Copy)]
struct Seen {
    handle: RegimentHandle,
    /// Tick before which no further blow is played for it.
    quiet_until: u32,
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
    /// True while `observe` only records what it sees: no sound plays.
    hushed: bool,
    /// When the output may next be reopened, and how long to wait after
    /// that if it fails again. Kept after a success, so a stream that dies
    /// again at once still waits.
    reopen: Option<(Instant, Duration)>,
}

fn forget_match(seen: &mut [Option<Seen>], flights: &mut Vec<Shots>) {
    seen.fill(None);
    flights.clear();
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
                    hushed: false,
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
        forget_match(&mut self.seen, &mut self.flights);
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
    pub fn observe(
        &mut self,
        world: &World,
        camera: &RtsCamera,
        heard: bool,
        shown: impl Fn(RegimentHandle) -> bool,
    ) {
        self.focus = camera.focus;
        self.right = Vec2::new(camera.yaw.cos(), -camera.yaw.sin());
        self.hearing = HEAR_BASE + camera.distance * HEAR_PER_ZOOM;
        let tick = world.tick_count();

        self.volleys(world, heard, &shown);
        self.regiments_told(world, heard, &shown);

        let mut fighting = 0.0f32;
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
            if r.engaged && (at - self.focus).length() < self.hearing * 1.5 {
                fighting += 1.0;
            }
        }
        self.hushed = false;

        let intensity = (fighting / FULL_BATTLE).min(1.0);
        let byte = (intensity * 20.0).round() as u8;
        if byte != self.last_intensity {
            self.last_intensity = byte;
            self.audio.set_music_intensity(intensity);
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
    ) {
        let tick = world.tick_count();
        for e in world.events() {
            match *e {
                Event::Contact { regiment, .. } => {
                    if let Some(r) = world.regiments.get(regiment) {
                        self.hushed = !heard || !shown(regiment);
                        self.play_at(
                            SoundId::Charge,
                            from_fx_vec(r.position),
                            1.0,
                            1.4,
                            PRI_CHARGE,
                        );
                    }
                }
                Event::Morale { regiment, from, to } => {
                    if let Some(r) = world.regiments.get(regiment) {
                        self.hushed = !heard || !shown(regiment);
                        self.morale_change(from, to, from_fx_vec(r.position));
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

    /// A volley loosed this tick sounds a twang, if its shooter can be seen
    /// (archers loose from the middle of their ranks); one that came down
    /// sounds its impact, louder for more arrows, wherever it fell.
    fn volleys(&mut self, world: &World, heard: bool, shown: &impl Fn(RegimentHandle) -> bool) {
        for e in world.events() {
            match *e {
                Event::Loosed {
                    volley, shooter, ..
                } => {
                    let flying = world.volleys().find(|v| v.id == volley);
                    self.flights.push((volley, flying.map_or(0, |v| v.shots)));
                    self.hushed = !heard || !shown(shooter);
                    let from = flying.map(|v| from_fx_vec(v.from)).or_else(|| {
                        world
                            .regiments
                            .get(shooter)
                            .map(|r| from_fx_vec(r.position))
                    });
                    if let Some(from) = from {
                        self.play_at(SoundId::ArrowRelease, from, 0.8, 1.0, PRI_ARROWS);
                    }
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
        self.audio.play(sound, gain * near * near, pan, priority);
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
        forget_match(&mut seen, &mut flights);
        assert!(seen.iter().all(Option::is_none));
        assert_eq!(seen.len(), 4, "one slot per regiment stays");
        assert!(flights.is_empty());
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
