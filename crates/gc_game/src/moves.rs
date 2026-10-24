//! The move picker (docs/33-animation-engine.md): the fight asks for a move
//! by what it is for (an attack from the left with a spear, a hit from
//! behind) and gets the best one the model has, falling back to plainer ones
//! (a parry to a block, a stagger to a hit) and in the end to the model's
//! plain clips. A model offers its own clips, by kind, and when fitted to a
//! shared skeleton also that skeleton's tagged moves.

use gc_assets::anim::{Act, Mask, MoveRecord, NO_KIND, Side, Weapon};
use gc_assets::model::{ClipRecord, Models};
use gc_render::{LAYER_ADDITIVE, PoseLayer};

use crate::models::Pose;

/// One move a model can play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveClip {
    pub first_row: u32,
    pub frames: u32,
    pub fps: f32,
    pub loops: bool,
    /// Stored as changes from its first frame, to play on top of another.
    pub additive: bool,
    pub mask: Mask,
    pub act: Act,
    pub weapon: Weapon,
    pub side: Side,
}

impl MoveClip {
    /// A model's own clip, as the move its kind stands for.
    fn of_clip(c: &ClipRecord) -> Option<MoveClip> {
        let kind = c.kind()?;
        Some(MoveClip {
            first_row: c.first_row,
            frames: c.frames,
            fps: c.fps,
            loops: kind.loops(),
            additive: false,
            mask: Mask::All,
            act: kind.act(),
            weapon: Weapon::Any,
            side: Side::Any,
        })
    }

    pub(crate) fn of_move(r: &MoveRecord) -> MoveClip {
        MoveClip {
            first_row: r.first_row,
            frames: r.frames,
            fps: r.fps,
            loops: r.loops(),
            additive: r.additive(),
            mask: r.mask(),
            act: r.act(),
            weapon: r.weapon(),
            side: r.side(),
        }
    }

    /// The pose `seconds` in, round and round whether or not the move is
    /// made to loop.
    pub fn looping(&self, seconds: f32) -> Pose {
        let f = (seconds * self.fps).rem_euclid(self.frames as f32);
        let a = (f.floor() as u32).min(self.frames - 1);
        self.pose(a, (a + 1) % self.frames, f - a as f32)
    }

    /// How long it plays, in seconds.
    pub fn seconds(&self) -> f32 {
        self.frames as f32 / self.fps
    }

    /// The pose `fraction` (0 to 1) of the way through, whatever its length.
    pub fn at(&self, fraction: f32) -> Pose {
        self.at_frame(fraction * (self.frames - 1) as f32)
    }

    /// The pose at `frame`, held on the first and last frames outside it.
    pub fn at_frame(&self, frame: f32) -> Pose {
        let f = frame.clamp(0.0, (self.frames - 1) as f32);
        let a = f.floor() as u32;
        self.pose(a, (a + 1).min(self.frames - 1), f - a as f32)
    }

    fn pose(&self, a: u32, b: u32, blend: f32) -> Pose {
        Pose {
            rows: [(self.first_row + a) as u16, (self.first_row + b) as u16],
            blend,
        }
    }

    /// This move at `pose` as a layer on top of a soldier's base move,
    /// showing `weight` (0 to 1) of it.
    pub fn layer(&self, pose: Pose, weight: f32) -> PoseLayer {
        PoseLayer {
            frames: pose.rows,
            blend: (pose.blend * 255.0) as u8,
            weight: (weight.clamp(0.0, 1.0) * 255.0) as u8,
            mask: self.mask as u8,
            flags: if self.additive { LAYER_ADDITIVE } else { 0 },
        }
    }
}

/// What the fight wants a soldier to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Want {
    pub act: Act,
    pub weapon: Weapon,
    pub side: Side,
    /// Picks among equally good moves, so a regiment does not move as one.
    pub seed: u32,
    /// A move to lay on top of the base one (an additive flinch, an arm's
    /// swing): only layered moves of this very act, with no fallbacks. A
    /// base move is never a layered one.
    pub layer: bool,
}

impl Want {
    pub fn new(act: Act) -> Want {
        Want {
            act,
            weapon: Weapon::Any,
            side: Side::Any,
            seed: 0,
            layer: false,
        }
    }
}

