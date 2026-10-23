//! The sound bank: every clip the game can play, in one embedded blob.
//!
//! Layout (little-endian): magic `GCSN`, version `u32`, clip count `u32`;
//! then one 20-byte entry per clip (`id u8`, `flags u8`, 2 bytes padding,
//! `rate u32`, `samples u32`, `offset u32`, `len u32`); then the ADPCM data.
//! `gc_sfx` writes it; the game embeds it with `include_bytes!`.

use std::fmt;

/// Every sound the game knows. The numbers are stored in the bank and the
/// command queue, so only ever add to the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SoundId {
    /// A bowstring twang as a volley leaves the archers.
    ArrowRelease = 0,
    /// Arrows thudding into wood, shields and flesh.
    ArrowImpact = 1,
    /// Steel on steel: melee blows landing.
    Clash = 2,
    /// A war-drum roll as two regiments meet.
    Charge = 3,
    /// A low, uneasy tone when a regiment starts to waver.
    Waver = 4,
    /// A horn blown as a regiment breaks and runs.
    RoutHorn = 5,
    /// A crash as a regiment shatters for good.
    Shatter = 6,
    /// A rising horn as a routed regiment rallies.
    Rally = 7,
    /// UI: a regiment was selected.
    UiSelect = 8,
    /// UI: a move order was given.
    UiMove = 9,
    /// UI: an attack order was given.
    UiAttack = 10,
    /// Music, calm layer (loops).
    MusicCalm = 11,
    /// Music, battle layer (loops, plays in step with the calm layer).
    MusicBattle = 12,
    /// An Ember Clans regiment charges into contact: its war voice.
    EmberCharge = 13,
    /// An Ember Clans regiment breaks and runs: its war voice.
    EmberRout = 14,
    /// An Ember Clans regiment rallies: its war voice.
    EmberRally = 15,
    /// An Ember Clans regiment's champion wins the pre-battle duel; the regiment roars.
    EmberVictory = 16,
    /// A Brood regiment charges into contact: its war voice.
    BroodCharge = 17,
    /// A Brood regiment breaks and runs: its war voice.
    BroodRout = 18,
    /// A Brood regiment rallies: its war voice.
    BroodRally = 19,
    /// A Brood regiment's champion wins the pre-battle duel; the regiment roars.
    BroodVictory = 20,
    /// A Shard Choir regiment charges into contact: its war voice.
    ChoirCharge = 21,
    /// A Shard Choir regiment breaks and runs: its war voice.
    ChoirRout = 22,
    /// A Shard Choir regiment rallies: its war voice.
    ChoirRally = 23,
    /// A Shard Choir regiment's champion wins the pre-battle duel; the regiment roars.
    ChoirVictory = 24,
    /// A Carrion Concord regiment charges into contact: its war voice.
    CarrionCharge = 25,
    /// A Carrion Concord regiment breaks and runs: its war voice.
    CarrionRout = 26,
    /// A Carrion Concord regiment rallies: its war voice.
    CarrionRally = 27,
    /// A Carrion Concord regiment's champion wins the pre-battle duel; the regiment roars.
    CarrionVictory = 28,
    /// Bed (loops): wind over open ground.
    BedWind = 29,
    /// Bed (loops): running and lapping water.
    BedWater = 30,
    /// Bed (loops): leaves and branches in woods.
    BedWoods = 31,
    /// Bed (loops): the far roar of many soldiers fighting.
    BedBattle = 32,
    /// A blade or club striking a wall or door: a woody thunk, a splintery
    /// crack and a little grit falling. Short and dry so many can overlap.
    WallHit = 33,
    /// A boulder or siege shot slamming into a building: a deep boom, a
    /// crunch of timber and stone and rubble trickling down.
    SiegeImpact = 34,
    /// A catapult or siege engine firing: a creak, the heavy whump of the
    /// arm and a rushing whoosh.
    SiegeLaunch = 35,
    /// A building coming down: a cracking groan, a rumbling roar of masonry
    /// and timber with heavy thuds, settling into trickling debris.
    Collapse = 36,
}

impl SoundId {
    /// The battle cries, by faction (Ember Clans, Brood, Shard Choir,
    /// Carrion Concord) and by [`Cry`].
    const CRIES: [[SoundId; 4]; 4] = [
        [
            SoundId::EmberCharge,
            SoundId::EmberRout,
            SoundId::EmberRally,
            SoundId::EmberVictory,
        ],
        [
            SoundId::BroodCharge,
            SoundId::BroodRout,
            SoundId::BroodRally,
            SoundId::BroodVictory,
        ],
        [
            SoundId::ChoirCharge,
            SoundId::ChoirRout,
            SoundId::ChoirRally,
            SoundId::ChoirVictory,
        ],
        [
            SoundId::CarrionCharge,
            SoundId::CarrionRout,
            SoundId::CarrionRally,
            SoundId::CarrionVictory,
        ],
    ];

    pub const COUNT: usize = 37;

