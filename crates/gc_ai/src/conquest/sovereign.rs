//! The Sovereign (docs/18-ai.md, "Sovereign: strategy"): the big plans
//! above the economy and the army. In a game of several rival sides it
//! picks the enemy to beat first, the leader or one that has attacked it,
//! and the Marshal goes for that enemy and leaves the others be; a level
//! that plans timing pushes picks when to go with everything, and the
//! Steward arms for it beforehand while the Marshal holds the army back.

use gc_sim::TICKS_PER_SECOND;
use gc_sim::structures::Role;

use super::Sense;
use super::marshal::enemy_buildings;
use super::spymaster;
use crate::personality::Personality;

const THINK_TICKS: u32 = 3 * TICKS_PER_SECOND;
/// How far ahead a rival's standing must be of its focus's to take its
/// place: it doesn't switch on a whim.
const SWITCH: f32 = 1.25;
/// A rival's standing for each hall and each other building of its it has
/// seen; its army counts its supply.
const HALL_STANDING: f32 = 12.0;
const BUILDING_STANDING: f32 = 3.0;
/// How near its buildings an enemy's soldiers earn a grudge, tiles.
const GRUDGE_REACH: f32 = 25.0;
/// The supply of an enemy's soldiers at its buildings that earns a grudge.
const GRUDGE_SUPPLY: f32 = 6.0;
/// How long the most vengeful keeps a grudge, ticks; the least, a third.
const GRUDGE_TICKS: u32 = 4 * 60 * TICKS_PER_SECOND;
/// How long before a push it arms and holds the army back, ticks.
pub const PUSH_LEAD: u32 = 100 * TICKS_PER_SECOND;
/// How long after a push's time the army may still set out on it, ticks.
pub const PUSH_WINDOW: u32 = 30 * TICKS_PER_SECOND;
/// The first push, minutes: the most aggressive, the least.
const FIRST_PUSH: (f32, f32) = (7.0, 10.0);
/// Between one push and the next, ticks.
const PUSH_EVERY: u32 = 6 * 60 * TICKS_PER_SECOND;

/// Why it goes for an enemy first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// The rival furthest ahead, that all the others have to stop.
    Leader,
    /// It attacked its buildings.
    Grudge,
}

#[derive(Default)]
pub struct Sovereign {
    next_think: u32,
    /// The enemy it goes for first, and why.
    pub focus: Option<(u8, Why)>,
    /// Enemies whose soldiers came at its buildings, and when last.
    grudges: Vec<(u8, u32)>,
    /// When its next timing push sets out.
    pub push: Option<u32>,
}

impl Sovereign {
    /// Think at its pace, and say what it has decided that is new.
    pub fn think(
        &mut self,
        s: &Sense,
        persona: &Personality,
        name: impl Fn(u8) -> String,
    ) -> Vec<String> {
        let mut notes = Vec::new();
        if s.now < self.next_think {
            return notes;
        }
        self.next_think = s.now + THINK_TICKS;
        self.plan_push(s, persona, &mut notes);
        self.remember_grudges(s, persona);
        let focus = if s.plan.gangs_up {
            self.choose_focus(s)
        } else {
            None
        };
        if focus != self.focus
            && let Some((o, why)) = focus
        {
            notes.push(match why {
                Why::Leader => format!("{} leads: going for them first", name(o)),
                Why::Grudge => format!("{} attacked us: going for them first", name(o)),
            });
        }
        self.focus = focus;
        notes
    }

    /// The timing push: the first by its aggression, then one every few
    /// minutes once the last has had its chance.
    fn plan_push(&mut self, s: &Sense, persona: &Personality, notes: &mut Vec<String>) {
        if !s.plan.pushes {
            self.push = None;
            return;
        }
        let next = match self.push {
            None => {
                let (soonest, latest) = FIRST_PUSH;
                let minutes = latest - (latest - soonest) * persona.aggression();
                (minutes * 60.0 * TICKS_PER_SECOND as f32) as u32
            }
            Some(t) if s.now >= t + PUSH_WINDOW => t + PUSH_EVERY,
            Some(_) => return,
        };
        self.push = Some(next);
        notes.push(format!(
            "Planning a timing push at {}",
            crate::lens::clock(next)
        ));
    }