/// The acts to try for `act`, best first.
fn fallbacks(act: Act) -> &'static [Act] {
    use Act::*;
    match act {
        Idle => &[Idle],
        Walk => &[Walk, Run, Idle],
        Run => &[Run, Walk, Idle],
        Guard => &[Guard, Idle],
        Attack => &[Attack, Idle],
        // A parry meets the blow with the weapon, as an attack raises it.
        Parry => &[Parry, Attack, Idle],
        Block => &[Block, Parry, Idle],
        Hit => &[Hit, Idle],
        Stagger => &[Stagger, Hit, Idle],
        Knockdown => &[Knockdown, Death],
        GetUp => &[GetUp],
        Death => &[Death],
        Cheer => &[Cheer, Idle],
        Shoot => &[Shoot, Attack, Idle],
        Step => &[Step, Walk, Idle],
        Flinch => &[Flinch, Hit],
        // A half of a matched kill means nothing alone: see Clips::kill_pair.
        Kill => &[Kill],
        Killed => &[Killed],
    }
}

/// How well a tag fits what is wanted: the very one 2, one made for
/// anything 1, another `None` (never picked). Wanting `any` takes only moves
/// made for anything: a soldier whose weapon the fight does not name never
/// plays a spear's parry from another model on his skeleton.
pub(crate) fn fits<T: PartialEq>(have: T, want: T, any: T) -> Option<u32> {
    if have == want {
        Some(2)
    } else if have == any {
        Some(1)
    } else {
        None
    }
}

/// Of the `items` that `score` takes, one of those scoring best, picked by
/// `seed` among them.
pub(crate) fn best_by_seed<T>(
    items: &[T],
    score: impl Fn(&T) -> Option<u32>,
    seed: u32,
) -> Option<&T> {
    let best = items.iter().filter_map(&score).max()?;
    let top = || items.iter().filter(|i| score(i) == Some(best));
    top().nth(seed as usize % top().count())
}

/// Every model's moves, looked up once.
pub struct Moves {
    by_model: Vec<Vec<MoveClip>>,
}

impl Moves {
    /// Each model's own clips, by kind, and for a model fitted to a shared
    /// skeleton also the skeleton's tagged moves (those no clip kind stands
    /// for): the library is shared, but a model keeps its own walk and
    /// idle, which another model on the same rig may hold differently.
    pub fn new(models: &Models<'_>) -> Moves {
        let anim = &models.anim;
        let by_model = (0..models.models.len())
            .map(|m| {
                let own = models.clips_of(m).iter().filter_map(MoveClip::of_clip);
                let library = anim.fit(m).into_iter().flat_map(|fit| {
                    anim.moves_of(fit.skeleton as usize)
                        .filter(|(_, r)| r.kind == NO_KIND)
                        .map(|(_, r)| MoveClip::of_move(r))
                });
                own.chain(library).collect()
            })
            .collect();
        Moves { by_model }
    }

    /// Every move `model` has.
    pub fn of(&self, model: usize) -> &[MoveClip] {
        self.by_model.get(model).map_or(&[], Vec::as_slice)
    }

