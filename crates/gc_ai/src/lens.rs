//! The AI lens (docs/18-ai.md, "Testing, tuning and explaining"): what a
//! mind is thinking, laid out for the game to draw over the field.
//!
//! On the field: the front its Captain wants (with the end it has anchored
//! on ground nobody crosses), every regiment's role, who each is going for
//! or the place it is heading to, and the enemy regiment it has picked as
//! its point of attack. In the panel: its plan, what it expects from the
//! fight now against what it expected at the start, its ground, its roles,
//! the orders it has sent, and the latest entries in its journal.
//!
//! Only a picture of decisions already made: nothing here changes them.

use gc_sim::{PlayerId, RegimentHandle};
use glam::Vec2;

use crate::Mind;
use crate::captain::{Role, describe_margin};
use crate::perception::{Perception, TICKS};

/// Journal entries the panel shows, newest last.
pub const JOURNAL_LINES: usize = 3;

/// Where the Captain wants its line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Front {
    /// The middle of its front edge.
    pub centre: Vec2,
    /// The way it faces.
    pub forward: Vec2,
    /// Tiles from end to end.
    pub width: f32,
    /// The end anchored on ground nobody crosses: -1 the left, +1 the
    /// right, 0 neither.
    pub covered: f32,
}

/// One of the mind's regiments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub regiment: RegimentHandle,
    pub role: Role,
    /// The enemy it was last told to attack, while that enemy stands.
    pub target: Option<RegimentHandle>,
    /// Its place in the formation.
    pub place: Option<Vec2>,
}

/// Everything the lens shows for one mind.
#[derive(Clone, Debug, PartialEq)]
pub struct Lens {
    pub party: PlayerId,
    /// None before the battle has started.
    pub front: Option<Front>,
    pub marks: Vec<Mark>,
    /// The point of attack.
    pub focus: Option<RegimentHandle>,
    /// The panel, one line each, without the player's name.
    pub text: Vec<String>,
}

impl Mind {
    /// What the lens shows for this mind, against the field in `view`.
    pub fn lens(&self, view: &Perception) -> Lens {
        let c = self.captain();
        let standing = |h: RegimentHandle| view.get(h).is_some_and(|r| !r.broken());
        let marks = c
            .members
            .iter()
            .filter(|m| standing(m.handle))
            .map(|m| Mark {
                regiment: m.handle,
                role: m.role,
                target: m.target.filter(|&t| standing(t)),
                place: c.place_of(m.handle),
            })
            .collect::<Vec<_>>();
        let front = c.front().map(|(centre, forward, width, covered)| Front {
            centre,
            forward,
            width,
            covered,
        });
        Lens {
            party: self.party(),
            front,
            focus: c.focus.filter(|&f| standing(f)),
            text: self.lens_text(&marks),
            marks,
        }
    }

    fn lens_text(&self, marks: &[Mark]) -> Vec<String> {
        let c = self.captain();
        let mut text = vec![self.summary()];
        let odds = match (c.estimate, c.first_estimate) {
            (Some(now), Some(start)) => format!(
                "expects {} ({:+.2}), at the start {} ({:+.2})",
                describe_margin(now.margin),
                now.margin,
                describe_margin(start.margin),
                start.margin
            ),
            _ => "not weighed up yet".to_string(),
        };
        let ground = if c.waiting_behind_water() {
            "waiting behind water for them to cross".to_string()
        } else {
            match c.front().map(|f| f.3) {
                Some(s) if s != 0.0 => format!(
                    "{} end anchored on ground nobody crosses",
                    if s > 0.0 { "right" } else { "left" }
                ),
                Some(_) => "open ground".to_string(),
                None => "not formed up yet".to_string(),
            }
        };
        text.push(format!("  odds    {odds}   ground: {ground}"));
        let mut roles: Vec<(Role, usize)> = Vec::new();
        for m in marks {
            match roles.iter_mut().find(|(r, _)| *r == m.role) {
                Some((_, n)) => *n += 1,
                None => roles.push((m.role, 1)),
            }
        }
        let roles: Vec<String> = roles
            .iter()
            .map(|(r, n)| format!("{n} {}", r.name()))
            .collect();
        text.push(format!(
            "  roles   {}   orders: {} sent, {} a minute at most",
            roles.join(", "),
            self.orders().count(),
            self.knobs().orders_per_minute
        ));
        let entries: Vec<_> = self.journal().entries().rev().take(JOURNAL_LINES).collect();
        for e in entries.iter().rev() {
            text.push(format!("  {:<7} {}", clock(e.tick), e.text));
        }
        text
    }
}

/// A tick as minutes and seconds into the match.
pub fn clock(tick: u32) -> String {
    let s = tick / TICKS as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_minutes_and_seconds() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(75 * gc_sim::TICKS_PER_SECOND), "1:15");
    }
}
