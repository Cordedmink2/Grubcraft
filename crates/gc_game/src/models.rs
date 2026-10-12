//! Soldier models, baked from glTF by `tools/gc_artbake` into
//! `assets/models.pak` and embedded in the executable, and the choice of
//! which animation frame each soldier shows.

use gc_assets::model::{ClipKind, ClipRecord, Models};
use gc_assets::{Aligned, Pack};

static PACK: &Aligned<[u8]> = &Aligned(*include_bytes!("../../../assets/models.pak"));

/// The embedded models. The pack is checked when the game starts; a failure
/// means the committed pack is broken, so the game runs with boxes instead.
pub fn load() -> Result<Models<'static>, String> {
    let pack = Pack::parse(&PACK.0).map_err(|e| format!("embedded model pack: {e}"))?;
    Models::from_pack(&pack).map_err(|e| format!("embedded model pack: {e}"))
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
}

/// The two bone texture rows to blend between and how far, or `None` when
/// the model lacks every clip that could stand in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub rows: [u16; 2],
    pub blend: f32,
}

/// Clips to try for an action, best first.
fn candidates(action: Action) -> &'static [ClipKind] {
    use ClipKind::*;
    match action {
        Action::Idle => &[Idle],
        Action::Walk => &[Walk, Run, Idle],
        Action::Run => &[Run, Walk, Idle],
        Action::Attack => &[Attack, Attack2, Idle],
        Action::Attack2 => &[Attack2, Attack, Idle],
        Action::Shoot => &[Shoot, Attack, Idle],
        Action::Cheer => &[Cheer, Idle],
    }
}

/// Per model, the clip for each action, looked up once.
pub struct Clips {
    by_model: Vec<[Option<ClipRecord>; 7]>,
    die: Vec<Option<ClipRecord>>,
}

const ACTIONS: [Action; 7] = [
    Action::Idle,
    Action::Walk,
    Action::Run,
    Action::Attack,
    Action::Attack2,
    Action::Shoot,
    Action::Cheer,
];

impl Clips {
    pub fn new(models: &Models<'_>) -> Clips {
        let by_model = (0..models.models.len())
            .map(|m| {
                ACTIONS.map(|a| {
                    candidates(a)
                        .iter()
                        .find_map(|&k| models.clip(m, k))
                        .copied()
                })
            })
            .collect();
        let die = (0..models.models.len())
            .map(|m| models.clip(m, ClipKind::Die).copied())
            .collect();
        Clips { by_model, die }
    }

    /// The pose for `action` `seconds` into a looping clip.
    pub fn pose(&self, model: usize, action: Action, seconds: f32) -> Option<Pose> {
        let slot = ACTIONS.iter().position(|&a| a == action)?;
        let clip = self.by_model.get(model)?[slot]?;
        Some(looping(&clip, seconds))
    }

    /// The pose `seconds` after falling: the death clip, held on its last
    /// frame. `None` when the model has no death clip (the renderer lays the
    /// idle pose down instead).
    pub fn death(&self, model: usize, seconds: f32) -> Option<Pose> {
        let clip = (*self.die.get(model)?)?;
        let f = (seconds.max(0.0) * clip.fps).min((clip.frames - 1) as f32);
        let a = f.floor() as u32;
        let b = (a + 1).min(clip.frames - 1);
        Some(Pose {
            rows: [(clip.first_row + a) as u16, (clip.first_row + b) as u16],
            blend: f - a as f32,
        })
    }
}

fn looping(clip: &ClipRecord, seconds: f32) -> Pose {
    let f = (seconds * clip.fps).rem_euclid(clip.frames as f32);
    let a = (f.floor() as u32).min(clip.frames - 1);
    let b = (a + 1) % clip.frames;
    Pose {
        rows: [(clip.first_row + a) as u16, (clip.first_row + b) as u16],
        blend: f - a as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_models_load_and_dress_every_unit_type() {
        let models = load().unwrap();
        let data = crate::data::load();
        for u in data.units.iter() {
            assert!(
                models.by_name(u.name()).is_some(),
                "no model for {}",
                u.name()
            );
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
    fn loops_wrap_and_deaths_hold() {
        let clip = ClipRecord {
            kind: ClipKind::Walk as u8,
            _pad: [0; 3],
            first_row: 10,
            frames: 4,
            fps: 10.0,
        };
        let p = looping(&clip, 0.35);
        assert_eq!(p.rows, [13, 10]);
        assert!((p.blend - 0.5).abs() < 1e-4);
        assert_eq!(looping(&clip, 0.4).rows, [10, 11]);
        assert_eq!(looping(&clip, -0.05).rows, [13, 10]);

        let models = load().unwrap();
        let clips = Clips::new(&models);
        let m = (0..models.models.len())
            .find(|&m| models.clip(m, ClipKind::Die).is_some())
            .unwrap();
        let die = models.clip(m, ClipKind::Die).unwrap();
        let end = clips.death(m, 100.0).unwrap();
        assert_eq!(end.rows, [(die.first_row + die.frames - 1) as u16; 2]);
    }
}
