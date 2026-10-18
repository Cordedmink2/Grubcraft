//! The Steward: a Conquest mind's economy, production and building
//! (docs/18-ai.md, "Steward"). Every half second it puts idle workers to
//! work at the seeps and veins its halls serve, trains workers until those
//! are full, builds housing before supply runs out, puts up and adds
//! production buildings, tiers up, expands to free seeps, and keeps every
//! production building training towards a balanced army.
//!
//! Each faction's economy ([`EconomyStyle`]) changes which nodes count as
//! served and what it builds to serve more: drop-off halls for the shared
//! economy, Pustules and Hives spreading the root-veins for the Brood, and
//! Resonators and Siphons tuning nodes for the Choir, whose workers only
//! build.

use gc_sim::economy::{GRUB_SLOTS, MARROW_SLOTS, SUPPLY_LIMIT};
use gc_sim::structures::{BuildingStats, Role, StructureHandle, StructureView, TRADE_POST, VEINS};
use gc_sim::{BuildingId, Command, RegimentHandle, Resource, TICKS_PER_SECOND, TradePost, UnitId};
use glam::Vec2;

use super::Sense;
use super::kit::{EconomyStyle, KitBuilding, KitUnit};
use super::strategy::Early;
use super::{placing, sovereign, spymaster};
use crate::orders::Hands as Clicks;
use crate::perception::{Class, Seen, to_fx, to_fx_vec, v2};

/// Ticks between looks.
const THINK_TICKS: u32 = TICKS_PER_SECOND / 2;
/// A node is served by a drop-off within this many tiles of it.
const SERVE: f32 = 22.0;
/// Ticks a lay may take to show on the field before it is tried again.
const LAY_TICKS: u32 = 3 * TICKS_PER_SECOND;
/// Ticks a worker sent somewhere is left alone before it counts as idle.
const SENT_TICKS: u32 = 2 * TICKS_PER_SECOND;
/// The longest an opening's building still to go up comes before arming.
const OPENING_HOLD: u32 = 120 * TICKS_PER_SECOND;
/// Regiments queued per production building.
const QUEUE: usize = 2;
/// Most workers it keeps.
const MAX_WORKERS: usize = 48;
/// Workers it keeps beyond those its nodes take: builders.
const SPARE_WORKERS: usize = 2;
/// Army make-up it aims for, by class: line, shock, ranged.
const MIX: [f32; 3] = [0.5, 0.2, 0.3];
/// How far a level that counters shifts its make-up towards the answer to
/// the enemy army it has seen: its share of each class moves this much of
/// the class it beats into it.
const COUNTER_SHIFT: f32 = 0.5;
/// One siege or artillery regiment per this many fighters, at most
/// [`MAX_BREAKERS`].
const PER_BREAKER: usize = 6;
const MAX_BREAKERS: usize = 3;
/// Marrow in store it stops sending more workers to the veins at.
const MARROW_ENOUGH: u32 = 600;
/// A node with less than this left counts as running out.
const LOW: u32 = 1_500;
/// The army a level that weighs the odds keeps up before it spends on
/// growth: this much supply a minute of game time, up to the floor.
const ARMY_PER_MINUTE: f32 = 4.0;
const ARMY_FLOOR: f32 = 40.0;
/// Odds against an enemy army it knows of that make it arm.
const OUTMATCHED: f32 = -0.1;
/// Supply short of the limit at which it stops arming: it is as armed as
/// it can be.
const ARMED_SLACK: u32 = 8;
/// How often a Marrow gatherer may move to Grub.
const REBALANCE_TICKS: u32 = 5 * TICKS_PER_SECOND;
/// Workers the Choir keeps, all builders.
const BUILDERS: usize = 3;
/// How far from the main building's middle a Pustule may go to carry the
/// veins on towards a seep: inside the Hive's own veins, so it links.
const RELAY_REACH: f32 = 16.0;
/// Caravans on each trade route the Concord runs.
const PER_ROUTE: usize = 3;
/// Routes it runs from the start, before any tier.
const START_ROUTES: usize = 2;
/// Routes it runs at once at most, one more for each tier after the first.
const MAX_ROUTES: usize = 3;
/// How many workers a caravan counts for in the size of its economy: what
/// its Grub is worth against a Clerk's Marrow.
const CARAVAN_WORKERS: usize = 3;
/// How far from home a neutral trade post is worth running to, in tiles.
const POST_REACH: f32 = 110.0;
/// How far from home it puts a Waystation up when no hamlet is in reach.
const WAYSTATION_RANGE: f32 = 36.0;
/// A caravan sent to a post that is idle again before this is up never set
/// out (the way is shut): the post is given up.
const ROUTE_TICKS: u32 = 15 * TICKS_PER_SECOND;
/// A caravan on the road that hasn't moved this far in this long is stuck
/// (blocked in a ford, or at the end of a path that goes nowhere): it is
/// sent home, and sent out again from there.
const STUCK_TICKS: u32 = 40 * TICKS_PER_SECOND;
const STUCK_TILES: f32 = 4.0;

/// A building it ordered laid that hasn't shown on the field yet.
#[derive(Clone, Copy, Debug)]
struct Lay {
    building: BuildingId,
    tile: (i32, i32),
    since: u32,
}

/// What the Steward is up to, for the journal, the lens and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    pub workers: u16,
    pub want_workers: u16,
    pub producers: u8,
    pub tier: u8,
    /// Grub and Marrow set aside for what it wants to build next.
    pub saving: (u32, u32),
}

#[derive(Clone, Debug, Default)]
pub struct Steward {
    next_think: u32,
    laying: Vec<Lay>,
    /// Workers given an order lately, and when.
    sent: Vec<(RegimentHandle, u32)>,
    /// The rally point each production building was last given.
    rallied: Vec<(StructureHandle, Vec2)>,
    pub ledger: Ledger,
    /// The middle of its first main building: home.
    pub home: Option<Vec2>,
    /// Since when an opening's building has come before arming.
    opening_since: Option<u32>,
    /// When a Marrow gatherer may next move to Grub.
    next_rebalance: u32,
    /// When it may next look for a spot, after finding none.
    next_lay: u32,
    /// Orders given to its buildings that haven't landed yet (so it can't
    /// see them in the world): the building, the unit if one is to be
    /// trained, and when.
    busy: Vec<(StructureHandle, Option<UnitId>, u32)>,
    /// Nodes it found no spot to tune from.
    untunable: Vec<u16>,
    /// It is short of an army: it trains soldiers first, and puts off
    /// timed expansion and new production meanwhile.
    arming: bool,
    /// Caravans sent to a trade post lately, and when.
    trips: Vec<(RegimentHandle, TradePost, u32)>,
    /// Trade posts a caravan never reached.
    shunned: Vec<TradePost>,
    /// Where each caravan on the road last was seen to be, and since when:
    /// one that doesn't move for a long while is stuck.
    drift: Vec<(RegimentHandle, Vec2, u32)>,
}