    /// Note the enemies whose soldiers are at its buildings, and forget
    /// grudges past its vengefulness.
    fn remember_grudges(&mut self, s: &Sense, persona: &Personality) {
        let vengeful = f32::from(persona.vengefulness) / 100.0;
        let keep = GRUDGE_TICKS / 3 + (GRUDGE_TICKS as f32 * 2.0 / 3.0 * vengeful) as u32;
        self.grudges.retain(|&(_, t)| s.now < t + keep);
        let Some(st) = s.world.structures() else {
            return;
        };
        let mine: Vec<_> = st
            .iter()
            .filter(|v| v.owner == s.party)
            .map(|v| crate::perception::v2(v.centre()))
            .collect();
        // Soldiers in sight now, not workers, and enough of them to be an
        // attack rather than a scout passing.
        let mut at_us: Vec<(u8, f32)> = Vec::new();
        for r in s.view.regiments.iter().filter(|r| {
            s.view.hostile(s.party, r.owner)
                && !s.kit.all_workers.contains(&r.unit)
                && mine.iter().any(|b| b.distance(r.pos) <= GRUDGE_REACH)
        }) {
            let supply = s.kit.supply_of(std::iter::once(r));
            match at_us.iter_mut().find(|(o, _)| *o == r.owner) {
                Some(a) => a.1 += supply,
                None => at_us.push((r.owner, supply)),
            }
        }
        for (o, _) in at_us.into_iter().filter(|&(_, n)| n >= GRUDGE_SUPPLY) {
            match self.grudges.iter_mut().find(|(g, _)| *g == o) {
                Some(g) => g.1 = s.now,
                None => self.grudges.push((o, s.now)),
            }
        }
    }

    /// Whom to go for first: in a game of one rival side, nobody in
    /// particular; else the latest grudge, else the rival furthest ahead
    /// by what it has seen, keeping its focus unless another is clearly
    /// further.
    fn choose_focus(&self, s: &Sense) -> Option<(u8, Why)> {
        let rivals = rivals(s);
        let teams = s.world.stances().teams();
        let team = |p: u8| teams.get(usize::from(p)).copied();
        let mut sides: Vec<_> = rivals.iter().map(|&p| team(p)).collect();
        sides.sort_unstable();
        sides.dedup();
        if sides.len() < 2 {
            return None;
        }
        // A grudge first: the one it holds already while it lasts, else the
        // latest.
        let held = |o: u8| rivals.contains(&o) && self.grudges.iter().any(|g| g.0 == o);
        if let Some((o, Why::Grudge)) = self.focus
            && held(o)
        {
            return self.focus;
        }
        if let Some(&(o, _)) = self
            .grudges
            .iter()
            .filter(|g| held(g.0))
            .max_by_key(|g| g.1)
        {
            return Some((o, Why::Grudge));
        }
        let standing = standings(s, &rivals);
        let (best, top) = standing
            .iter()
            .copied()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .filter(|&(_, v)| v > 0.0)?;
        let kept = self
            .focus
            .filter(|&(o, why)| why == Why::Leader && rivals.contains(&o))
            .and_then(|(o, _)| standing.iter().find(|(p, _)| *p == o).copied());
        match kept {
            Some((o, v)) if top < v * SWITCH => Some((o, Why::Leader)),
            _ => Some((best, Why::Leader)),
        }
    }
}

/// The enemies still in the game.
fn rivals(s: &Sense) -> Vec<u8> {
    let parties = s.world.stances().teams().len() as u8;
    (0..parties)
        .filter(|&p| p != s.party && s.view.hostile(s.party, p) && !s.world.is_out(p))
        .collect()
}

/// How far ahead each rival is, as far as it has seen: its army's supply,
/// and its halls and other buildings.
fn standings(s: &Sense, rivals: &[u8]) -> Vec<(u8, f32)> {
    let buildings = enemy_buildings(s);
    rivals
        .iter()
        .map(|&o| {
            let army = s.kit.supply_of(spymaster::army_of(s, o));
            let built: f32 = buildings
                .iter()
                .filter(|b| b.owner == o)
                .map(|b| match b.role {
                    Role::Main | Role::Outpost => HALL_STANDING,
                    _ => BUILDING_STANDING,
                })
                .sum();
            (o, army + built)
        })
        .collect()
}
