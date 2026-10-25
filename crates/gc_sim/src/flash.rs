//! Flash floods (docs/38-living-waterways.md, stage 3): a storm in
//! changing weather may send a wave down one river. It starts at the top
//! of the river and runs down it at [`WAVE_TILES_PER_SECOND`]; as the front
//! passes, the water there jumps to a full flood for [`HIGH_SECONDS`], then
//! falls back over [`FALL_SECONDS`]. The river's fords shut once as the
//! front reaches the first of them and open once the water at the last
//! has fallen, so the walkable grid changes twice a wave. A warning comes
//! [`WARNING_SECONDS`] before the front reaches the first ford.
//!
//! The rivers come from the map's flow layer ([`crate::Flow`]): tiles of
//! running water side by side are one river (joined rivers are one), and
//! each tile's reach is how far it lies up the river from where the river
//! ends (the map's edge, or a lake or pond it runs into). Maps with still
//! water have no rivers, so no flash floods.

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::TICKS_PER_SECOND;
use crate::flood::FLOOD_RISE;
use crate::hash::StateHasher;
use crate::sky::{Season, Spell, WeatherKind};
use crate::terrain::{Flow, Terrain};

/// How fast the wave's front runs down the river.
pub const WAVE_TILES_PER_SECOND: u32 = 6;
/// How long the water stays up where the front has passed.
pub const HIGH_SECONDS: u32 = 20;
/// How long it then takes to fall back.
pub const FALL_SECONDS: u32 = 30;
/// How long before the front reaches the first ford the warning comes.
pub const WARNING_SECONDS: u32 = 8;
/// A spell of rain at least this strong is a storm.
pub const STORM_STRENGTH: u8 = 220;

/// The rivers of a map, and the wave running down one of them.
#[derive(Clone, Debug, Default)]
pub struct Flash {
    seed: u64,
    /// The map's side, in tiles.
    side: i32,
    /// One per tile: 0 off the rivers, else the river's number plus 1.
    river: Arc<[u16]>,
    /// One per tile: tiles up the river from where it ends.
    reach: Arc<[u16]>,
    /// The rivers that have fords, with them.
    groups: Arc<[Group]>,
    wave: Option<Wave>,
}

/// One river's fords.
#[derive(Clone, Debug)]
struct Group {
    river: u16,
    /// Where a wave on it sets off: the top of the river, or a stretch
    /// above its first ford if the river runs further up than that.
    top: u16,
    /// The furthest up any of them lies (the first the wave reaches), and
    /// the nearest the river's end (the last).
    first: u16,
    last: u16,
    /// The ford tiles, and the summer fords on the river (which the
    /// wave makes deep again), as `(y, x)`, sorted.
    fords: Vec<(i32, i32)>,
    /// They as runs along each row (`[x, y, w, 1]`).
    runs: Vec<[i32; 4]>,
}

/// A wave running down a river.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wave {
    /// Which of the rivers with fords.
    group: u16,
    /// Ticks since it set off from the top of the river.
    ticks: u32,
    /// Whether the warning has been given.
    warned: bool,
    /// Whether the river's fords are shut by it.
    shut: bool,
}

/// Where a flash flood is, as told (see [`crate::events::Event::FlashFlood`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Its front is [`WARNING_SECONDS`] from the river's first ford.
    Coming,
    /// It has shut the river's fords.
    Here,
    /// The water has fallen and the fords are open again.
    Gone,
}

/// What one tick of the wave changed (see [`Flash::tick`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Turn {
    /// The front is [`WARNING_SECONDS`] from the first ford.
    pub(crate) coming: bool,
    /// The river's fords just shut (`Some(true)`) or opened again.
    pub(crate) fords: Option<bool>,
}

/// The way to a neighbour that a flow points.
fn step_of(flow: u8) -> Option<(i32, i32)> {
    // In the order of the eight ways (an eighth of a turn each from east).
    const STEPS: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    Flow::of(flow).map(|_| STEPS[usize::from(flow & 7)])
}

