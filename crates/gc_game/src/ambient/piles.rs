//! Cut wood beside the halls (docs/34-ambient-life.md, stage 3d): a finished
//! timber or stone hall, outpost, house or workshop may have a stack of logs
//! held by stakes, or a loose heap, against one of its walls. They are scenery models
//! standing where the wood-cutters' work ends up, chosen from a hash of the
//! building, so a town looks the same each time you look at it. Picture
//! only.

use gc_sim::structures::Role;
use glam::{Vec2, Vec3};

use super::bases::hash;
use crate::scenery::{Extra, Model, turn_of};
use crate::siege::{Look, Style};

/// How far outside a wall the middle of a pile stands, in tiles, and how
/// far its ends reach from there along the wall.
const OUTSIDE: f32 = 0.7;
const HALF_LENGTH: f32 = 0.75;
/// How near a pile's middle or ends may come to any building's walls.
pub const CLEAR: f32 = 0.5;
/// The most the ground may differ between a pile's middle and its ends, in
/// tiles: a model stands level at the ground under its middle.
const LEVEL: f32 = 0.15;
/// How far along its wall a pile may stand off the wall's middle, as a
/// share of the wall's half-length.
const OFF_CENTRE: f32 = 0.2;
/// Chance that a house or workshop (not a hall) has a pile at all.
const WORKSHOP_PILE: f32 = 0.6;
/// The most a pile is turned off square to its wall, in turns of a circle.
const ASKEW: f32 = 0.05;

/// The first of the hash salts used here, apart from the chimneys' in
/// `bases.rs`.
const SALT: u32 = 100;

/// What a finished building of this role keeps: a stack of logs and then a
/// heap, as many as there are (the main hall has both).
fn kept(look: &Look) -> (bool, bool) {
    if !look.finished || !matches!(look.stuff.style, Style::Timber | Style::Stone) {
        return (false, false);
    }
    match look.role {
        Role::Main => (true, true),
        Role::Outpost => (false, true),
        Role::Economy | Role::Production | Role::Housing => {
            if hash(look, SALT) >= WORKSHOP_PILE {
                (false, false)
            } else if hash(look, SALT + 1) < 0.5 {
                (true, false)
            } else {
                (false, true)
            }
        }
        _ => (false, false),
    }
}

/// Where each of a building's piles stands: the stack against the wall
/// away from the door (`facing` is the way the door looks), the heap
/// against a side wall (against the back wall if there is no stack), each a
/// little off the wall's middle. Gives the middle of each pile on the map
/// and the way out of the wall there.
fn spots(look: &Look, facing: Vec2, with_pile: bool, with_heap: bool) -> [Option<(Vec2, Vec2)>; 2] {
    let centre = look.centre();
    let half = (look.max - look.min) / 2.0;
    let back = -facing;
    let side = Vec2::new(-facing.y, facing.x)
        * if hash(look, SALT + 2) < 0.5 {
            1.0
        } else {
            -1.0
        };
    let place = |out: Vec2, salt: u32| {
        let at = centre + out * (out.abs().dot(half) + OUTSIDE);
        let tangent = Vec2::new(-out.y, out.x);
        let along = (hash(look, salt) - 0.5) * 2.0 * OFF_CENTRE * half.dot(tangent.abs());
        (at + tangent * along, out)
    };
    [
        with_pile.then(|| place(back, SALT + 3)),
        with_heap.then(|| place(if with_pile { side } else { back }, SALT + 4)),
    ]
}

