//! Which shape your army takes on a group order (docs/08-ui-ux.md, "Group
//! formation"): one of `gc_sim::arrange`'s presets, Line by default, or a
//! shape you saved. The arranging itself is `gc_sim::arrange`.

use gc_data::conquest::Class;
use gc_sim::arrange::{Arrangement, Preset, Role, Shape};

/// How many shapes of your own you can keep.
pub const SAVED_SLOTS: usize = 3;

/// Where a regiment of `class` stands in an arranged army.
pub fn role_of(class: Class) -> Role {
    match class {
        Class::Light | Class::Spear | Class::Heavy | Class::Monster => Role::Melee,
        Class::Ranged => Role::Ranged,
        Class::Cavalry | Class::Air => Role::Flanker,
        Class::Support => Role::Support,
        Class::Worker | Class::Artillery | Class::Siege => Role::Rear,
    }
}

/// The shape chosen for group orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Preset(Preset),
    /// One of your saved shapes, by slot.
    Saved(usize),
}

impl Default for Choice {
    fn default() -> Choice {
        Choice::Preset(Preset::Line)
    }
}

/// The chosen shape and the shapes you saved.
#[derive(Clone, Debug, Default)]
pub struct ArmyShapes {
    pub choice: Choice,
    pub saved: [Option<Shape>; SAVED_SLOTS],
}

impl ArmyShapes {
    /// How the next group order is arranged. A chosen slot that is empty
    /// arranges as a Line.
    pub fn arrangement(&self) -> Arrangement<'_> {
        match self.choice {
            Choice::Preset(p) => Arrangement::Preset(p),
            Choice::Saved(k) => match self.saved.get(k).and_then(Option::as_ref) {
                Some(shape) => Arrangement::Saved(shape),
                None => Arrangement::Preset(Preset::Line),
            },
        }
    }

    /// The chosen shape's name: "Line", "Mine 2" ...
    pub fn name(&self) -> String {
        match self.choice {
            Choice::Preset(p) => p.name().to_string(),
            Choice::Saved(k) => format!("Mine {}", k + 1),
        }
    }

    /// The next choice: the presets, then the saved shapes there are.
    pub fn next(&self) -> Choice {
        let saved = |from: usize| {
            (from..SAVED_SLOTS)
                .find(|&k| self.saved[k].is_some())
                .map(Choice::Saved)
        };
        match self.choice {
            Choice::Preset(p) if p.next() != Preset::ALL[0] => Choice::Preset(p.next()),
            Choice::Preset(_) => saved(0).unwrap_or_default(),
            Choice::Saved(k) => saved(k + 1).unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycling_visits_presets_then_saved_shapes() {
        let mut shapes = ArmyShapes::default();
        shapes.saved[1] = Some(Shape::default());
        let mut seen = Vec::new();
        for _ in 0..Preset::ALL.len() + 2 {
            seen.push(shapes.choice);
            shapes.choice = shapes.next();
        }
        let mut want: Vec<Choice> = Preset::ALL.iter().map(|&p| Choice::Preset(p)).collect();
        want.push(Choice::Saved(1));
        want.push(Choice::Preset(Preset::Line));
        assert_eq!(seen, want);
    }

    #[test]
    fn every_class_has_a_place() {
        assert_eq!(role_of(Class::Cavalry), Role::Flanker);
        assert_eq!(role_of(Class::Ranged), Role::Ranged);
        assert_eq!(role_of(Class::Spear), Role::Melee);
        assert_eq!(role_of(Class::Artillery), Role::Rear);
    }
}