    pub const ALL: [SoundId; SoundId::COUNT] = [
        SoundId::ArrowRelease,
        SoundId::ArrowImpact,
        SoundId::Clash,
        SoundId::Charge,
        SoundId::Waver,
        SoundId::RoutHorn,
        SoundId::Shatter,
        SoundId::Rally,
        SoundId::UiSelect,
        SoundId::UiMove,
        SoundId::UiAttack,
        SoundId::MusicCalm,
        SoundId::MusicBattle,
        SoundId::EmberCharge,
        SoundId::EmberRout,
        SoundId::EmberRally,
        SoundId::EmberVictory,
        SoundId::BroodCharge,
        SoundId::BroodRout,
        SoundId::BroodRally,
        SoundId::BroodVictory,
        SoundId::ChoirCharge,
        SoundId::ChoirRout,
        SoundId::ChoirRally,
        SoundId::ChoirVictory,
        SoundId::CarrionCharge,
        SoundId::CarrionRout,
        SoundId::CarrionRally,
        SoundId::CarrionVictory,
        SoundId::BedWind,
        SoundId::BedWater,
        SoundId::BedWoods,
        SoundId::BedBattle,
        SoundId::WallHit,
        SoundId::SiegeImpact,
        SoundId::SiegeLaunch,
        SoundId::Collapse,
    ];

    /// The ambient beds, in the order [`Bed`](crate::queue::Command::Bed) numbers them.
    pub const BEDS: [SoundId; 4] = [
        SoundId::BedWind,
        SoundId::BedWater,
        SoundId::BedWoods,
        SoundId::BedBattle,
    ];

    pub fn from_u8(v: u8) -> Option<SoundId> {
        SoundId::ALL.get(v as usize).copied()
    }

    pub fn is_music(self) -> bool {
        matches!(self, SoundId::MusicCalm | SoundId::MusicBattle)
    }

    /// Whether this is an ambient bed: a loop the mixer plays continuously
    /// at a level it is told, not a one-shot.
    pub fn is_bed(self) -> bool {
        SoundId::BEDS.contains(&self)
    }

    /// The battle cry of a faction (0 Ember Clans, 1 Brood, 2 Shard Choir,
    /// 3 Carrion Concord, the order of the game's factions; a larger number
    /// wraps) for an event.
    pub fn cry(faction: u8, cry: Cry) -> SoundId {
        SoundId::CRIES[usize::from(faction % 4)][cry as usize]
    }
}

/// What a regiment shouts about; each faction has its own voice for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Cry {
    /// It charges into contact with the enemy.
    Charge = 0,
    /// It breaks and runs.
    Rout = 1,
    /// It rallies after a rout.
    Rally = 2,
    /// Its champion wins the duel between the lines.
    Victory = 3,
}

/// One compressed clip, borrowed from the bank blob.
#[derive(Clone, Copy, Debug)]
pub struct Clip<'a> {
    /// Samples per second.
    pub rate: u32,
    /// Decoded length in samples.
    pub samples: u32,
    pub looped: bool,
    pub data: &'a [u8],
}

/// A clip to be written into a bank.
pub struct ClipSource<'a> {
    pub id: SoundId,
    pub rate: u32,
    pub looped: bool,
    pub pcm: &'a [i16],
}

#[derive(Debug, PartialEq, Eq)]
pub enum BankError {
    Truncated,
    BadMagic,
    BadVersion(u32),
    BadEntry(usize),
    Missing(SoundId),
}

impl fmt::Display for BankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BankError::Truncated => write!(f, "sound bank is truncated"),
            BankError::BadMagic => write!(f, "not a sound bank"),
            BankError::BadVersion(v) => write!(f, "unsupported sound bank version {v}"),
            BankError::BadEntry(i) => write!(f, "sound bank entry {i} is invalid"),
            BankError::Missing(id) => write!(f, "sound bank has no {id:?}"),
        }
    }
}

impl std::error::Error for BankError {}

const MAGIC: &[u8; 4] = b"GCSN";
const VERSION: u32 = 1;
const HEADER: usize = 12;
const ENTRY: usize = 20;

pub struct Bank<'a> {
    clips: [Option<Clip<'a>>; SoundId::COUNT],
}

impl<'a> Bank<'a> {
    pub fn parse(blob: &'a [u8]) -> Result<Bank<'a>, BankError> {
        if blob.len() < HEADER {
            return Err(BankError::Truncated);
        }
        if &blob[0..4] != MAGIC {
            return Err(BankError::BadMagic);
        }
        let u32_at = |at: usize| u32::from_le_bytes(blob[at..at + 4].try_into().unwrap());
        let version = u32_at(4);
        if version != VERSION {
            return Err(BankError::BadVersion(version));
        }
        let count = u32_at(8) as usize;
        if count > SoundId::COUNT {
            return Err(BankError::BadEntry(count));
        }
        let table_end = HEADER + count * ENTRY;
        if blob.len() < table_end {
            return Err(BankError::Truncated);
        }
        let mut clips = [None; SoundId::COUNT];
        for i in 0..count {
            let at = HEADER + i * ENTRY;
            let id = SoundId::from_u8(blob[at]).ok_or(BankError::BadEntry(i))?;
            let looped = blob[at + 1] & 1 != 0;
            let (rate, samples) = (u32_at(at + 4), u32_at(at + 8));
            let (offset, len) = (u32_at(at + 12) as usize, u32_at(at + 16) as usize);
            let end = offset.checked_add(len).ok_or(BankError::BadEntry(i))?;
            if end > blob.len() || offset < table_end {
                return Err(BankError::Truncated);
            }
            if rate == 0 || len != (samples as usize).div_ceil(2) || clips[id as usize].is_some() {
                return Err(BankError::BadEntry(i));
            }
            clips[id as usize] = Some(Clip {
                rate,
                samples,
                looped,
                data: &blob[offset..end],
            });
        }
        Ok(Bank { clips })
    }

    pub fn clip(&self, id: SoundId) -> Option<&Clip<'a>> {
        self.clips[id as usize].as_ref()
    }