impl Flash {
    /// The rivers of `terrain`'s map, `map_tiles` a side, and which of
    /// the `fords` and summer fords (`shallows`), as `(y, x)` tiles, lie
    /// on each. Only a river with fords can be flooded.
    pub(crate) fn new(
        terrain: &Terrain,
        fords: &[(i32, i32)],
        shallows: &[(i32, i32)],
        map_tiles: u32,
        seed: u64,
    ) -> Flash {
        let flow = terrain.flow_layer();
        let n = map_tiles as i32;
        if flow.is_empty() || n == 0 {
            return Flash {
                seed,
                ..Flash::default()
            };
        }
        let at = |x: i32, y: i32| crate::terrain::index_in(n, x, y);
        let tiles = (n * n) as usize;
        let mut river = alloc::vec![0u16; tiles];
        let mut reach = alloc::vec![u16::MAX; tiles];
        let mut queue = Vec::new();
        // Where each river ends: running water that runs off the map, or
        // into still water.
        for i in 0..tiles {
            let Some((dx, dy)) = step_of(flow[i]) else {
                continue;
            };
            let (x, y) = (i as i32 % n, i as i32 / n);
            let ends = match at(x + dx, y + dy) {
                None => true,
                Some(j) => flow[j] == 0 && terrain.tile_under_water(x + dx, y + dy),
            };
            if ends {
                reach[i] = 0;
                queue.push(i);
            }
        }
        // How far up each tile lies, through running water.
        let mut head = 0;
        while let Some(&i) = queue.get(head) {
            head += 1;
            let (x, y) = (i as i32 % n, i as i32 / n);
            for (dx, dy) in crate::nav::DIRS {
                if let Some(j) =
                    at(x + dx, y + dy).filter(|&j| flow[j] != 0 && reach[j] == u16::MAX)
                {
                    reach[j] = reach[i].saturating_add(1).min(u16::MAX - 1);
                    queue.push(j);
                }
            }
        }
        // Each river, side by side.
        let mut rivers = 0u16;
        for start in 0..tiles {
            if flow[start] == 0 || river[start] != 0 || reach[start] == u16::MAX {
                continue;
            }
            rivers = rivers.saturating_add(1);
            river[start] = rivers;
            queue.clear();
            queue.push(start);
            while let Some(i) = queue.pop() {
                let (x, y) = (i as i32 % n, i as i32 / n);
                for (dx, dy) in crate::nav::DIRS {
                    if let Some(j) =
                        at(x + dx, y + dy).filter(|&j| reach[j] != u16::MAX && river[j] == 0)
                    {
                        river[j] = rivers;
                        queue.push(j);
                    }
                }
            }
        }
        // Each river's fords.
        let mut groups: Vec<Group> = Vec::new();
        for &(y, x) in fords {
            let Some(i) = at(x, y).filter(|&i| river[i] != 0) else {
                continue;
            };
            let r = reach[i];
            match groups.iter_mut().find(|g| g.river == river[i]) {
                Some(g) => {
                    g.first = g.first.max(r);
                    g.last = g.last.min(r);
                    g.fords.push((y, x));
                }
                None => groups.push(Group {
                    river: river[i],
                    top: 0,
                    first: r,
                    last: r,
                    fords: alloc::vec![(y, x)],
                    runs: Vec::new(),
                }),
            }
        }
        for &(y, x) in shallows {
            let Some(i) = at(x, y).filter(|&i| river[i] != 0) else {
                continue;
            };
            if let Some(g) = groups.iter_mut().find(|g| g.river == river[i]) {
                g.fords.push((y, x));
            }
        }
        // The top of each river.
        let mut tops = alloc::vec![0u16; usize::from(rivers) + 1];
        for i in 0..tiles {
            let t = &mut tops[usize::from(river[i])];
            *t = (*t).max(if river[i] == 0 { 0 } else { reach[i] });
        }
        for g in &mut groups {
            let above = (WARNING_SECONDS * 2 * WAVE_TILES_PER_SECOND) as u16;
            g.top = tops[usize::from(g.river)].min(g.first.saturating_add(above));
            g.fords.sort_unstable();
            g.runs = crate::flood::runs_of(&g.fords);
        }
        groups.sort_by_key(|g| g.river);
        if groups.is_empty() {
            return Flash {
                seed,
                ..Flash::default()
            };
        }
        Flash {
            seed,
            side: n,
            river: river.into(),
            reach: reach.into(),
            groups: groups.into(),
            wave: None,
        }
    }