/// One of its own buildings, with its type's stats.
#[derive(Clone, Copy)]
struct Mine {
    view: StructureView,
    stats: BuildingStats,
}

impl Mine {
    fn finished(&self) -> bool {
        self.view.finished()
    }
    fn at(&self) -> Vec2 {
        v2(self.view.centre())
    }
    fn takes(&self, r: Resource) -> bool {
        self.stats.flags & r.drop_off() != 0
    }
    /// Whether, once finished, it makes the nodes near it worth working:
    /// a drop-off hall, a Hive spreading veins, a Resonator.
    fn hub(&self, style: EconomyStyle) -> bool {
        match style {
            EconomyStyle::Shared => self.takes(Resource::Grub),
            EconomyStyle::Veins => self.stats.flags & VEINS != 0 && self.stats.role == Role::Main,
            EconomyStyle::Tuned => self.tunes(Resource::Grub),
            EconomyStyle::Caravans => self.takes(Resource::Marrow) && self.stats.role == Role::Main,
        }
    }
    /// Whether it tunes nodes of `r`.
    fn tunes(&self, r: Resource) -> bool {
        self.stats.tunes.is_some_and(|(t, _)| t == r)
    }
    /// Whether it tunes (or will, once finished) a node of `r` at `p`.
    fn reaches(&self, r: Resource, p: Vec2) -> bool {
        self.stats.tunes.is_some_and(|(t, tiles)| {
            t == r
                && gap(self.view.tile, (self.stats.width, self.stats.depth), p) <= f32::from(tiles)
        })
    }
}

/// How far `p` is from the edge of the footprint at `tile`, `size` across
/// and deep, in tiles (0 inside it).
fn gap(tile: (i32, i32), size: (u8, u8), p: Vec2) -> f32 {
    let lo = Vec2::new(tile.0 as f32, tile.1 as f32);
    let hi = lo + Vec2::new(f32::from(size.0), f32::from(size.1));
    p.clamp(lo, hi).distance(p)
}

/// What the Steward reads off the field at each look.
struct Look<'a> {
    mine: Vec<Mine>,
    workers: Vec<&'a Seen>,
    army: Vec<&'a Seen>,
    /// Its caravans, broken ones aside.
    caravans: Vec<&'a Seen>,
    /// Workers at each node, by node index.
    at_node: Vec<u16>,
    /// Nodes its finished drop-offs serve: (index, resource, at, left).
    served: Vec<(u16, Resource, Vec2, u32)>,
    tier: u8,
}

impl Steward {
    /// Whether it is short of an army and training soldiers first.
    pub fn arming(&self) -> bool {
        self.arming
    }

    pub fn new() -> Steward {
        Steward::default()
    }

    /// Look and give the economy's orders, sending what it trains to
    /// `rally`.
    pub fn think(&mut self, s: &Sense, clicks: &mut Clicks, rally: Option<Vec2>) {
        if s.now < self.next_think {
            return;
        }
        self.next_think = s.now + THINK_TICKS;
        let Some(look) = self.look(s) else {
            return;
        };
        if self.home.is_none() {
            self.home = look
                .mine
                .iter()
                .find(|m| m.stats.role == Role::Main)
                .map(Mine::at);
        }
        let reaction = s.reaction;
        self.laying.retain(|l| {
            s.now < l.since + reaction + LAY_TICKS
                && !look
                    .mine
                    .iter()
                    .any(|m| m.view.building == l.building && m.view.tile == l.tile)
        });
        self.sent
            .retain(|&(_, t)| s.now < t + reaction + SENT_TICKS);
        self.busy.retain(|&(_, _, t)| s.now <= t + reaction);
        self.ledger.saving = (0, 0);
        self.ledger.tier = look.tier;

        self.arming = self.must_arm(s, &look);
        self.put_to_work(s, clicks, &look);
        self.rebalance(s, clicks, &look);
        self.finish_buildings(s, clicks, &look);
        // Short of an army, soldiers come before more workers, and the
        // money it has goes on them first; but an opening's building still
        // due comes first, or arming would spend what it saves for it.
        // One it can't get up stops holding up the army after a while.
        let due = self.arming && self.opening_due(s, &look);
        let since = *self.opening_since.get_or_insert(s.now);
        if !due {
            self.opening_since = None;
        }
        let soldiers_first = self.arming && !(due && s.now < since + OPENING_HOLD);
        if soldiers_first {
            self.train_army(s, clicks, &look);
        }
        self.train_workers(s, clicks, &look);
        self.build(s, clicks, &look);
        self.tier_up(s, clicks, &look);
        if !soldiers_first {
            self.train_army(s, clicks, &look);
        }
        if let Some(rally) = rally {
            self.rally(s, clicks, &look, rally);
        }
    }

