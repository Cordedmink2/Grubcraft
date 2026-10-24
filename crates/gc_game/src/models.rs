//! Soldier and building models, baked from glTF by `tools/gc_artbake` into
//! `assets/models.pak` and `assets/buildings/*.pak`, joined into one pack by
//! the build script and embedded in the executable, and the choice of
//! which animation frame each soldier shows.

use gc_assets::anim::Act;
use gc_assets::detail::Detail;
use gc_assets::model::Models;
use gc_assets::{Aligned, Pack};

use crate::moves::{MoveClip, Moves, Want};
use crate::puppets::Move;

static PACK: &Aligned<[u8]> = &Aligned(*include_bytes!(concat!(env!("OUT_DIR"), "/models.pak")));

/// The embedded models. The pack is checked when the game starts; a failure
/// means the committed pack is broken, so the game runs with boxes instead.
pub fn load() -> Result<Models<'static>, String> {
    let pack = Pack::parse(&PACK.0).map_err(|e| format!("embedded model pack: {e}"))?;
    Models::from_pack(&pack).map_err(|e| format!("embedded model pack: {e}"))
}

static DETAIL: &Aligned<[u8]> = &Aligned(*include_bytes!("../../../assets/detail.pak"));

/// The embedded surface detail textures (`assets/detail.pak`), layered over
/// the models' own. A failure means the committed pack is broken, so the
/// models draw without detail.
pub fn load_detail() -> Result<Detail<'static>, String> {
    let pack = Pack::parse(&DETAIL.0).map_err(|e| format!("embedded detail pack: {e}"))?;
    Detail::from_pack(&pack).map_err(|e| format!("embedded detail pack: {e}"))
}

/// What a soldier is doing, which picks the clip he plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Idle,
    Walk,
    Run,
    Attack,
    Attack2,
    Shoot,
    Cheer,
    /// Reeling from a blow.
    Hit,
    /// Guarding against a blow.
    Block,
}

/// The two bone texture rows to blend between and how far, or `None` when
/// the model lacks every clip that could stand in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub rows: [u16; 2],
    pub blend: f32,
}

/// A soldier instance's `flags`: the blend toward the second frame, the
/// selection `highlight`, and whether it lies `flat`, as 0 to 255.
pub fn pose_flags(pose: Pose, highlight: u8, flat: bool) -> [u8; 4] {
    [
        (pose.blend * 255.0) as u8,
        highlight,
        u8::from(flat) * 255,
        0,
    ]
}

const ACTIONS: [Action; 9] = [
    Action::Idle,
    Action::Walk,
    Action::Run,
    Action::Attack,
    Action::Attack2,
    Action::Shoot,
    Action::Cheer,
    Action::Hit,
    Action::Block,
];

/// What the move picker is asked for an action.
fn want(action: Action) -> Want {
    let (act, seed) = match action {
        Action::Idle => (Act::Idle, 0),
        Action::Walk => (Act::Walk, 0),
        Action::Run => (Act::Run, 0),
        Action::Attack => (Act::Attack, 0),
        // The model's second attack, or its only one.
        Action::Attack2 => (Act::Attack, 1),
        Action::Shoot => (Act::Shoot, 0),
        Action::Cheer => (Act::Cheer, 0),
        Action::Hit => (Act::Hit, 0),
        Action::Block => (Act::Block, 0),
    };
    Want {
        seed,
        ..Want::new(act)
    }
}

/// How far into the attack clip a parry raises the weapon to meet the blow,
/// and into the block clip a soldier ducks behind his guard.
const PARRY_DEPTH: f32 = 0.35;
const BLOCK_DEPTH: f32 = 0.25;
/// A knockdown spends this much of its time falling, and this much getting
/// up; between the two he lies still.
const DOWN_FALL: f32 = 0.25;
const DOWN_RISE: f32 = 0.35;

/// One model's moves for what soldiers do, picked once.
struct ModelClips {
    /// In the order of `ACTIONS`, which is the enum's.
    actions: [Option<MoveClip>; ACTIONS.len()],
    die: Option<MoveClip>,
    /// The picks for a duel's parry and stagger: moves made for them when
    /// the model has them, else what the picker falls back to (an attack
    /// raised partway, a hit).
    parry: Option<MoveClip>,
    stagger: Option<MoveClip>,
}

/// Per model, the move for each action, picked once by the move picker
/// (`moves.rs`).
pub struct Clips {
    by_model: Vec<ModelClips>,
}

impl Clips {
    pub fn new(models: &Models<'_>) -> Clips {
        let moves = Moves::new(models);
        let by_model = (0..models.models.len())
            .map(|m| {
                let pick = |act| moves.pick(m, Want::new(act));
                ModelClips {
                    actions: ACTIONS.map(|a| moves.pick(m, want(a))),
                    die: pick(Act::Death),
                    parry: pick(Act::Parry),
                    stagger: pick(Act::Stagger),
                }
            })
            .collect();
        Clips { by_model }
    }

    /// The move `model` plays for `action`.
    pub fn clip(&self, model: usize, action: Action) -> Option<MoveClip> {
        self.by_model.get(model)?.actions[action as usize]
    }

