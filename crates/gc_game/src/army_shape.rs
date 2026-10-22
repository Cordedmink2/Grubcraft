//! Which shape your army takes on a group order (docs/08-ui-ux.md, "Group
//! formation"): one of `gc_sim::arrange`'s presets, Line by default, or a
//! shape you saved. The arranging itself is `gc_sim::arrange`.

use std::path::{Path, PathBuf};

use gc_data::conquest::Class;
use gc_fixed::{Angle, Fx};
use gc_platform::logln;
use gc_sim::arrange::{Arrangement, MAX_SLOTS, Preset, Role, Shape, Slot};
use serde::{Deserialize, Serialize};

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

/// Names of the saved slots, as chips and messages show them.
const SAVED_NAMES: [&str; SAVED_SLOTS] = ["Mine 1", "Mine 2", "Mine 3"];

impl Choice {
    /// "Line", "Mine 2" ...
    pub fn label(self) -> &'static str {
        match self {
            Choice::Preset(p) => p.name(),
            Choice::Saved(k) => SAVED_NAMES[k.min(SAVED_SLOTS - 1)],
        }
    }

    /// One line on what it does, for tooltips and messages.
    pub fn tip(self) -> &'static str {
        match self {
            Choice::Preset(p) => p.tip(),
            Choice::Saved(_) => "Your saved shape. Shift+O saves the selection's shape.",
        }
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

    /// Every shape that can be chosen, in the order the selection panel
    /// lists its chips and `O` cycles them: the presets, then the saved
    /// shapes there are.
    pub fn chips(&self) -> Vec<Choice> {
        let saved = (0..SAVED_SLOTS)
            .filter(|&k| self.saved[k].is_some())
            .map(Choice::Saved);
        Preset::ALL
            .iter()
            .map(|&p| Choice::Preset(p))
            .chain(saved)
            .collect()
    }

    /// The next choice after the chosen one, in [`ArmyShapes::chips`] order.
    pub fn next(&self) -> Choice {
        let chips = self.chips();
        let i = chips.iter().position(|&c| c == self.choice).unwrap_or(0);
        chips[(i + 1) % chips.len()]
    }

    /// The slot a new shape goes in: the first empty one, else the chosen
    /// one if it is a saved shape, else the first.
    pub fn slot_for_new(&self) -> usize {
        (self.saved.iter().position(Option::is_none)).unwrap_or(match self.choice {
            Choice::Saved(k) => k,
            Choice::Preset(_) => 0,
        })
    }

    /// Keep `shape` in the slot [`ArmyShapes::slot_for_new`] picks and choose
    /// it. Returns the slot.
    pub fn save(&mut self, shape: Shape) -> usize {
        let k = self.slot_for_new();
        self.saved[k] = Some(shape);
        self.choice = Choice::Saved(k);
        k
    }
}

/// Where your shapes are kept: beside the settings.
pub fn shapes_path() -> Option<PathBuf> {
    gc_ui::settings::app_dir().map(|d| d.join("army_shapes.ron"))
}

/// One regiment's place in a saved shape, as plain numbers.
#[derive(Serialize, Deserialize)]
struct SlotRecord {
    role: u8,
    lateral: i32,
    back: i32,
    turn: u16,
    files: u16,
}

/// The file: which shape was chosen and the saved shapes.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Record {
    /// An index into `Preset::ALL`, or `Preset::ALL.len()` and a slot number
    /// for a saved shape.
    chosen: usize,
    saved: Vec<Option<Vec<SlotRecord>>>,
}

impl ArmyShapes {
    fn to_record(&self) -> Record {
        let chosen = match self.choice {
            Choice::Preset(p) => Preset::ALL.iter().position(|&q| q == p).unwrap_or(0),
            Choice::Saved(k) => Preset::ALL.len() + k,
        };
        let saved = self
            .saved
            .iter()
            .map(|slot| {
                slot.as_ref().map(|shape| {
                    (shape.slots.iter())
                        .map(|s| SlotRecord {
                            role: s.role as u8,
                            lateral: s.lateral.raw(),
                            back: s.back.raw(),
                            turn: s.turn.0,
                            files: s.files,
                        })
                        .collect()
                })
            })
            .collect();
        Record { chosen, saved }
    }

    /// Whatever in `record` makes sense: a hand-edited file loses the parts
    /// that do not, not the rest.
    fn from_record(record: &Record) -> ArmyShapes {
        let mut shapes = ArmyShapes::default();
        for (k, slot) in record.saved.iter().take(SAVED_SLOTS).enumerate() {
            let Some(rows) = slot else {
                continue;
            };
            let slots: Vec<Slot> = rows
                .iter()
                .take(MAX_SLOTS)
                .filter_map(|r| {
                    Some(Slot {
                        role: Role::from_u8(r.role)?,
                        lateral: Fx::from_raw(r.lateral),
                        back: Fx::from_raw(r.back),
                        turn: Angle(r.turn),
                        files: r.files,
                    })
                })
                .collect();
            if slots.len() >= 2 {
                shapes.saved[k] = Some(Shape { slots });
            }
        }
        let presets = Preset::ALL.len();
        shapes.choice = match record.chosen {
            i if i < presets => Choice::Preset(Preset::ALL[i]),
            i if shapes.saved.get(i - presets).is_some_and(Option::is_some) => {
                Choice::Saved(i - presets)
            }
            _ => Choice::default(),
        };
        shapes
    }