/// The piles of `look`'s building, whose door looks `facing`, drawn on
/// the ground `ground` gives; `free` says a spot is dry and clear of other
/// buildings. A pile is left out where the ground slopes under it.
pub fn extras(
    look: &Look,
    facing: Vec2,
    ground: impl Fn(Vec2) -> f32,
    free: impl Fn(Vec2) -> bool,
) -> impl Iterator<Item = Extra> {
    let (with_pile, with_heap) = kept(look);
    let mut out = [None, None];
    let models = [Model::LogPile, Model::LogHeap];
    for (i, spot) in spots(look, facing, with_pile, with_heap)
        .into_iter()
        .enumerate()
    {
        let Some((at, outward)) = spot else { continue };
        let along = Vec2::new(-outward.y, outward.x) * HALF_LENGTH;
        let (low, high) = (at - along, at + along);
        let middle = ground(at);
        let level = (ground(low) - middle)
            .abs()
            .max((ground(high) - middle).abs())
            <= LEVEL;
        if !(level && free(at) && free(low) && free(high)) {
            continue;
        }
        let skew = (hash(look, SALT + 5 + i as u32) - 0.5) * 2.0 * ASKEW;
        out[i] = Some(Extra {
            model: models[i],
            at: Vec3::new(at.x, middle, at.y),
            // The model's front faces the wall, so its length runs along it.
            turn: turn_of(-outward).wrapping_add((skew * 256.0) as i32 as u8),
            scale: 0.9 + 0.25 * hash(look, SALT + 7 + i as u32),
            tint: [128; 3],
            phase: 0,
            lean: 0,
            fall: 0,
        });
    }
    out.into_iter().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::siege::Stuff;
    use gc_sim::structures::StructureHandle;

    fn look(role: Role, style: Style, finished: bool, index: u32) -> Look {
        Look {
            handle: StructureHandle {
                index,
                generation: 0,
            },
            min: Vec2::new(10.0, 10.0),
            max: Vec2::new(14.0, 13.0),
            base: 0.0,
            top: 4.0,
            model: None,
            lift: 0.0,
            owner: 0,
            colour: [200; 3],
            stuff: Stuff {
                style,
                colours: [[0; 4]; 3],
                dust: [0; 4],
            },
            role,
            finished,
            hp: 1.0,
        }
    }

    fn piles(look: &Look, free: impl Fn(Vec2) -> bool) -> Vec<Extra> {
        extras(look, Vec2::Y, |_| 2.0, free).collect()
    }

    #[test]
    fn a_finished_timber_hall_has_a_stack_and_a_heap() {
        let all = piles(&look(Role::Main, Style::Timber, true, 1), |_| true);
        let models: Vec<_> = all.iter().map(|e| e.model).collect();
        assert_eq!(models, [Model::LogPile, Model::LogHeap]);
    }

    #[test]
    fn an_outpost_has_a_heap() {
        let all = piles(&look(Role::Outpost, Style::Stone, true, 1), |_| true);
        assert_eq!(
            all.iter().map(|e| e.model).collect::<Vec<_>>(),
            [Model::LogHeap]
        );
    }

    #[test]
    fn nothing_beside_a_building_going_up_or_made_of_bone() {
        assert!(piles(&look(Role::Main, Style::Timber, false, 1), |_| true).is_empty());
        assert!(piles(&look(Role::Main, Style::Bone, true, 1), |_| true).is_empty());
        assert!(piles(&look(Role::Wall, Style::Timber, true, 1), |_| true).is_empty());
    }

    #[test]
    fn piles_lie_along_a_wall_the_stack_behind_the_door_and_the_heap_beside_it() {
        let l = look(Role::Main, Style::Timber, true, 3);
        let all = piles(&l, |_| true);
        assert_eq!(all.len(), 2);
        for e in &all {
            let p = Vec2::new(e.at.x, e.at.z);
            assert!(l.gap(p) > 0.5 && l.gap(p) < 1.0, "gap {}", l.gap(p));
            assert_eq!(e.at.y, 2.0);
        }
        // The door looks +y: the stack is north of the hall, its front
        // facing the wall (north-south, give or take the skew).
        let (stack, heap) = (all[0], all[1]);
        assert!(stack.at.z < l.min.y);
        let front = crate::scenery::facing_of(stack.turn);
        assert!(front.y.abs() > (ASKEW * std::f32::consts::TAU * 1.01).cos());
        // The heap is east or west of the hall, its front facing the wall.
        assert!(heap.at.x < l.min.x || heap.at.x > l.max.x);
        assert!(crate::scenery::facing_of(heap.turn).x.abs() > 0.95);
    }

    #[test]
    fn a_blocked_or_sloping_spot_gets_no_pile_and_the_choice_is_steady() {
        let l = look(Role::Main, Style::Stone, true, 7);
        assert_eq!(piles(&l, |_| true).len(), 2);
        assert_eq!(piles(&l, |_| true), piles(&l, |_| true));
        assert!(piles(&l, |_| false).is_empty());
        // Only one end of a pile off the free ground: no pile either.
        let west = |p: Vec2| p.x > 14.0 || p.y > 13.0;
        assert!(piles(&l, west).len() < 2);
        let slope: Vec<Extra> = extras(&l, Vec2::Y, |p| p.x * 0.5 + p.y * 0.5, |_| true).collect();
        assert!(slope.len() < 2);
    }

    #[test]
    fn houses_and_workshops_have_a_stack_or_a_heap_or_nothing() {
        for role in [Role::Economy, Role::Production, Role::Housing] {
            let models: Vec<Vec<Model>> = (0..60)
                .map(|i| {
                    piles(&look(role, Style::Timber, true, i), |_| true)
                        .iter()
                        .map(|e| e.model)
                        .collect()
                })
                .collect();
            assert!(models.iter().all(|m| m.len() <= 1));
            for want in [vec![], vec![Model::LogPile], vec![Model::LogHeap]] {
                assert!(models.contains(&want), "{role:?} never has {want:?}");
            }
        }
    }
}