    /// The pose for `action` `seconds` into its move, looping.
    pub fn pose(&self, model: usize, action: Action, seconds: f32) -> Option<Pose> {
        Some(self.clip(model, action)?.looping(seconds))
    }

    /// The pose `fraction` (0 to 1) of the way through the move for
    /// `action`, played once whatever its length.
    pub fn at(&self, model: usize, action: Action, fraction: f32) -> Option<Pose> {
        Some(self.clip(model, action)?.at(fraction))
    }

    /// The pose `seconds` after falling dead (negative while he still
    /// stands, waiting for the blow), and whether to lay the model flat: see
    /// `fall`.
    pub fn death(&self, model: usize, seconds: f32) -> Option<(Pose, bool)> {
        self.fall(model, |die| seconds * die.fps, seconds >= 0.0)
    }

    /// The death move at the frame `frame` picks, held on its first and
    /// last frames, or for a model without one its idle pose, laid flat
    /// when `down`.
    fn fall(
        &self,
        model: usize,
        frame: impl FnOnce(&MoveClip) -> f32,
        down: bool,
    ) -> Option<(Pose, bool)> {
        match self.by_model.get(model)?.die {
            Some(die) => Some((die.at_frame(frame(&die)), false)),
            None => Some((self.pose(model, Action::Idle, 0.0)?, down)),
        }
    }

    /// The pose for a soldier's move in a duel, and whether to lay the model
    /// flat (a knockdown on a model with no death clip). `seconds` keeps the
    /// guard's idle loop going.
    pub fn fight(&self, model: usize, fight: Move, seconds: f32) -> Option<(Pose, bool)> {
        let standing = |pose: Option<Pose>| Some((pose?, false));
        let own = self.by_model.get(model)?;
        match fight {
            Move::Guard => standing(self.pose(model, Action::Idle, seconds)),
            Move::Cheer => standing(self.pose(model, Action::Cheer, seconds)),
            Move::Swing { at, alt } => {
                let attack = if alt { Action::Attack2 } else { Action::Attack };
                standing(self.at(model, attack, at))
            }
            Move::Parry(f) => {
                let parry = own.parry?;
                if parry.act == Act::Parry {
                    standing(Some(parry.at(f)))
                } else {
                    // Raise the weapon to meet the blow and recover.
                    standing(Some(parry.at(PARRY_DEPTH * peak(f))))
                }
            }
            // Duck behind the shield and come back up.
            Move::Block(f) => standing(self.at(model, Action::Block, BLOCK_DEPTH * peak(f))),
            // A stagger, or the hit standing in for one.
            Move::Stagger(f) => standing(Some(own.stagger?.at(f))),
            // Fall, lie still a moment and get up: the death move played
            // forward, then back.
            Move::Down(f) => {
                let depth = (f / DOWN_FALL).min((1.0 - f) / DOWN_RISE).min(1.0);
                self.fall(model, |die| depth * (die.frames - 1) as f32, depth > 0.5)
            }
        }
    }
}

/// Small differences between soldiers of one model, so a regiment does not
/// look cloned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// For the last byte of [`gc_render::SoldierInstance::color`]
    /// ([`gc_render::TONE_NEUTRAL`] leaves him as painted).
    pub tone: u8,
    /// Playback rate of his idle, walk and run loops, 0.93 to 1.07.
    pub rate: f32,
    /// His height, 0.97 to 1.03.
    pub scale: f32,
}

/// The look of a soldier whose stable look value is `seed`
/// (`puppets::Soldier::look`).
pub fn look(seed: u8) -> Look {
    let mix = |salt| crate::puppets::mix(u32::from(seed), 0, 0, salt);
    Look {
        tone: seed,
        rate: 0.93 + 0.14 * mix(1),
        scale: 0.97 + 0.06 * mix(2),
    }
}

