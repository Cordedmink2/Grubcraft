//! Scripted opponents for tests and sparring: no thinking at all, just a
//! fixed habit, so the real minds have something predictable to beat.

use gc_sim::{Command, PlayerCommand, PlayerId, RegimentHandle};

use crate::perception::Perception;

/// Ticks between looks.
const EVERY: u32 = 20;

/// "Charge straight in": every regiment attacks the nearest enemy that is
/// still fighting, and keeps at it. The opponent the battle AI must beat
/// nine times in ten (docs/18-ai.md, AI-1).
pub struct ChargeBot {
    party: PlayerId,
    /// The target each regiment was last sent at, by pool index.
    sent: Vec<Option<(RegimentHandle, RegimentHandle)>>,
}

impl ChargeBot {
    pub fn new(party: PlayerId) -> ChargeBot {
        ChargeBot {
            party,
            sent: Vec::new(),
        }
    }

    pub fn party(&self) -> PlayerId {
        self.party
    }

    /// Append this tick's orders to `out`.
    pub fn tick(&mut self, view: &Perception, out: &mut Vec<PlayerCommand>) {
        if !view.tick.is_multiple_of(EVERY) {
            return;
        }
        for r in view.regiments.iter().filter(|r| r.owner == self.party) {
            if r.broken() || r.engaged {
                continue;
            }
            let target = view
                .regiments
                .iter()
                .filter(|f| view.hostile(self.party, f.owner) && !f.broken())
                .min_by(|a, b| a.pos.distance(r.pos).total_cmp(&b.pos.distance(r.pos)));
            let Some(target) = target else {
                continue;
            };
            let i = r.handle.index as usize;
            if self.sent.len() <= i {
                self.sent.resize(i + 1, None);
            }
            if self.sent[i] == Some((r.handle, target.handle)) {
                continue;
            }
            self.sent[i] = Some((r.handle, target.handle));
            out.push(PlayerCommand {
                player: self.party,
                command: Command::Attack {
                    regiment: r.handle,
                    target: target.handle,
                },
            });
        }
    }
}