    /// Fails unless every sound in [`SoundId::ALL`] is present.
    pub fn require_all(&self) -> Result<(), BankError> {
        match SoundId::ALL.iter().find(|&&id| self.clip(id).is_none()) {
            Some(&id) => Err(BankError::Missing(id)),
            None => Ok(()),
        }
    }

    /// Total compressed size of all clips, for the dev overlay.
    pub fn data_bytes(&self) -> usize {
        self.clips.iter().flatten().map(|c| c.data.len()).sum()
    }
}

/// Compress clips and lay them out as a bank blob.
pub fn write_bank(sources: &[ClipSource]) -> Vec<u8> {
    let packed: Vec<Vec<u8>> = sources
        .iter()
        .map(|s| crate::adpcm::encode(s.pcm))
        .collect();
    let mut blob = Vec::new();
    blob.extend_from_slice(MAGIC);
    blob.extend_from_slice(&VERSION.to_le_bytes());
    blob.extend_from_slice(&(sources.len() as u32).to_le_bytes());
    let mut offset = HEADER + sources.len() * ENTRY;
    for (s, data) in sources.iter().zip(&packed) {
        blob.push(s.id as u8);
        blob.push(u8::from(s.looped));
        blob.extend_from_slice(&[0, 0]);
        blob.extend_from_slice(&s.rate.to_le_bytes());
        blob.extend_from_slice(&(s.pcm.len() as u32).to_le_bytes());
        blob.extend_from_slice(&(offset as u32).to_le_bytes());
        blob.extend_from_slice(&(data.len() as u32).to_le_bytes());
        offset += data.len();
    }
    for data in &packed {
        blob.extend_from_slice(data);
    }
    blob
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_parse() {
        let pcm: Vec<i16> = (0..1001).map(|i| (i * 20) as i16).collect();
        let blob = write_bank(&[
            ClipSource {
                id: SoundId::Clash,
                rate: 22050,
                looped: false,
                pcm: &pcm,
            },
            ClipSource {
                id: SoundId::MusicCalm,
                rate: 16000,
                looped: true,
                pcm: &pcm[..400],
            },
        ]);
        let bank = Bank::parse(&blob).unwrap();
        let clash = bank.clip(SoundId::Clash).unwrap();
        assert_eq!(
            (clash.rate, clash.samples, clash.looped),
            (22050, 1001, false)
        );
        let calm = bank.clip(SoundId::MusicCalm).unwrap();
        assert_eq!((calm.rate, calm.samples, calm.looped), (16000, 400, true));
        assert!(bank.clip(SoundId::Rally).is_none());
        assert_eq!(
            bank.require_all(),
            Err(BankError::Missing(SoundId::ArrowRelease))
        );
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(Bank::parse(b"nope").err(), Some(BankError::Truncated));
        assert_eq!(Bank::parse(&[0u8; 32]).err(), Some(BankError::BadMagic));
        let mut blob = write_bank(&[ClipSource {
            id: SoundId::Clash,
            rate: 8000,
            looped: false,
            pcm: &[1, 2, 3],
        }]);
        blob.truncate(blob.len() - 1);
        assert!(Bank::parse(&blob).is_err());
    }

    #[test]
    fn ids_round_trip() {
        for id in SoundId::ALL {
            assert_eq!(SoundId::from_u8(id as u8), Some(id));
        }
        assert_eq!(SoundId::from_u8(SoundId::COUNT as u8), None);
    }

    #[test]
    fn every_faction_has_its_four_cries() {
        let mut seen = Vec::new();
        for faction in 0..4 {
            for cry in [Cry::Charge, Cry::Rout, Cry::Rally, Cry::Victory] {
                let id = SoundId::cry(faction, cry);
                assert!(!id.is_music() && !seen.contains(&id));
                seen.push(id);
            }
        }
        assert_eq!(SoundId::cry(0, Cry::Charge), SoundId::EmberCharge);
        assert_eq!(SoundId::cry(3, Cry::Victory), SoundId::CarrionVictory);
        assert_eq!(SoundId::cry(5, Cry::Rout), SoundId::BroodRout, "wraps");
    }
}