/// 0 at both ends of `f` (0 to 1), rising to 1 halfway.
fn peak(f: f32) -> f32 {
    1.0 - (2.0 * f - 1.0).abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_assets::model::ClipKind;

    #[test]
    fn looks_stay_in_range_and_vary() {
        let looks: Vec<Look> = (0..=255).map(look).collect();
        assert!(looks.iter().all(|l| (0.93..=1.07).contains(&l.rate)));
        assert!(looks.iter().all(|l| (0.97..=1.03).contains(&l.scale)));
        assert!(looks.iter().any(|l| l.rate < 0.96) && looks.iter().any(|l| l.rate > 1.04));
        assert!(looks.iter().any(|l| l.scale < 0.98) && looks.iter().any(|l| l.scale > 1.02));
    }

    #[test]
    fn embedded_models_load_and_dress_every_unit_type() {
        let models = load().unwrap();
        let data = crate::data::load();
        for u in data.units {
            assert!(
                models.by_name(u.name()).is_some(),
                "no model for {}",
                u.name()
            );
        }
    }

    #[test]
    fn actions_are_listed_in_order() {
        for (k, a) in ACTIONS.into_iter().enumerate() {
            assert_eq!(a as usize, k, "{a:?}");
        }
    }

    /// The clips the game picked before the move picker, best first: plain
    /// clips must still play exactly these.
    fn old_candidates(action: Action) -> &'static [ClipKind] {
        use ClipKind::*;
        match action {
            Action::Idle => &[Idle],
            Action::Walk => &[Walk, Run, Idle],
            Action::Run => &[Run, Walk, Idle],
            Action::Attack => &[Attack, Attack2, Idle],
            Action::Attack2 => &[Attack2, Attack, Idle],
            Action::Shoot => &[Shoot, Attack, Idle],
            Action::Cheer => &[Cheer, Idle],
            Action::Hit => &[Hit, Idle],
            Action::Block => &[Block, Idle],
        }
    }

    #[test]
    fn the_picker_plays_the_clips_the_game_always_did() {
        let models = load().unwrap();
        let clips = Clips::new(&models);
        let rows = |c: Option<MoveClip>| c.map(|c| (c.first_row, c.frames));
        let old = |m, kinds: &[ClipKind]| {
            kinds
                .iter()
                .find_map(|&k| models.clip(m, k))
                .map(|c| (c.first_row, c.frames))
        };
        for m in 0..models.models.len() {
            for a in ACTIONS {
                assert_eq!(
                    rows(clips.clip(m, a)),
                    old(m, old_candidates(a)),
                    "{m} {a:?}"
                );
            }
            let own = &clips.by_model[m];
            assert_eq!(rows(own.die), old(m, &[ClipKind::Die]));
            assert_eq!(rows(own.parry), old(m, old_candidates(Action::Attack)));
            assert_eq!(rows(own.stagger), old(m, old_candidates(Action::Hit)));
        }
    }

    #[test]
    fn every_model_can_stand_walk_and_fight() {
        let models = load().unwrap();
        let clips = Clips::new(&models);
        for m in 0..models.models.len() {
            for a in ACTIONS {
                assert!(clips.pose(m, a, 1.3).is_some(), "model {m} {a:?}");
            }
        }
    }

    #[test]
    fn duel_moves_play_their_clips() {
        let models = load().unwrap();
        let clips = Clips::new(&models);
        let m = models.by_name("Bone Pikes").unwrap();
        let clip = |k| *models.clip(m, k).unwrap();
        let (die, duck, hit) = (
            clip(ClipKind::Die),
            clip(ClipKind::Block),
            clip(ClipKind::Hit),
        );
        let rows = |f: Move| clips.fight(m, f, 0.0).unwrap();
        // A knockdown falls, lies on the death clip's last frame, and gets
        // up again; it never lays the model flat when it has a death clip.
        let (start, flat) = rows(Move::Down(0.0));
        assert_eq!((start.rows[0], flat), (die.first_row as u16, false));
        let lying = (die.first_row + die.frames - 1) as u16;
        assert_eq!(rows(Move::Down(0.5)).0.rows, [lying; 2]);
        assert_eq!(rows(Move::Down(1.0)).0.rows[0], die.first_row as u16);
        // A block ducks and comes back up; a stagger plays the hit clip.
        assert_eq!(rows(Move::Block(0.0)).0.rows[0], duck.first_row as u16);
        let low = u32::from(rows(Move::Block(0.5)).0.rows[0]);
        assert!(low > duck.first_row && low < duck.first_row + duck.frames / 2);
        assert_eq!(
            rows(Move::Stagger(1.0)).0.rows,
            [(hit.first_row + hit.frames - 1) as u16; 2]
        );

        // A model without a death clip lies flat while knocked down (every
        // model has one now, so take the Skitterlings' away).
        let bug = models.by_name("Skitterlings").unwrap();
        let mut clips = Clips::new(&models);
        clips.by_model[bug].die = None;
        assert!(clips.fight(bug, Move::Down(0.5), 0.0).unwrap().1);
        assert!(!clips.fight(bug, Move::Down(0.05), 0.0).unwrap().1);
    }

    #[test]
    fn loops_wrap_and_deaths_hold() {
        let clip = MoveClip {
            first_row: 10,
            frames: 4,
            fps: 10.0,
            loops: true,
            additive: false,
            mask: gc_assets::anim::Mask::All,
            act: Act::Walk,
            weapon: gc_assets::anim::Weapon::Any,
            side: gc_assets::anim::Side::Any,
        };
        let p = clip.looping(0.35);
        assert_eq!(p.rows, [13, 10]);
        assert!((p.blend - 0.5).abs() < 1e-4);
        assert_eq!(clip.looping(0.4).rows, [10, 11]);
        assert_eq!(clip.looping(-0.05).rows, [13, 10]);

        let models = load().unwrap();
        let clips = Clips::new(&models);
        let m = (0..models.models.len())
            .find(|&m| models.clip(m, ClipKind::Die).is_some())
            .unwrap();
        let die = models.clip(m, ClipKind::Die).unwrap();
        let (end, flat) = clips.death(m, 100.0).unwrap();
        assert_eq!(end.rows, [(die.first_row + die.frames - 1) as u16; 2]);
        assert!(!flat);
    }
}