    /// Read the shapes kept at `path`; a missing file gives none. A file
    /// that cannot be read or does not parse also gives none, but is first
    /// copied to `army_shapes.ron.broken` and logged (as the settings are),
    /// so the next save doesn't silently wipe the player's shapes.
    pub fn load(path: &Path) -> ArmyShapes {
        match std::fs::read(path) {
            Ok(bytes) => std::str::from_utf8(&bytes)
                .map_err(|e| e.to_string())
                .and_then(|text| ron::from_str::<Record>(text).map_err(|e| e.to_string()))
                .map_or_else(
                    |e| {
                        let mut broken = path.as_os_str().to_owned();
                        broken.push(".broken");
                        let broken = Path::new(&broken);
                        let kept = match gc_platform::fsutil::write_atomic(broken, &bytes) {
                            Ok(()) => format!("a copy is in {}", broken.display()),
                            Err(e) => format!("the copy failed too: {e}"),
                        };
                        logln!(
                            "grubcraft: {} would not load ({e}); no saved army shapes, {kept}",
                            path.display()
                        );
                        ArmyShapes::default()
                    },
                    |record| ArmyShapes::from_record(&record),
                ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ArmyShapes::default(),
            Err(e) => {
                logln!(
                    "grubcraft: cannot read {} ({e}); no saved army shapes",
                    path.display()
                );
                ArmyShapes::default()
            }
        }
    }

    /// Keep the shapes and the choice at `path`; a failure is logged and
    /// the game carries on. `durable` waits for the disk (a new saved
    /// shape); a changed choice only needs the quick write.
    pub fn store(&self, path: &Path, durable: bool) {
        let text = match ron::ser::to_string_pretty(
            &self.to_record(),
            ron::ser::PrettyConfig::default(),
        ) {
            Ok(text) => text,
            Err(e) => {
                logln!("grubcraft: cannot write army shapes ({e})");
                return;
            }
        };
        let written = if durable {
            gc_platform::fsutil::write_durable(path, text.as_bytes())
        } else {
            gc_platform::fsutil::write_atomic(path, text.as_bytes())
        };
        if let Err(e) = written {
            logln!("grubcraft: cannot save {} ({e})", path.display());
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

    fn shape(n: i32) -> Shape {
        Shape {
            slots: (0..n)
                .map(|i| Slot {
                    role: Role::ALL[(i % 5) as usize],
                    lateral: Fx::from_raw(i * 70_001 - 90_000),
                    back: Fx::from_raw(-i * 12_345),
                    turn: Angle(65_000 - i as u16 * 777),
                    files: 3 + i as u16,
                })
                .collect(),
        }
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("gc_army_shapes_{name}_{}", std::process::id()))
    }

    #[test]
    fn saved_shapes_and_the_choice_round_trip_through_a_file() {
        let path = temp("rt");
        let mut shapes = ArmyShapes::default();
        shapes.saved[0] = Some(shape(4));
        shapes.saved[2] = Some(shape(7));
        shapes.choice = Choice::Saved(2);
        shapes.store(&path, true);
        let back = ArmyShapes::load(&path);
        assert_eq!(back.saved, shapes.saved);
        assert_eq!(back.choice, Choice::Saved(2));

        shapes.choice = Choice::Preset(Preset::Hammer);
        shapes.store(&path, true);
        assert_eq!(
            ArmyShapes::load(&path).choice,
            Choice::Preset(Preset::Hammer)
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_missing_or_broken_file_gives_no_shapes() {
        let none = ArmyShapes::load(&temp("missing"));
        assert!(none.saved.iter().all(Option::is_none));
        assert_eq!(none.choice, Choice::default());
        let path = temp("broken");
        std::fs::write(&path, "this is not ron").unwrap();
        assert!(ArmyShapes::load(&path).saved.iter().all(Option::is_none));
        // A choice of an empty slot, a bad role and too many slots are set right.
        std::fs::write(
            &path,
            "(chosen: 6, saved: [Some([(role: 9, lateral: 0, back: 0, turn: 0, files: 1)]), None, None, None, None])",
        )
        .unwrap();
        let odd = ArmyShapes::load(&path);
        assert!(odd.saved.iter().all(Option::is_none));
        assert_eq!(odd.choice, Choice::default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_new_shape_takes_the_first_empty_slot_then_the_chosen_one() {
        let mut shapes = ArmyShapes::default();
        assert_eq!(shapes.save(shape(2)), 0);
        assert_eq!(shapes.save(shape(3)), 1);
        assert_eq!(shapes.save(shape(4)), 2);
        assert_eq!(shapes.choice, Choice::Saved(2));
        // All full: the chosen saved slot is replaced.
        shapes.choice = Choice::Saved(1);
        assert_eq!(shapes.save(shape(5)), 1);
        assert_eq!(shapes.saved[1].as_ref().unwrap().slots.len(), 5);
        assert_eq!(shapes.saved[0].as_ref().unwrap().slots.len(), 2);
        // A preset chosen: the first slot.
        shapes.choice = Choice::Preset(Preset::Deep);
        assert_eq!(shapes.save(shape(6)), 0);
    }

    #[test]
    fn chips_list_the_presets_then_the_saved_shapes_there_are() {
        let mut shapes = ArmyShapes::default();
        assert_eq!(shapes.chips().len(), Preset::ALL.len());
        shapes.saved[1] = Some(shape(2));
        let chips = shapes.chips();
        assert_eq!(chips.len(), Preset::ALL.len() + 1);
        assert_eq!(chips[Preset::ALL.len()], Choice::Saved(1));
        assert_eq!(chips[0], Choice::Preset(Preset::Line));
    }
}