    /// The best move `model` has for `want`: the first act along its
    /// fallbacks that the model has a move for, then the move whose weapon
    /// and side fit best, then the seed among equals. Layered moves (an
    /// additive flinch, a move on part of the body) are only picked when
    /// `want.layer` asks for one, and then only of its very act.
    pub fn pick(&self, model: usize, want: Want) -> Option<MoveClip> {
        let moves = self.of(model);
        let acts = if want.layer {
            core::slice::from_ref(&want.act)
        } else {
            fallbacks(want.act)
        };
        for &act in acts {
            let score = |m: &MoveClip| {
                let layered = m.additive || m.mask != Mask::All;
                if m.act != act || layered != want.layer {
                    return None;
                }
                let w = fits(m.weapon, want.weapon, Weapon::Any)?;
                let s = fits(m.side, want.side, Side::Any)?;
                Some(w * 3 + s)
            };
            if let Some(m) = best_by_seed(moves, score, want.seed) {
                return Some(*m);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(act: Act, weapon: Weapon, side: Side, row: u32) -> MoveClip {
        MoveClip {
            first_row: row,
            frames: 10,
            fps: 12.0,
            loops: false,
            additive: false,
            mask: Mask::All,
            act,
            weapon,
            side,
        }
    }

    fn moves(list: Vec<MoveClip>) -> Moves {
        Moves {
            by_model: vec![list],
        }
    }

    #[test]
    fn the_best_fitting_move_is_picked() {
        let m = moves(vec![
            clip(Act::Attack, Weapon::Any, Side::Any, 0),
            clip(Act::Attack, Weapon::Spear, Side::Any, 10),
            clip(Act::Attack, Weapon::Spear, Side::Left, 20),
            clip(Act::Attack, Weapon::SwordShield, Side::Left, 30),
        ]);
        let want = |weapon, side| Want {
            weapon,
            side,
            ..Want::new(Act::Attack)
        };
        let row = |w| m.pick(0, w).unwrap().first_row;
        assert_eq!(row(want(Weapon::Spear, Side::Left)), 20);
        assert_eq!(row(want(Weapon::Spear, Side::Right)), 10);
        assert_eq!(row(want(Weapon::TwoHanded, Side::Front)), 0);
        assert_eq!(row(want(Weapon::SwordShield, Side::Left)), 30);
        // Naming no weapon or side takes only moves made for any.
        assert_eq!(row(Want::new(Act::Attack)), 0);
        let spear_only = moves(vec![clip(Act::Attack, Weapon::Spear, Side::Any, 10)]);
        assert_eq!(spear_only.pick(0, Want::new(Act::Attack)), None);
    }

    #[test]
    fn missing_acts_fall_back_and_seeds_vary_the_pick() {
        let m = moves(vec![
            clip(Act::Hit, Weapon::Any, Side::Any, 0),
            clip(Act::Hit, Weapon::Any, Side::Any, 10),
            clip(Act::Death, Weapon::Any, Side::Any, 20),
        ]);
        let rows: Vec<u32> = (0..4)
            .map(|seed| {
                m.pick(
                    0,
                    Want {
                        seed,
                        ..Want::new(Act::Stagger)
                    },
                )
                .unwrap()
                .first_row
            })
            .collect();
        assert_eq!(rows, vec![0, 10, 0, 10]);
        assert_eq!(m.pick(0, Want::new(Act::Knockdown)).unwrap().first_row, 20);
        assert_eq!(m.pick(0, Want::new(Act::GetUp)), None);
    }

    #[test]
    fn layered_moves_are_only_picked_as_layers() {
        let mut flinch = clip(Act::Hit, Weapon::Any, Side::Any, 30);
        flinch.additive = true;
        let m = moves(vec![flinch, clip(Act::Death, Weapon::Any, Side::Any, 0)]);
        // No base move is a layered one, even of the act asked for.
        assert_eq!(m.pick(0, Want::new(Act::Stagger)), None);
        assert_eq!(m.pick(0, Want::new(Act::Hit)), None);
        let layer = |act| Want {
            layer: true,
            ..Want::new(act)
        };
        assert_eq!(m.pick(0, layer(Act::Hit)).unwrap().first_row, 30);
        // A layer has no fallbacks, and a base move is no layer.
        assert_eq!(m.pick(0, layer(Act::Stagger)), None);
        assert_eq!(m.pick(0, layer(Act::Death)), None);
    }

    #[test]
    fn halves_of_a_kill_are_never_picked_in_place_of_plainer_moves() {
        let m = moves(vec![
            clip(Act::Kill, Weapon::Any, Side::Any, 0),
            clip(Act::Killed, Weapon::Any, Side::Any, 10),
        ]);
        for act in [Act::Attack, Act::Hit, Act::Death, Act::Knockdown] {
            assert_eq!(m.pick(0, Want::new(act)), None, "{act:?}");
        }
    }

    #[test]
    fn moves_loop_or_hold_their_ends() {
        let c = clip(Act::Walk, Weapon::Any, Side::Any, 100);
        let p = c.looping(10.5 / 12.0);
        assert_eq!(p.rows, [100, 101]);
        assert!((p.blend - 0.5).abs() < 1e-4);
        assert_eq!(c.at_frame(50.0).rows, [109, 109]);
        assert_eq!(c.at(0.5).rows, [104, 105]);
    }
}