    /// The wave running now, if any.
    pub fn wave(&self) -> Option<Wave> {
        self.wave
    }

    /// Whether a wave has shut its river's fords.
    pub fn shut(&self) -> bool {
        self.wave.is_some_and(|w| w.shut)
    }

    /// How many rivers have fords a wave can shut.
    pub fn rivers(&self) -> usize {
        self.groups.len()
    }

    /// The water the wave raises, for the terrain (`None` with no wave).
    pub(crate) fn water(&self) -> Option<WaveWater> {
        let wave = self.wave?;
        let g = &self.groups[usize::from(wave.group)];
        Some(WaveWater {
            side: self.side,
            river: self.river.clone(),
            reach: self.reach.clone(),
            id: g.river,
            top: g.top,
            ticks: wave.ticks,
        })
    }

    /// Whether a wave has shut the ford tile `(x, y)` and its front has
    /// reached it, so troops there are swept.
    pub fn shuts(&self, x: i32, y: i32) -> bool {
        let Some(w) = self.wave.filter(|w| w.shut) else {
            return false;
        };
        let g = &self.groups[usize::from(w.group)];
        let reached = crate::terrain::index_in(self.side, x, y)
            .is_some_and(|i| w.ticks >= front_ticks(g.top, self.reach[i]));
        reached && g.fords.binary_search(&(y, x)).is_ok()
    }

    /// The fords and summer fords, as `(row, column)` and sorted, of the
    /// river a wave is coming down or has shut: from its warning till they
    /// open again (empty otherwise). The computer players keep off them.
    pub fn threatened(&self) -> &[(i32, i32)] {
        match self.wave {
            Some(w) => {
                let g = &self.groups[usize::from(w.group)];
                let before = w.warned && w.ticks < front_ticks(g.top, g.first);
                if w.shut || before { &g.fords } else { &[] }
            }
            None => &[],
        }
    }

    /// The wave's river's fords as runs, to shut or open on the walkable
    /// grid (empty with no wave).
    pub(crate) fn runs(&self) -> &[[i32; 4]] {
        self.wave
            .map_or(&[], |w| &self.groups[usize::from(w.group)].runs)
    }

    /// A storm `spell` began, in `season` (with seasons): roll once for a
    /// wave, from the seed and the spell's start, and set one off. Only
    /// one wave runs at a time, and none on frozen water.
    pub(crate) fn storm(&mut self, spell: &Spell, season: Option<Season>, frozen: bool) {
        let w = spell.weather;
        if self.wave.is_some()
            || self.groups.is_empty()
            || frozen
            || w.kind != WeatherKind::Rain
            || w.strength < STORM_STRENGTH
        {
            return;
        }
        let chance = match season {
            None => 40,
            Some(Season::Spring | Season::Autumn) => 60,
            Some(Season::Summer) => 40,
            Some(Season::Winter) => 15,
        };
        let roll = roll(self.seed, spell.at);
        if roll % 100 >= chance {
            return;
        }
        let group = ((roll >> 8) % self.groups.len() as u32) as u16;
        self.wave = Some(Wave {
            group,
            ticks: 0,
            warned: false,
            shut: false,
        });
    }