    fn look<'a>(&self, s: &Sense<'a>) -> Option<Look<'a>> {
        let structures = s.world.structures()?;
        let economy = s.world.economy()?;
        let table = structures.buildings();
        let mine: Vec<Mine> = structures
            .iter()
            .filter(|v| v.owner == s.party)
            .filter_map(|view| {
                Some(Mine {
                    view,
                    stats: *table.get(usize::from(view.building))?,
                })
            })
            .collect();
        let ours = s.view.regiments.iter().filter(|r| r.owner == s.party);
        let workers: Vec<&Seen> = ours
            .clone()
            .filter(|r| s.kit.is_worker(r.unit) && !r.broken())
            .collect();
        let caravans: Vec<&Seen> = ours
            .clone()
            .filter(|r| s.kit.is_caravan(r.unit) && !r.broken())
            .collect();
        let army: Vec<&Seen> = ours.filter(|r| s.kit.is_fighter(r.unit)).collect();
        let nodes: Vec<_> = economy.nodes().collect();
        let mut at_node = vec![0u16; economy.node_slots()];
        for w in &workers {
            if let Some(n) = s.world.gathers_at(w.handle)
                && let Some(c) = at_node.get_mut(usize::from(n))
            {
                *c += 1;
            }
        }
        let served = nodes
            .iter()
            .filter(|n| n.left > 0)
            .filter(|n| match s.kit.economy {
                EconomyStyle::Shared | EconomyStyle::Caravans => {
                    let at = v2(n.at);
                    mine.iter().any(|m| {
                        m.finished() && m.takes(n.resource) && m.at().distance(at) <= SERVE
                    })
                }
                EconomyStyle::Veins => s.world.on_veins(s.party, n.at),
                EconomyStyle::Tuned => n
                    .tuner
                    .is_some_and(|h| mine.iter().any(|m| m.view.handle == h)),
            })
            .map(|n| (n.index, n.resource, v2(n.at), n.left))
            .collect();
        Some(Look {
            tier: structures.tier_of(s.party),
            mine,
            workers,
            army,
            caravans,
            at_node,
            served,
        })
    }

    fn recently_sent(&self, h: RegimentHandle) -> bool {
        self.sent.iter().any(|&(w, _)| w == h)
    }

    /// Whether it wants Marrow brought in now: once it trains soldiers or
    /// has tiered up, while it has less than it can soon spend.
    fn wants_marrow(&self, s: &Sense, look: &Look) -> bool {
        let banked = s.world.bank(s.party).map_or(0, |b| b.marrow);
        if s.kit.economy == EconomyStyle::Caravans {
            return banked < 3 * MARROW_ENOUGH;
        }
        let started = look.tier >= 2
            || working(s, look) >= 10
            || look
                .mine
                .iter()
                .any(|m| m.finished() && trains_fighters(s, m.view.building));
        started && banked < MARROW_ENOUGH
    }

    /// Idle workers go to the served node that most needs them, the nearest
    /// first; workers sent to one node go in one click.
    fn put_to_work(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        if s.kit.economy == EconomyStyle::Tuned {
            return;
        }
        if s.kit.economy == EconomyStyle::Caravans {
            self.run_caravans(s, clicks, look);
        }
        let mut at_node = look.at_node.clone();
        let marrow = self.wants_marrow(s, look);
        let idle: Vec<RegimentHandle> = s
            .world
            .idle_workers(s.party)
            // A worker that only builds (the Choir's Tuner) can't gather.
            .filter(|&h| !self.recently_sent(h) && !s.world.builds_only(h))
            .collect();
        let mut sent: Vec<(u16, Vec<RegimentHandle>)> = Vec::new();
        for h in idle {
            let Some(w) = s.view.get(h) else { continue };
            let room = |&(i, r, _, _): &(u16, Resource, Vec2, u32)| {
                let cap = match r {
                    Resource::Grub => GRUB_SLOTS,
                    Resource::Marrow => MARROW_SLOTS,
                };
                if r == Resource::Marrow && !marrow {
                    return 0;
                }
                i32::from(cap) - i32::from(at_node[usize::from(i)])
            };
            // Most room first (Marrow before Grub when both have room, as
            // it is scarcer), then the nearest.
            let pick = look
                .served
                .iter()
                .filter(|n| room(n) > 0)
                .min_by(|a, b| {
                    let key = |n: &(u16, Resource, Vec2, u32)| {
                        (n.1 == Resource::Grub, w.pos.distance(n.2) as i32)
                    };
                    key(a).cmp(&key(b))
                })
                // Everything full: queue at the Grub seep with fewest.
                .or_else(|| {
                    look.served
                        .iter()
                        .filter(|n| n.1 == Resource::Grub)
                        .min_by_key(|n| (at_node[usize::from(n.0)], w.pos.distance(n.2) as i32))
                });
            let Some(&(node, ..)) = pick else { continue };
            at_node[usize::from(node)] += 1;
            match sent.iter_mut().find(|(n, _)| *n == node) {
                Some((_, ws)) => ws.push(h),
                None => sent.push((node, vec![h])),
            }
        }
        for (node, ws) in sent {
            if !clicks.free() {
                break;
            }
            for (k, &regiment) in ws.iter().enumerate() {
                let order = Command::Gather { regiment, node };
                if k == 0 {
                    clicks.give(s.now, order);
                } else {
                    clicks.push(s.now, order);
                }
                self.sent.push((regiment, s.now));
            }
        }
    }

    /// The trade posts its caravans run to, nearest home first: the map's
    /// hamlets in reach that no caravan failed to reach, else its own
    /// finished Waystations.
    fn routes(&self, s: &Sense, look: &Look) -> Vec<TradePost> {
        let home = self.home.unwrap_or(Vec2::ZERO);
        let mut routes: Vec<(f32, TradePost)> = s
            .world
            .trade_posts()
            .iter()
            .enumerate()
            .map(|(i, p)| (home.distance(v2(*p)), TradePost::Hamlet(i as u16)))
            .filter(|(d, post)| *d <= POST_REACH && !self.shunned.contains(post))
            .filter(|(_, post)| s.world.is_trade_post(s.party, *post))
            .collect();
        if routes.is_empty() {
            routes = look
                .mine
                .iter()
                .filter(|m| m.finished() && m.stats.flags & TRADE_POST != 0)
                .map(|m| (home.distance(m.at()), TradePost::Structure(m.view.handle)))
                .filter(|(_, post)| !self.shunned.contains(post))
                .collect();
        }
        routes.sort_by(|a, b| a.0.total_cmp(&b.0));
        routes.into_iter().map(|(_, post)| post).collect()
    }

    /// Whether a hamlet is in reach of home and no caravan gave it up.
    fn hamlets(&self, s: &Sense) -> bool {
        let home = self.home.unwrap_or(Vec2::ZERO);
        s.world.trade_posts().iter().enumerate().any(|(i, p)| {
            home.distance(v2(*p)) <= POST_REACH
                && !self.shunned.contains(&TradePost::Hamlet(i as u16))
        })
    }

    /// The Concord's economy: Grub comes by caravan. Idle caravans go to
    /// the nearest trade post with room on its route, and a post a caravan
    /// sent there is idle again from, at once, is given up as shut.
    fn run_caravans(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        let idle: Vec<RegimentHandle> = s.world.idle_traders(s.party).collect();
        let trips = std::mem::take(&mut self.trips);
        for &(h, post, since) in &trips {
            let set_off = s.now > since + s.reaction + SENT_TICKS;
            if idle.contains(&h) && set_off && !self.shunned.contains(&post) {
                self.shunned.push(post);
            }
        }
        self.trips = trips
            .into_iter()
            .filter(|&(h, _, since)| {
                s.now < since + s.reaction + ROUTE_TICKS
                    && look.caravans.iter().any(|c| c.handle == h)
            })
            .collect();
        self.unstick_caravans(s, clicks, look);
        self.send_caravans(s, clicks, look, &idle);
        self.train_caravans(s, clicks, look);
    }

    /// A caravan on the road that has stood still for [`STUCK_TICKS`] (one
    /// locked in a fight it can't win, or chasing a routed enemy into a
    /// corner, is as stuck as one blocked in a ford) breaks off the fight
    /// and is sent home with a plain move, which ends its run; it is idle
    /// there, and sent out again.
    fn unstick_caravans(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        let Some(home) = self.home else { return };
        self.drift
            .retain(|d| look.caravans.iter().any(|c| c.handle == d.0));
        for c in &look.caravans {
            let trading = s.world.trades_at(c.handle).is_some();
            let k = self.drift.iter().position(|d| d.0 == c.handle);
            let Some(k) = k else {
                self.drift.push((c.handle, c.pos, s.now));
                continue;
            };
            // Loading at a post or standing idle isn't being stuck.
            let moved = c.pos.distance(self.drift[k].1) >= STUCK_TILES;
            let stuck = !moved && trading && c.moving && s.now >= self.drift[k].2 + STUCK_TICKS;
            // Locked in a fight, it breaks off first.
            let order = if c.engaged {
                Command::FallBack { regiment: c.handle }
            } else {
                Command::MoveTo {
                    regiment: c.handle,
                    target: to_fx_vec(home),
                    facing: None,
                    files: 0,
                    pace: to_fx(0.0),
                }
            };
            if !stuck || clicks.give(s.now, order) {
                // Moving on, or told to: the clock starts again here.
                if moved || stuck || !trading || !c.moving {
                    self.drift[k] = (c.handle, c.pos, s.now);
                }
            }
        }
    }

    /// Idle caravans take the first route with fewer than [`PER_ROUTE`]
    /// on it, else the one with the fewest.
    fn send_caravans(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
        idle: &[RegimentHandle],
    ) {
        let routes = self.routes(s, look);
        if routes.is_empty() {
            return;
        }
        let mut on: Vec<usize> = routes
            .iter()
            .map(|&post| {
                look.caravans
                    .iter()
                    .filter(|c| s.world.trades_at(c.handle) == Some(post))
                    .count()
            })
            .collect();
        for &h in idle {
            if self.recently_sent(h) || !clicks.free() {
                continue;
            }
            let pick = on
                .iter()
                .position(|&n| n < PER_ROUTE)
                .or_else(|| (0..on.len()).min_by_key(|&i| on[i]));
            let Some(i) = pick else { return };
            let post = routes[i];
            if clicks.give(s.now, Command::Trade { regiment: h, post }) {
                on[i] += 1;
                self.sent.push((h, s.now));
                self.trips.push((h, post, s.now));
            }
        }
    }

    /// Caravans to train: [`PER_ROUTE`] for each route it runs, up to one
    /// route for each tier, at its main buildings.
    fn train_caravans(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        let Some(caravan) = s.kit.caravan() else {
            return;
        };
        if self.routes(s, look).is_empty() {
            return;
        }
        let routes = (START_ROUTES + usize::from(look.tier.max(1)) - 1).min(MAX_ROUTES);
        let want = (PER_ROUTE * routes * usize::from(s.plan.workers_percent))
            .div_ceil(100)
            .max(2);
        let queued = |this: &Steward, p: &gc_sim::production::Production| {
            look.mine
                .iter()
                .flat_map(|m| p.queue_of(m.view.handle))
                .filter(|&&u| s.kit.is_caravan(u))
                .count()
                + this
                    .busy
                    .iter()
                    .filter(|b| b.1.is_some_and(|u| s.kit.is_caravan(u)))
                    .count()
        };
        let Some(p) = s.world.production() else {
            return;
        };
        let Some(mut bank) = s.world.bank(s.party) else {
            return;
        };
        let mut have = look.caravans.len() + queued(self, p);
        for m in look.mine.iter().filter(|m| m.finished()) {
            if have >= want {
                return;
            }
            let trains = caravan
                .unit
                .trained_at
                .is_some_and(|at| grew_from(s, m.view.building, at));
            if !trains || self.queued(p, m.view.handle) >= QUEUE {
                continue;
            }
            if bank.supply_used + u32::from(caravan.unit.supply) > bank.supply_cap {
                return;
            }
            if s.world
                .can_train(s.party, m.view.handle, caravan.id)
                .is_ok()
                && clicks.give(
                    s.now,
                    Command::Train {
                        structure: m.view.handle,
                        unit: caravan.id,
                    },
                )
            {
                have += 1;
                bank.supply_used += u32::from(caravan.unit.supply);
                self.busy.push((m.view.handle, Some(caravan.id), s.now));
            }
        }
    }

    /// A Waystation of its own, when no hamlet is in reach to run to: put
    /// up a stretch from home towards the middle of the map, once.
    fn waystation<'k>(&self, s: &Sense<'k>, look: &Look) -> Option<(&'k KitBuilding, Vec2)> {
        if s.kit.economy != EconomyStyle::Caravans || self.hamlets(s) {
            return None;
        }
        let b = s.kit.waystation()?;
        if look.mine.iter().any(|m| m.view.building == b.id) {
            return None;
        }
        let home = self.home?;
        let middle = s.kit.starts.iter().copied().sum::<Vec2>() / s.kit.starts.len().max(1) as f32;
        let spot = home + (middle - home).normalize_or_zero() * WAYSTATION_RANGE;
        Some((b, spot))
    }

    /// With Marrow piling up, one Marrow gatherer every few seconds moves to the
    /// nearest Grub seep with room: a bank of Marrow buys nothing without
    /// the Grub to go with it.
    fn rebalance(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        let banked = s.world.bank(s.party).map_or(0, |b| b.marrow);
        if banked < 2 * MARROW_ENOUGH
            || s.now < self.next_rebalance
            || !clicks.free()
            || matches!(s.kit.economy, EconomyStyle::Tuned | EconomyStyle::Caravans)
        {
            return;
        }
        let on_marrow = |n: u16| {
            look.served
                .iter()
                .any(|m| m.0 == n && m.1 == Resource::Marrow)
        };
        let Some(w) = look.workers.iter().find(|w| {
            !self.recently_sent(w.handle) && s.world.gathers_at(w.handle).is_some_and(on_marrow)
        }) else {
            return;
        };
        let Some(&(node, ..)) = look
            .served
            .iter()
            .filter(|n| {
                n.1 == Resource::Grub && look.at_node[usize::from(n.0)] < u16::from(GRUB_SLOTS)
            })
            .min_by_key(|n| w.pos.distance(n.2) as i32)
        else {
            return;
        };
        clicks.give(
            s.now,
            Command::Gather {
                regiment: w.handle,
                node,
            },
        );
        self.sent.push((w.handle, s.now));
        self.next_rebalance = s.now + REBALANCE_TICKS;
    }

    /// A building going up with nobody building it gets the nearest
    /// worker. One growing on its own (a Brood building that used up its
    /// Gnawer) needs nobody.
    fn finish_buildings(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        for m in look
            .mine
            .iter()
            .filter(|m| !m.finished() && !m.view.growing)
        {
            if self.is_busy(m.view.handle) {
                continue;
            }
            let building = look
                .workers
                .iter()
                .any(|w| s.world.building_target(w.handle) == Some(m.view.handle));
            if building {
                continue;
            }
            let Some(w) = self.builder(s, look, m.at()) else {
                return;
            };
            if clicks.give(
                s.now,
                Command::Construct {
                    regiment: w,
                    structure: m.view.handle,
                },
            ) {
                self.sent.push((w, s.now));
                self.busy.push((m.view.handle, None, s.now));
            }
        }
    }

    /// Whether an order to building `h` is still on its way.
    fn is_busy(&self, h: StructureHandle) -> bool {
        self.busy.iter().any(|b| b.0 == h)
    }

    /// Regiments queued at building `h`, counting those ordered but not
    /// yet landed.
    fn queued(&self, p: &gc_sim::production::Production, h: StructureHandle) -> usize {
        p.queue_of(h).len()
            + self
                .busy
                .iter()
                .filter(|b| b.0 == h && b.1.is_some())
                .count()
    }

    /// The worker to send to build at `at`: the nearest one gathering
    /// Grub with nothing in its hands, else any not building.
    fn builder(&self, s: &Sense, look: &Look, at: Vec2) -> Option<RegimentHandle> {
        let free = |w: &&&Seen| {
            !self.recently_sent(w.handle)
                && s.world.building_target(w.handle).is_none()
                && !w.engaged
        };
        let near = |a: &&&Seen, b: &&&Seen| a.pos.distance(at).total_cmp(&b.pos.distance(at));
        let grub = |w: &&&Seen| {
            s.world.gathers_at(w.handle).is_some_and(|n| {
                look.served
                    .iter()
                    .any(|x| x.0 == n && x.1 == Resource::Grub)
            }) && s.world.carries(w.handle).is_none()
        };
        look.workers
            .iter()
            .filter(free)
            .filter(grub)
            .min_by(near)
            .or_else(|| look.workers.iter().filter(free).min_by(near))
            .map(|w| w.handle)
    }

    /// Workers in training or ordered, across its buildings.
    fn workers_queued(&self, s: &Sense, look: &Look) -> usize {
        let Some(p) = s.world.production() else {
            return 0;
        };
        let ordered = self
            .busy
            .iter()
            .filter(|b| b.1.is_some_and(|u| s.kit.is_worker(u)))
            .count();
        look.mine
            .iter()
            .flat_map(|m| p.queue_of(m.view.handle))
            .filter(|&&u| s.kit.is_worker(u))
            .count()
            + ordered
    }

    fn train_workers(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        let Some(worker) = s.kit.worker() else { return };
        let marrow = self.wants_marrow(s, look);
        let slots: usize = look
            .served
            .iter()
            .map(|n| match n.1 {
                Resource::Grub => usize::from(GRUB_SLOTS),
                Resource::Marrow if marrow => usize::from(MARROW_SLOTS),
                Resource::Marrow => 0,
            })
            .sum();
        let rising = look
            .mine
            .iter()
            .filter(|m| !m.finished() && m.hub(s.kit.economy))
            .count();
        let want = match s.kit.economy {
            EconomyStyle::Tuned => BUILDERS + usize::from(look.tier),
            _ => {
                let want = (slots + SPARE_WORKERS + rising * 4).min(MAX_WORKERS);
                let want = want * usize::from(s.plan.workers_percent) / 100;
                match s.plan.early.worker_cap {
                    Some(cap) => want.min(usize::from(cap)),
                    None => want,
                }
            }
        };
        self.ledger.workers = look.workers.len() as u16;
        self.ledger.want_workers = want as u16;
        let mut have = look.workers.len() + self.workers_queued(s, look);
        let Some(p) = s.world.production() else {
            return;
        };
        let Some(mut bank) = s.world.bank(s.party) else {
            return;
        };
        for m in look.mine.iter().filter(|m| m.finished()) {
            if have >= want {
                return;
            }
            let trains = worker
                .unit
                .trained_at
                .is_some_and(|at| grew_from(s, m.view.building, at));
            if !trains || self.queued(p, m.view.handle) >= QUEUE {
                continue;
            }
            if bank.supply_used + u32::from(worker.unit.supply) > bank.supply_cap {
                return;
            }
            if s.world.can_train(s.party, m.view.handle, worker.id).is_ok()
                && clicks.give(
                    s.now,
                    Command::Train {
                        structure: m.view.handle,
                        unit: worker.id,
                    },
                )
            {
                have += 1;
                bank.supply_used += u32::from(worker.unit.supply);
                self.busy.push((m.view.handle, Some(worker.id), s.now));
            }
        }
    }

    /// Whatever it most wants to lay next, if anything: housing before
    /// supply runs out, a first production building, an expansion, more
    /// production.
    fn build(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        if !self.laying.is_empty() || s.now < self.next_lay {
            return;
        }
        let Some(bank) = s.world.bank(s.party) else {
            return;
        };
        let rising_supply: u32 = look
            .mine
            .iter()
            .filter(|m| !m.finished())
            .map(|m| u32::from(m.stats.supply))
            .sum();
        let producers: Vec<&Mine> = look
            .mine
            .iter()
            .filter(|m| trains_fighters(s, m.view.building))
            .collect();
        let production = producers
            .iter()
            .filter(|m| m.stats.role != Role::Main)
            .count();
        self.ledger.producers = producers.len() as u8;
        let headroom = 4 + 3 * producers.len() as u32;
        let cap = bank.supply_cap + rising_supply;
        let workers = working(s, look);
        let marrow = self.wants_marrow(s, look);
        let housing = || {
            (cap < SUPPLY_LIMIT && cap < bank.supply_used + headroom)
                .then(|| s.kit.housing())
                .flatten()
                .map(|b| (b, self.housing_spot(s, look, b)))
        };
        // The Choir: a tuner by each node near its buildings not yet tuned,
        // Grub first, Marrow once it wants it.
        let tune = || {
            if s.kit.economy != EconomyStyle::Tuned {
                return None;
            }
            [Resource::Grub, Resource::Marrow]
                .into_iter()
                .filter(|&r| r == Resource::Grub || marrow)
                .find_map(|r| Some((s.kit.tuner(r)?, self.tune_site(s, look, r)?)))
        };
        // The Brood: another Hatchery Pit when larvae run short and Grub
        // piles up, one per tier.
        let hatch = || {
            let b = s.kit.hatchery()?;
            let pits = look.mine.iter().filter(|m| m.view.building == b.id).count();
            (s.kit.economy == EconomyStyle::Veins
                && bank.larvae == 0
                && bank.grub >= 2 * u32::from(b.stats.cost_grub) + 200
                && pits < usize::from(look.tier.max(1)))
            .then(|| (b, self.inner_spot(look)))
        };
        let first = || {
            (production == 0 && workers >= usize::from(s.plan.early.workers_first))
                .then(|| self.production_type(s, look))
                .flatten()
                .map(|b| (b, self.inner_spot(look)))
        };
        // The opening's towers by the hall.
        let tower = || {
            let towers = look
                .mine
                .iter()
                .filter(|m| m.stats.role == Role::Defence)
                .count();
            if towers >= usize::from(s.plan.early.towers) {
                return None;
            }
            Some((s.kit.tower(look.tier)?, self.inner_spot(look)))
        };
        let expand = || {
            let b = match s.kit.economy {
                EconomyStyle::Tuned => s.kit.tuner(Resource::Grub),
                _ => s.kit.expansion(),
            }?;
            if !self.time_to_expand(s, look) {
                return None;
            }
            // Saving up for it: where can wait until it can pay.
            if s.world.may_lay(s.party, b.id).is_err() {
                return Some((b, Vec2::ZERO));
            }
            Some((b, self.expansion_site(s, look, self.home?)?))
        };
        // The opening's production buildings after the first go up
        // whatever the workers, and while it is arming.
        let opening_more = production > 0 && production < usize::from(s.plan.early.producers);
        let grown = !self.arming
            && production < usize::from(s.plan.producers_per_tier) * usize::from(look.tier.max(1))
            && workers >= 8 + 6 * production;
        let more = || {
            (opening_more || grown)
                .then(|| self.production_type(s, look))
                .flatten()
                .map(|b| (b, self.inner_spot(look)))
        };
        let wanted: Option<(&KitBuilding, Vec2)> = housing()
            .or_else(tune)
            .or_else(|| self.waystation(s, look))
            .or_else(first)
            .or_else(tower)
            .or_else(hatch)
            .or_else(expand)
            .or_else(more);
        let Some((b, near)) = wanted else { return };
        if s.world.may_lay(s.party, b.id).is_err() {
            self.ledger.saving = (u32::from(b.stats.cost_grub), u32::from(b.stats.cost_marrow));
            return;
        }
        let Some(tile) =
            placing::spot_for(s.world, s.party, b.id, &b.stats, near, &self.lanes(s, look))
        else {
            // Nowhere fits: look again in a while, not every think.
            self.next_lay = s.now + LAY_TICKS;
            return;
        };
        // A tuner too far from the node it was put up for would tune
        // nothing: give that node up.
        if let Some((r, tiles)) = b.stats.tunes
            && let Some(n) = self.node_at(s, r, near)
            && gap(tile, (b.stats.width, b.stats.depth), near) > f32::from(tiles)
        {
            self.untunable.push(n);
            return;
        }
        let foot_mid = Vec2::new(
            tile.0 as f32 + f32::from(b.stats.width) / 2.0,
            tile.1 as f32 + f32::from(b.stats.depth) / 2.0,
        );
        let Some(w) = self.builder(s, look, foot_mid) else {
            return;
        };
        let order = Command::Build {
            regiment: w,
            building: b.id,
            tile: (tile.0 as i16, tile.1 as i16),
        };
        if clicks.give(s.now, order) {
            self.sent.push((w, s.now));
            self.laying.push(Lay {
                building: b.id,
                tile,
                since: s.now,
            });
        }
    }

    /// Where to put housing: for the Brood, a Pustule carrying the veins
    /// towards the nearest seep they don't reach; for the Choir, a
    /// Resonator by a seep it doesn't tune yet; else at home.
    fn housing_spot(&self, s: &Sense, look: &Look, b: &KitBuilding) -> Vec2 {
        let home = self.home.unwrap_or(Vec2::ZERO);
        let site = match s.kit.economy {
            EconomyStyle::Tuned => b.stats.tunes.and_then(|(r, _)| self.tune_site(s, look, r)),
            EconomyStyle::Veins if b.stats.flags & VEINS != 0 => s.world.economy().and_then(|e| {
                e.nodes()
                    .filter(|n| n.resource == Resource::Grub && n.left > LOW)
                    .filter(|n| !s.world.on_veins(s.party, n.at))
                    .map(|n| v2(n.at))
                    .filter(|p| p.distance(home) <= RELAY_REACH + 10.0)
                    .min_by(|a, b| a.distance(home).total_cmp(&b.distance(home)))
                    .map(|p| {
                        let d = p.distance(home);
                        home + (p - home) / d.max(1.0) * (d - 4.0).clamp(0.0, RELAY_REACH)
                    })
            }),
            _ => None,
        };
        site.unwrap_or_else(|| self.inner_spot(look))
    }

    /// The Choir's next node of `r` to tune: the nearest to home of those
    /// near its buildings that none of its tuners (finished or going up)
    /// reaches.
    fn tune_site(&self, s: &Sense, look: &Look, r: Resource) -> Option<Vec2> {
        let home = self.home?;
        let economy = s.world.economy()?;
        economy
            .nodes()
            .filter(|n| n.resource == r && n.left > LOW / 2)
            .filter(|n| !self.untunable.contains(&n.index))
            .map(|n| v2(n.at))
            .filter(|&p| !look.mine.iter().any(|m| m.reaches(r, p)))
            .filter(|&p| look.mine.iter().any(|m| m.at().distance(p) <= SERVE))
            .min_by(|a, b| a.distance(home).total_cmp(&b.distance(home)))
    }

    /// The node of `r` at `p`, if one is there.
    fn node_at(&self, s: &Sense, r: Resource, p: Vec2) -> Option<u16> {
        s.world
            .economy()?
            .nodes()
            .find(|n| n.resource == r && v2(n.at).distance(p) < 0.5)
            .map(|n| n.index)
    }

    /// Lanes from each hall to the nodes it serves, kept free of buildings.
    fn lanes(&self, s: &Sense, look: &Look) -> Vec<(Vec2, Vec2)> {
        let mut lanes = Vec::new();
        for m in look.mine.iter().filter(|m| m.hub(s.kit.economy)) {
            for n in &look.served {
                if m.at().distance(n.2) <= SERVE {
                    lanes.push((m.at(), n.2));
                }
            }
        }
        lanes
    }

    /// Where to put a building at home: behind the main building, away
    /// from its seeps and veins.
    fn inner_spot(&self, look: &Look) -> Vec2 {
        let home = self.home.unwrap_or(Vec2::ZERO);
        let near: Vec<Vec2> = look
            .served
            .iter()
            .map(|n| n.2)
            .filter(|p| p.distance(home) <= SERVE)
            .collect();
        if near.is_empty() {
            return home + Vec2::new(0.0, 8.0);
        }
        let mid = near.iter().copied().sum::<Vec2>() / near.len() as f32;
        home - (mid - home).normalize_or(Vec2::Y) * 8.0
    }

    /// The production building to add: one that trains fighters it can't
    /// train yet, else the one that trains the most.
    fn production_type<'k>(&self, s: &Sense<'k>, look: &Look) -> Option<&'k KitBuilding> {
        let can =
            |b: &KitBuilding| b.stats.tier <= look.tier.max(1) && b.stats.upgrades_from.is_none();
        let have = |id: BuildingId| look.mine.iter().any(|m| m.view.building == id);
        let fighters = |b: &KitBuilding| {
            s.kit
                .trained_at(s.world, b.id)
                .filter(|u| u.fighter && u.unit.tier <= look.tier.max(1))
                .count()
        };
        let options = s
            .kit
            .buildings
            .iter()
            .filter(|b| b.stats.role == Role::Production && can(b))
            .filter(|b| fighters(b) > 0);
        options
            .clone()
            .filter(|b| !have(b.id))
            .max_by_key(|b| (fighters(b), std::cmp::Reverse(b.stats.cost_grub)))
            .or_else(|| options.max_by_key(|b| (fighters(b), std::cmp::Reverse(b.stats.cost_grub))))
    }

    /// Whether a building the opening asks for is still to go up: its
    /// towers, its production after the first, or its early expansion.
    fn opening_due(&self, s: &Sense, look: &Look) -> bool {
        let early = s.plan.early;
        if early == Early::USUAL {
            return false;
        }
        let count = |f: &dyn Fn(&Mine) -> bool| look.mine.iter().filter(|m| f(m)).count();
        let towers = count(&|m| m.stats.role == Role::Defence);
        let production =
            count(&|m| m.stats.role != Role::Main && trains_fighters(s, m.view.building));
        let expanding = early.expand_at.is_some_and(|t| s.now >= t) && self.hubs_at_home(s, look);
        towers < usize::from(early.towers)
            || (production > 0 && production < usize::from(early.producers))
            || expanding
    }

    /// Whether every hub it has is at home: it hasn't expanded yet.
    fn hubs_at_home(&self, s: &Sense, look: &Look) -> bool {
        self.home.is_some_and(|home| {
            look.mine
                .iter()
                .filter(|m| m.hub(s.kit.economy))
                .all(|m| m.at().distance(home) <= SERVE)
        })
    }

    /// Whether it is time to expand: its seeps are running out, or the
    /// time has come and they are full.
    fn time_to_expand(&self, s: &Sense, look: &Look) -> bool {
        let grub: Vec<_> = look
            .served
            .iter()
            .filter(|n| n.1 == Resource::Grub)
            .collect();
        // The Choir's tuned seeps need nobody: they are as full as they get.
        // Others count as full with as many on each as their level keeps.
        let keeps = (u16::from(GRUB_SLOTS) - 1) * u16::from(s.plan.workers_percent) / 100;
        let full = s.kit.economy == EconomyStyle::Tuned
            || grub.iter().all(|n| look.at_node[usize::from(n.0)] >= keeps);
        let left: u32 = grub.iter().map(|n| n.3).sum();
        let running_out = left < LOW * grub.len().max(1) as u32 || grub.len() < 2;
        let time = s.now >= s.expand_after;
        // An opening that expands early does, seeps full or not, once: while
        // every hub it has is at home.
        let early =
            s.plan.early.expand_at.is_some_and(|t| s.now >= t) && self.hubs_at_home(s, look);
        let rising = look
            .mine
            .iter()
            .any(|m| !m.finished() && m.hub(s.kit.economy));
        // Arming puts off growing, but not replacing seeps that run dry.
        !rising && (running_out || early || (time && full && !self.arming))
    }

    /// The middle of the nearest cluster of Grub seeps no hall of its own
    /// serves, away from any building of anyone else's it has seen and any
    /// enemy in sight.
    fn expansion_site(&self, s: &Sense, look: &Look, home: Vec2) -> Option<Vec2> {
        let economy = s.world.economy()?;
        let structures = s.world.structures()?;
        let theirs: Vec<Vec2> = structures
            .iter()
            .filter(|v| v.owner != s.party && v.seen_by(s.party))
            .map(|v| v2(v.centre()))
            .collect();
        let foes: Vec<Vec2> = s
            .view
            .regiments
            .iter()
            .filter(|r| s.view.hostile(s.party, r.owner))
            .map(|r| r.pos)
            .collect();
        let free: Vec<Vec2> = economy
            .nodes()
            .filter(|n| n.resource == Resource::Grub && n.left > LOW)
            .map(|n| v2(n.at))
            .filter(|&p| !look.served.iter().any(|x| x.2 == p))
            .filter(|&p| look.mine.iter().all(|m| m.at().distance(p) > SERVE))
            .filter(|&p| theirs.iter().all(|t| t.distance(p) > SERVE + 6.0))
            .filter(|&p| foes.iter().all(|f| f.distance(p) > 20.0))
            .collect();
        free.iter()
            .map(|&p| {
                let cluster: Vec<Vec2> = free
                    .iter()
                    .copied()
                    .filter(|q| q.distance(p) <= 12.0)
                    .collect();
                let mid = cluster.iter().copied().sum::<Vec2>() / cluster.len() as f32;
                (p, mid, home.distance(mid) - 25.0 * cluster.len() as f32)
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            // A tuner goes by a seep, to tune it; a hall in the middle.
            .map(|(p, mid, _)| match s.kit.economy {
                EconomyStyle::Tuned => p,
                _ => mid,
            })
    }

    /// Tier up when the economy and army can stand the wait.
    fn tier_up(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        if look.tier >= 3 || look.mine.iter().any(|m| m.view.upgrading.is_some()) {
            return;
        }
        let production = look
            .mine
            .iter()
            .any(|m| m.finished() && trains_fighters(s, m.view.building));
        let workers = working(s, look);
        let army = s.kit.supply_of(look.army.iter().copied());
        let ready = production
            && workers >= 12 + 6 * usize::from(look.tier.saturating_sub(1))
            && army >= 8.0 * f32::from(look.tier);
        if !ready {
            return;
        }
        let Some(main) = look
            .mine
            .iter()
            .find(|m| m.stats.role == Role::Main && m.finished())
        else {
            return;
        };
        let Some(to) = s.kit.upgrade_of(main.view.building) else {
            return;
        };
        if s.world.can_upgrade(s.party, main.view.handle).is_err() {
            if self.ledger.saving == (0, 0) {
                self.ledger.saving = (
                    u32::from(to.stats.cost_grub),
                    u32::from(to.stats.cost_marrow),
                );
            }
            return;
        }
        if self.is_busy(main.view.handle) {
            return;
        }
        if clicks.give(
            s.now,
            Command::Upgrade {
                structure: main.view.handle,
            },
        ) {
            self.busy.push((main.view.handle, None, s.now));
        }
    }

    /// Keep every production building training, towards [`MIX`].
    fn train_army(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        let Some(p) = s.world.production() else {
            return;
        };
        let Some(mut bank) = s.world.bank(s.party) else {
            return;
        };
        // What it has and has queued, by class, and its siege.
        let mut by_class = [0usize; 3];
        let mut breakers = 0;
        let count = |u: UnitId, by_class: &mut [usize; 3], breakers: &mut usize| {
            let Some(k) = s.kit.unit(u).filter(|k| k.fighter) else {
                return;
            };
            if is_breaker(k) {
                *breakers += 1;
            } else if let Some(info) = s.view.units.get(usize::from(u)) {
                by_class[class_slot(info.class)] += 1;
            }
        };
        for r in &look.army {
            count(r.unit, &mut by_class, &mut breakers);
        }
        for m in &look.mine {
            for &u in p.queue_of(m.view.handle) {
                count(u, &mut by_class, &mut breakers);
            }
        }
        let (save_grub, save_marrow) = self.ledger.saving;
        let mix = self.mix(s);
        for m in look.mine.iter().filter(|m| m.finished()) {
            if self.queued(p, m.view.handle) >= QUEUE {
                continue;
            }
            let options: Vec<&KitUnit> = s
                .kit
                .trained_at(s.world, m.view.building)
                .filter(|u| u.fighter && u.unit.tier <= look.tier)
                .collect();
            if options.is_empty() {
                continue;
            }
            let fighters: usize = by_class.iter().sum::<usize>() + breakers;
            let want_breaker = breakers < MAX_BREAKERS && fighters >= PER_BREAKER * (breakers + 1);
            let total = fighters.max(1) as f32;
            let deficit = |u: &&KitUnit| -> f32 {
                if is_breaker(u) {
                    return if want_breaker { 1.0 } else { -1.0 };
                }
                let Some(info) = s.view.units.get(usize::from(u.id)) else {
                    return -1.0;
                };
                let k = class_slot(info.class);
                mix[k] - by_class[k] as f32 / total
            };
            // The class it is shortest of first, the higher tier first
            // within a class; the first it can pay for (so Grub piling up
            // while Marrow runs short still buys regiments).
            let mut options = options;
            options.sort_by(|a, b| {
                deficit(b)
                    .total_cmp(&deficit(a))
                    .then(b.unit.tier.cmp(&a.unit.tier))
            });
            // What is set aside is kept back only from what it costs: a
            // Grub-only regiment doesn't wait on Marrow saved for a tier.
            let paid = |u: &&KitUnit| {
                let (g, m) = (u32::from(u.unit.cost_grub), u32::from(u.unit.cost_marrow));
                (g == 0 || bank.grub >= g + save_grub) && (m == 0 || bank.marrow >= m + save_marrow)
            };
            let Some(unit) = options.into_iter().find(paid) else {
                continue;
            };
            let (g, mr) = (
                u32::from(unit.unit.cost_grub),
                u32::from(unit.unit.cost_marrow),
            );
            if bank.supply_used + u32::from(unit.unit.supply) > bank.supply_cap {
                return;
            }
            if s.world.can_train(s.party, m.view.handle, unit.id).is_err() {
                continue;
            }
            if !clicks.give(
                s.now,
                Command::Train {
                    structure: m.view.handle,
                    unit: unit.id,
                },
            ) {
                return;
            }
            bank.grub -= g;
            bank.marrow -= mr;
            bank.supply_used += u32::from(unit.unit.supply);
            count(unit.id, &mut by_class, &mut breakers);
            self.busy.push((m.view.handle, Some(unit.id), s.now));
        }
    }

    /// Whether it must build up its army before anything else: for a level
    /// that weighs the odds, while its army is smaller than what an enemy
    /// could have by now, or would lose to an enemy army it knows of.
    fn must_arm(&self, s: &Sense, look: &Look) -> bool {
        if !s.plan.weighs_odds {
            return false;
        }
        // Before a timing push it arms for it.
        let pushing_soon = s.push_phase() == sovereign::PushPhase::Arming;
        // At the supply limit there is nothing more to arm with.
        if s.world
            .bank(s.party)
            .is_some_and(|b| b.supply_used + ARMED_SLACK >= SUPPLY_LIMIT)
        {
            return false;
        }
        if pushing_soon {
            return true;
        }
        let minutes = s.now as f32 / (60 * TICKS_PER_SECOND) as f32;
        let supply = s.kit.supply_of(look.army.iter().copied());
        if supply < (ARMY_PER_MINUTE * minutes).min(ARMY_FLOOR) {
            return true;
        }
        let ours = look.army.iter().copied();
        spymaster::enemies(s)
            .into_iter()
            .any(|o| spymaster::odds(s, ours.clone(), spymaster::army_of(s, o)) < OUTMATCHED)
    }

    /// The army make-up it aims for: [`MIX`], or for a level that
    /// counters, shifted towards what beats the enemy armies it has seen:
    /// spears and line against riders, riders against archers, archers
    /// against a line.
    fn mix(&self, s: &Sense) -> [f32; 3] {
        if !s.plan.counters {
            return MIX;
        }
        let [line, shock, ranged] = spymaster::classes(s, spymaster::enemy_army(s));
        if line + shock + ranged == 0.0 {
            return MIX;
        }
        // What beats each of theirs: line beats shock, shock beats ranged,
        // ranged beats line.
        let want = [
            MIX[0] + COUNTER_SHIFT * shock,
            MIX[1] + COUNTER_SHIFT * ranged,
            MIX[2] + COUNTER_SHIFT * line,
        ];
        let all: f32 = want.iter().sum();
        want.map(|w| w / all)
    }

    /// Point every production building's rally at `at`.
    fn rally(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look, at: Vec2) {
        self.rallied
            .retain(|(h, _)| look.mine.iter().any(|m| m.view.handle == *h));
        for m in look.mine.iter().filter(|m| m.finished()) {
            if !trains_fighters(s, m.view.building) {
                continue;
            }
            let k = self.rallied.iter().position(|(h, _)| *h == m.view.handle);
            if k.is_some_and(|k| self.rallied[k].1.distance(at) < 4.0) {
                continue;
            }
            if !clicks.give(
                s.now,
                Command::Rally {
                    structure: m.view.handle,
                    at: to_fx_vec(at),
                },
            ) {
                return;
            }
            match k {
                Some(k) => self.rallied[k].1 = at,
                None => self.rallied.push((m.view.handle, at)),
            }
        }
    }
}

/// How big its economy is, in workers: those it has, or for the Choir,
/// whose workers only build, the workers its tuned nodes stand for.
fn working(s: &Sense, look: &Look) -> usize {
    match s.kit.economy {
        EconomyStyle::Tuned => look
            .served
            .iter()
            .map(|n| match n.1 {
                Resource::Grub => usize::from(GRUB_SLOTS),
                Resource::Marrow => usize::from(MARROW_SLOTS),
            })
            .sum(),
        EconomyStyle::Caravans => look.workers.len() + CARAVAN_WORKERS * look.caravans.len(),
        _ => look.workers.len(),
    }
}

fn grew_from(s: &Sense, building: BuildingId, of: BuildingId) -> bool {
    s.world
        .structures()
        .is_some_and(|t| t.grew_from(building, of))
}

/// Whether building type `b` trains any fighters.
fn trains_fighters(s: &Sense, b: BuildingId) -> bool {
    s.kit.trained_at(s.world, b).any(|u| u.fighter)
}

/// A siege engine or artillery: what knocks buildings down.
fn is_breaker(u: &KitUnit) -> bool {
    matches!(
        u.unit.breaker,
        gc_sim::structures::Breaker::Siege | gc_sim::structures::Breaker::Artillery
    )
}

pub(super) fn class_slot(c: Class) -> usize {
    match c {
        Class::Line => 0,
        Class::Shock => 1,
        Class::Ranged => 2,
    }
}
