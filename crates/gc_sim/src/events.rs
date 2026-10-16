//! What happened in the last tick, told by the simulation itself (review
//! fix T2), so sounds, soldiers falling, shooting marks and alerts don't
//! each work it out by comparing what they saw before with what they see
//! now. Events are not part of the match's state (not hashed, not in a
//! snapshot): a viewer that skips ticks, catching up or seeking, simply
//! doesn't read theirs.

use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::combat::Combat;
use crate::morale::MoraleState;
use crate::regiments::{RegimentHandle, Regiments};
use crate::vision::Ambush;

/// One volley, from when it is loosed until it comes down: the pool slot it
/// flies in and the tick it was loosed on (a slot is reused once it lands).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VolleyId {
    pub slot: u32,
    pub launch_tick: u32,
}

/// Something that happened in the last tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `shooter` loosed a volley at `target`.
    Loosed {
        volley: VolleyId,
        shooter: RegimentHandle,
        target: RegimentHandle,
    },
    /// A volley came down where it was aimed, and `lost` of its target's
    /// soldiers fell to it (none if the target had walked out from under
    /// it, or was gone).
    Landed {
        volley: VolleyId,
        target: RegimentHandle,
        at: FxVec2,
        lost: u16,
    },
    /// A regiment lost soldiers this tick, to anything (`lost` is all it
    /// had when it is wiped out; its handle is no longer live then).
    Losses { regiment: RegimentHandle, lost: u16 },
    /// A regiment's heart changed: it wavered, broke, rallied or shattered.
    Morale {
        regiment: RegimentHandle,
        from: MoraleState,
        to: MoraleState,
    },
    /// A regiment met an enemy in melee, having fought nobody last tick.
    Contact {
        regiment: RegimentHandle,
        foe: RegimentHandle,
    },
    /// An ambush was sprung.
    Ambush(Ambush),
}

/// Each regiment slot as it stood before the tick.
#[derive(Clone, Copy)]
struct Before {
    alive: bool,
    generation: u16,
    models: u16,
    morale: MoraleState,
    engaged: bool,
}

/// The last tick's events, with all their room reserved when the match
/// starts.
pub(crate) struct Events {
    list: Vec<Event>,
    before: Vec<Before>,
}

impl Events {
    pub(crate) fn new(capacity: usize) -> Events {
        Events {
            // A volley's two events, and each regiment's four at most.
            list: Vec::with_capacity(2 * crate::combat::MAX_VOLLEYS + 4 * capacity),
            before: alloc::vec![
                Before {
                    alive: false,
                    generation: 0,
                    models: 0,
                    morale: MoraleState::Steady,
                    engaged: false,
                };
                capacity
            ],
        }
    }

    /// No room at all, for a world that tells no one (a rehearsal's).
    pub(crate) fn none() -> Events {
        Events {
            list: Vec::new(),
            before: Vec::new(),
        }
    }

    pub(crate) fn list(&self) -> &[Event] {
        &self.list
    }

    /// Forget the last tick's events (a tick with nothing played, such as
    /// one spent deploying, has none).
    pub(crate) fn clear(&mut self) {
        self.list.clear();
    }

    /// Note each regiment as it stands before the tick is played (after
    /// its commands, so regiments placed or removed by them aren't told).
    pub(crate) fn before(&mut self, regs: &Regiments) {
        for (i, b) in self.before.iter_mut().enumerate() {
            *b = Before {
                alive: regs.alive[i],
                generation: regs.generation[i],
                models: regs.models[i],
                morale: regs.morale_state[i],
                engaged: regs.engaged[i].is_some(),
            };
        }
    }

    /// Tell what the tick just played did.
    pub(crate) fn after(&mut self, regs: &Regiments, combat: &Combat, ambushes: &[Ambush]) {
        let list = &mut self.list;
        list.extend(
            combat
                .loosed()
                .iter()
                .map(|&(volley, shooter, target)| Event::Loosed {
                    volley,
                    shooter,
                    target,
                }),
        );
        list.extend_from_slice(combat.landed());
        for (i, b) in self.before.iter().enumerate() {
            if !b.alive {
                continue;
            }
            let regiment = RegimentHandle {
                index: i as u32,
                generation: b.generation,
            };
            // Gone: wiped out.
            let still = regs.alive[i] && regs.generation[i] == b.generation;
            let models = if still { regs.models[i] } else { 0 };
            if models < b.models {
                list.push(Event::Losses {
                    regiment,
                    lost: b.models - models,
                });
            }
            if !still {
                continue;
            }
            if regs.morale_state[i] != b.morale {
                list.push(Event::Morale {
                    regiment,
                    from: b.morale,
                    to: regs.morale_state[i],
                });
            }
            if let Some(foe) = regs.engaged[i]
                && !b.engaged
            {
                list.push(Event::Contact { regiment, foe });
            }
        }
        list.extend(ambushes.iter().copied().map(Event::Ambush));
    }
}