    /// One tick of the wave: the front runs on, and the river's fords
    /// shut as it reaches the first and open once the water has fallen at
    /// the last.
    pub(crate) fn tick(&mut self) -> Turn {
        let mut turn = Turn::default();
        let Some(mut wave) = self.wave else {
            return turn;
        };
        wave.ticks += 1;
        let g = &self.groups[usize::from(wave.group)];
        let top = g.top;
        let reached = |reach: u16| front_ticks(top, reach);
        if !wave.warned && wave.ticks + WARNING_SECONDS * TICKS_PER_SECOND >= reached(g.first) {
            wave.warned = true;
            turn.coming = true;
        }
        let over = reached(g.last) + (HIGH_SECONDS + FALL_SECONDS / 2) * TICKS_PER_SECOND;
        if !wave.shut && wave.ticks >= reached(g.first) && wave.ticks < over {
            wave.shut = true;
            turn.fords = Some(true);
        } else if wave.shut && wave.ticks >= over {
            wave.shut = false;
            turn.fords = Some(false);
        }
        // Over after the fords open (the water at the river's end falls
        // last), so their runs can still be read as they open.
        let done = reached(0) + (HIGH_SECONDS + FALL_SECONDS) * TICKS_PER_SECOND;
        self.wave = (wave.ticks < done).then_some(wave);
        turn
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        if let Some(w) = self.wave {
            h.u16(w.group);
            h.u32(w.ticks);
            h.u8(u8::from(w.warned) | u8::from(w.shut) << 1);
        }
    }
}

/// The water a running wave raises, as the terrain reads it.
#[derive(Clone, Debug)]
pub(crate) struct WaveWater {
    side: i32,
    river: Arc<[u16]>,
    reach: Arc<[u16]>,
    id: u16,
    top: u16,
    ticks: u32,
}

impl WaveWater {
    /// How far it raises the water on tile `(x, y)`.
    pub(crate) fn rise_at(&self, x: i32, y: i32) -> u16 {
        match crate::terrain::index_in(self.side, x, y) {
            // Above where the wave set off, the river stays as it was.
            Some(i) if self.river[i] == self.id && self.reach[i] <= self.top => {
                rise_after(self.ticks, self.top, self.reach[i])
            }
            _ => 0,
        }
    }
}

/// Ticks from the wave setting off at `top` until its front reaches
/// `reach`.
fn front_ticks(top: u16, reach: u16) -> u32 {
    u32::from(top.saturating_sub(reach)) * TICKS_PER_SECOND / WAVE_TILES_PER_SECOND
}

/// How far a wave `ticks` old that set off at `top` has raised the water
/// at `reach`.
fn rise_after(ticks: u32, top: u16, reach: u16) -> u16 {
    let Some(since) = ticks.checked_sub(front_ticks(top, reach)) else {
        return 0;
    };
    let (high, fall) = (
        HIGH_SECONDS * TICKS_PER_SECOND,
        FALL_SECONDS * TICKS_PER_SECOND,
    );
    if since < high {
        FLOOD_RISE
    } else if since < high + fall {
        (u32::from(FLOOD_RISE) * (high + fall - since) / fall) as u16
    } else {
        0
    }
}

/// A roll from the match seed and a tick: no random stream moves.
fn roll(seed: u64, tick: u32) -> u32 {
    crate::fire::splitmix(seed ^ u64::from(tick).rotate_left(29) ^ 0xf1a5_4f10_0d00_0000) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_water_rises_only_below_where_the_wave_set_off() {
        // One row of river, reach 0 to 9, and a wave set off at reach 5.
        let water = WaveWater {
            side: 10,
            river: alloc::vec![1u16; 100].into(),
            reach: (0..100u16).map(|i| i % 10).collect::<Vec<_>>().into(),
            id: 1,
            top: 5,
            ticks: 1,
        };
        assert_eq!(water.rise_at(5, 0), FLOOD_RISE);
        assert_eq!(water.rise_at(6, 0), 0);
        assert_eq!(water.rise_at(9, 0), 0);
        // Not yet reached further down.
        assert_eq!(water.rise_at(0, 0), 0);
    }

    #[test]
    fn the_water_falls_after_it_has_been_high() {
        let high = HIGH_SECONDS * TICKS_PER_SECOND;
        let fall = FALL_SECONDS * TICKS_PER_SECOND;
        assert_eq!(rise_after(0, 10, 10), FLOOD_RISE);
        assert_eq!(rise_after(high + fall / 2, 10, 10), FLOOD_RISE / 2);
        assert_eq!(rise_after(high + fall, 10, 10), 0);
    }
}
