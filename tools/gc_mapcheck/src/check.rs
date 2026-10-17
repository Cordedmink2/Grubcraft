//! The checks: is the map valid, and is it fair.

use crate::map::{Ground, Map, Placed};
use gc_map::def::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

/// How far apart (in percent of the larger) two players' numbers may be.
pub const TOLERANCE_PERCENT: u32 = 3;
/// Walkable land with no start that is bigger than this is dead ground.
const DEAD_LAND_TILES: usize = 150;
/// A hamlet must be this many tiles of walking from every start.
const HAMLET_REACH: u32 = 80;
/// The base gate (see [`gate_width`]) must be at least this many tiles wide.
const MIN_GATE_TILES: usize = 12;
/// A rush route is blocked by walling off this many tiles either side of it;
/// a counter-route must survive that.
const BLOCK: i32 = 6;

#[derive(Default)]
pub struct Report {
    pub name: String,
    pub hash: String,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    /// Lines of numbers for the person reading the report.
    pub notes: Vec<String>,
}

impl Report {
    fn err(&mut self, s: impl Into<String>) {
        self.errors.push(s.into());
    }
    fn warn(&mut self, s: impl Into<String>) {
        self.warnings.push(s.into());
    }
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn check(def: &MapDef) -> Report {
    let mut r = Report {
        name: def.name.clone(),
        ..Report::default()
    };
    if !header(def, &mut r) {
        return r;
    }
    let map = Map::build(def);
    r.hash = map.hash();
    let walk = map.walkable();
    let blocked = map.ground.iter().filter(|g| **g == Ground::Blocked).count();
    r.notes.push(format!(
        "{} × {} tiles, {}% cliff or deep water, {} objects",
        map.n,
        map.n,
        blocked * 100 / map.ground.len(),
        map.objects.len()
    ));
    if blocked * 100 / map.ground.len() > 55 {
        r.warn("more than half of the map is cliff or deep water");
    }

    placement(&map, &walk, &mut r);
    // A Field Battle map has armies and starts, no economy: it is checked
    // for placement, for every start reaching every other, and for its set
    // pieces, and its fairness is its symmetry. A big Conquest map (the
    // Wilds made for Conquest) is Field size with an economy.
    let field = matches!(map.size, Some(SizeClass::Field(_))) && !map.is_conquest();
    if !r.ok() || !starts(&map, &mut r) || (!field && !counts(&map, &mut r)) {
        return r;
    }

    let from: Vec<Search> = (0..map.players)
        .map(|p| Search::run(&map, &walk, map.start_of(p).unwrap(), None))
        .collect();
    connectivity(&map, &walk, &from, &mut r);
    if !r.ok() {
        return r;
    }
    if field {
        set_pieces(&map, &walk, &mut r);
        return r;
    }
    areas(&map, &from, &mut r);
    fairness(&map, &from, &alike_starts(def, &map), &mut r);
    routes(&map, &walk, &from, &mut r);
    set_pieces(&map, &walk, &mut r);
    r
}

fn header(def: &MapDef, r: &mut Report) -> bool {
    let mut ok = true;
    if !def.size.players().contains(&def.players) {
        r.err(format!(
            "{:?} maps take {:?} players, not {}",
            def.size,
            def.size.players(),
            def.players
        ));
        ok = false;
    }
    // Each turned copy belongs to the players a turn further round, so the
    // players must split evenly between the copies.
    match (def.symmetry, def.players) {
        (Symmetry::None, _) => {}
        (s, n) if n % s.copies() as u8 == 0 => {}
        (s, n) => {
            r.err(format!("{s:?} symmetry doesn't fit {n} players"));
            ok = false;
        }
    }
    if let Some(o) = def
        .objects
        .iter()
        .find(|o| o.owner.is_some_and(|p| p >= def.players))
    {
        r.err(format!(
            "{:?} at {:?} belongs to player {:?}, but there are {}",
            o.kind, o.at, o.owner, def.players
        ));
        ok = false;
    }
    if def.water < 0 {
        r.err("the water line is below zero");
        ok = false;
    }
    // Heights are sixteenths of a tile; the ground stops at the highest.
    let top = (gc_map::MAX_HEIGHT * 16) as i32;
    let heights = [("the base", def.base), ("the water line", def.water)]
        .into_iter()
        .chain(def.terrain.iter().map(|op| ("a terrain stamp", op.height)));
    for (what, h) in heights {
        if h > top {
            r.err(format!(
                "{what} is at {h}, above the highest ground ({top})"
            ));
            ok = false;
        }
    }
    ok
}

fn cheb(a: Pos, b: Pos) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

fn euclid(a: Pos, b: Pos) -> i32 {
    let (dx, dy) = (i64::from(a.0 - b.0), i64::from(a.1 - b.1));
    ((dx * dx + dy * dy) as u64).isqrt() as i32
}

/// Objects are inside the map, on usable ground and not on top of each other.
fn placement(map: &Map, walk: &[bool], r: &mut Report) {
    for o in &map.objects {
        if !map.in_bounds(o.at) {
            r.err(format!("{:?} at {:?} is outside the map", o.kind, o.at));
            continue;
        }
        let g = map.ground_at(o.at);
        let wet_ok = matches!(
            o.kind,
            Kind::CreepCamp | Kind::FrontierSlot | Kind::Bloodseep
        );
        if g == Ground::Blocked || (g == Ground::Ford && !wet_ok) {
            r.err(format!(
                "{:?} at {:?} stands on {:?} ground",
                o.kind, o.at, g
            ));
        } else if o.kind == Kind::Start && !walk[map.idx(o.at)] {
            r.err(format!("the start at {:?} is hemmed in by cliffs", o.at));
        }
    }
    for (i, a) in map.objects.iter().enumerate() {
        for b in &map.objects[i + 1..] {
            if cheb(a.at, b.at) < 2 {
                r.err(format!(
                    "{:?} at {:?} and {:?} at {:?} overlap",
                    a.kind, a.at, b.kind, b.at
                ));
            }
        }
    }
}

fn count(map: &Map, kind: Kind) -> usize {
    map.objects_of(kind).count()
}

/// Each thing there should be the right number of times.
/// One start for each player, and none for anyone else.
fn starts(map: &Map, r: &mut Report) -> bool {
    let before = r.errors.len();
    for p in 0..map.players {
        let n = map
            .objects_of(Kind::Start)
            .filter(|o| o.owner == Some(p))
            .count();
        if n != 1 {
            r.err(format!("player {p} has {n} starts, not 1"));
        }
    }
    if count(map, Kind::Start) != map.players as usize {
        r.err("a start has an owner outside the player count");
    }
    r.errors.len() == before
}

/// The economy's objects: one Heart, the Warfield's Old Mouth and Bazaar,
/// the frontier slots and the Bloodseep sites.
fn counts(map: &Map, r: &mut Report) -> bool {
    let before = r.errors.len();
    if count(map, Kind::Heart) != 1 {
        r.err(format!(
            "a map needs exactly one Heart, this has {}",
            count(map, Kind::Heart)
        ));
    }
    let warfield = map.size == Some(SizeClass::Warfield);
    let mouths = count(map, Kind::OldMouth);
    if mouths != usize::from(warfield) {
        r.err(format!(
            "{} Old Mouth(s); a Warfield map has one, others none",
            mouths
        ));
    }
    let bazaars = count(map, Kind::Bazaar);
    if bazaars > 1 || (warfield && bazaars == 0) {
        r.err(format!(
            "{bazaars} Bazaars; a Warfield map has one, others at most one"
        ));
    }
    let slots = count(map, Kind::FrontierSlot);
    if !FRONTIER_SLOTS.contains(&slots) {
        r.err(format!(
            "{slots} frontier site slots; a map has {} to {}",
            FRONTIER_SLOTS.start(),
            FRONTIER_SLOTS.end()
        ));
    }
    if count(map, Kind::Bloodseep) < MIN_BLOODSEEPS {
        r.err(format!(
            "a map needs at least {MIN_BLOODSEEPS} Bloodseep sites (Tremors at minutes 12, 24 and 36)"
        ));
    }
    r.errors.len() == before
}

/// A search from one tile over the walkable ground: the cost of the cheapest
/// walk to every tile (ten per open tile) and the way back.
pub struct Search {
    pub dist: Vec<u32>,
    parent: Vec<u32>,
    tiles: i32,
}

const UNSEEN: u32 = u32::MAX;

impl Search {
    /// `closed` is extra ground to treat as blocked.
    pub fn run(map: &Map, walk: &[bool], from: Pos, closed: Option<&[bool]>) -> Search {
        let n = walk.len();
        let mut dist = vec![UNSEEN; n];
        let mut parent = vec![UNSEEN; n];
        let mut heap = BinaryHeap::new();
        dist[map.idx(from)] = 0;
        heap.push(Reverse((0u32, map.idx(from) as u32)));
        let ok = |i: usize| walk[i] && !closed.is_some_and(|c| c[i]);
        while let Some(Reverse((d, i))) = heap.pop() {
            if d > dist[i as usize] {
                continue;
            }
            let (x, y) = (i as i32 % map.n, i as i32 / map.n);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if (dx, dy) == (0, 0) {
                        continue;
                    }
                    let (nx, ny) = (x + dx, y + dy);
                    if !map.in_bounds((nx, ny)) {
                        continue;
                    }
                    let j = map.idx((nx, ny));
                    if !ok(j) {
                        continue;
                    }
                    // No cutting corners.
                    if dx != 0 && dy != 0 && !(ok(map.idx((x + dx, y))) && ok(map.idx((x, y + dy))))
                    {
                        continue;
                    }
                    let base = if dx != 0 && dy != 0 { 14 } else { 10 };
                    let step = base * map.ground[j].cost() / 10;
                    let nd = d + step;
                    if nd < dist[j] {
                        dist[j] = nd;
                        parent[j] = i;
                        heap.push(Reverse((nd, j as u32)));
                    }
                }
            }
        }
        Search {
            dist,
            parent,
            tiles: map.n,
        }
    }

    /// Cost in tenths of a tile to reach something standing on (or beside) a tile.
    pub fn to_object(&self, at: Pos) -> Option<u32> {
        let mut best = UNSEEN;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y) = (at.0 + dx, at.1 + dy);
                if (0..self.tiles).contains(&x) && (0..self.tiles).contains(&y) {
                    best = best.min(self.dist[(y * self.tiles + x) as usize]);
                }
            }
        }
        (best != UNSEEN).then_some(best)
    }

    fn path_to(&self, at: Pos) -> Vec<Pos> {
        let mut out = Vec::new();
        let mut i = (at.1 * self.tiles + at.0) as u32;
        while i != UNSEEN {
            out.push((i as i32 % self.tiles, i as i32 / self.tiles));
            i = self.parent[i as usize];
        }
        out
    }
}

fn tenths(v: u32) -> String {
    format!("{}.{}", v / 10, v % 10)
}

/// Everything can be reached from every start, and no big stretch of land
/// is cut off from all of them.
fn connectivity(map: &Map, walk: &[bool], from: &[Search], r: &mut Report) {
    for o in &map.objects {
        for (p, s) in from.iter().enumerate() {
            if s.to_object(o.at).is_none() {
                r.err(format!(
                    "{:?} at {:?} can't be reached from player {p}'s start",
                    o.kind, o.at
                ));
                break;
            }
        }
    }
    let mut seen: Vec<bool> = from[0].dist.iter().map(|&d| d != UNSEEN).collect();
    for y in 0..map.n {
        for x in 0..map.n {
            let i = map.idx((x, y));
            if !walk[i] || seen[i] {
                continue;
            }
            let (mut size, mut q) = (0, VecDeque::from([(x, y)]));
            seen[i] = true;
            while let Some((cx, cy)) = q.pop_front() {
                size += 1;
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let n = (cx + dx, cy + dy);
                    if map.in_bounds(n) && walk[map.idx(n)] && !seen[map.idx(n)] {
                        seen[map.idx(n)] = true;
                        q.push_back(n);
                    }
                }
            }
            if size >= DEAD_LAND_TILES {
                r.err(format!("{size} tiles of walkable ground near ({x}, {y}) can't be reached from any start"));
            }
        }
    }
}

/// Each start has its base seeps and vein close, a natural expansion a
/// little further, and a hamlet within reach.
fn areas(map: &Map, from: &[Search], r: &mut Report) {
    for p in 0..map.players {
        let start = map.start_of(p).unwrap();
        let own = |kind: Kind| map.objects_of(kind).filter(move |o| o.owner == Some(p));
        let near = |o: &&Placed| euclid(o.at, start) <= BASE_AREA;
        let base_seeps = own(Kind::GrubSeep).filter(near).count();
        let natural = own(Kind::GrubSeep)
            .filter(|o| (BASE_AREA + 1..=EXPANSION_AREA).contains(&euclid(o.at, start)))
            .count();
        let veins = own(Kind::MarrowVein).filter(near).count();
        if base_seeps < 2 {
            r.err(format!(
                "player {p} has {base_seeps} seeps in its base area, not 2"
            ));
        }
        if natural < 2 {
            r.err(format!(
                "player {p} has {natural} seeps for a natural expansion, not 2"
            ));
        }
        if veins < 1 {
            r.err(format!("player {p} has no Marrow vein in its base area"));
        }
        let hamlet = [Kind::Village, Kind::Town, Kind::Bazaar]
            .iter()
            .flat_map(|&k| map.objects_of(k))
            .filter_map(|o| from[p as usize].to_object(o.at))
            .min();
        match hamlet {
            Some(d) if d / 10 <= HAMLET_REACH => {}
            Some(d) => r.err(format!(
                "player {p}'s nearest hamlet is {} tiles away",
                tenths(d)
            )),
            None => r.err(format!("player {p} has no hamlet")),
        }
    }
}

/// Total walking cost to the `k` nearest objects of the kinds (only the
/// ones no player owns, when `unowned_only`).
fn nearest(map: &Map, s: &Search, kinds: &[Kind], unowned_only: bool, k: usize) -> u32 {
    let mut d: Vec<u32> = kinds
        .iter()
        .flat_map(|&kind| map.objects_of(kind))
        .filter(|o| !unowned_only || o.owner.is_none())
        .filter_map(|o| s.to_object(o.at))
        .collect();
    d.sort_unstable();
    d.iter().take(k).sum()
}

fn spread(values: &[u32]) -> u32 {
    let (lo, hi) = (
        u64::from(values.iter().min().copied().unwrap_or(0)),
        u64::from(values.iter().max().copied().unwrap_or(0)),
    );
    ((hi - lo) * 100).checked_div(hi).unwrap_or(0) as u32
}

/// Note one number per player, and make it an error if they differ too
/// much between starts in the same group (`groups`, one per player; all
/// the same group compares every start with every other).
fn compare(
    r: &mut Report,
    label: &str,
    values: &[u32],
    groups: &[usize],
    show: impl Fn(u32) -> String,
) {
    let alike = (0..values.len())
        .map(|g| {
            let group: Vec<u32> = values
                .iter()
                .zip(groups)
                .filter(|&(_, &h)| h == g)
                .map(|(&v, _)| v)
                .collect();
            spread(&group)
        })
        .max()
        .unwrap_or(0);
    let sp = spread(values);
    let shown: Vec<String> = values.iter().map(|&v| format!("{:>7}", show(v))).collect();
    let between = if groups.iter().any(|&g| g != 0) {
        format!(", {alike}% between alike starts")
    } else {
        String::new()
    };
    r.notes.push(format!(
        "{label:<22} {}   spread {sp}%{between}",
        shown.join(" ")
    ));
    if alike > TOLERANCE_PERCENT {
        r.err(format!(
            "unfair: {label} differs by {alike}% between starts (limit {TOLERANCE_PERCENT}%)"
        ));
    }
}

/// Which starts the map's symmetry makes alike, as a group per player:
/// the lowest player whose start is one of this start's turned copies.
/// On a six-player map under `Rot2` that is three pairs of twins, found by
/// where they stand, however their owners are numbered. Every start is in
/// one group when the turned copies take every start round to every
/// other (a Warfield's four), when there is no symmetry, or when no start
/// has a twin.
fn alike_starts(def: &MapDef, map: &Map) -> Vec<usize> {
    let starts: Vec<Pos> = (0..map.players).map(|p| map.start_of(p).unwrap()).collect();
    let groups: Vec<usize> = starts
        .iter()
        .map(|&s| {
            let copies = def.symmetry.tile_copies(s, map.n);
            starts.iter().position(|t| copies.contains(t)).unwrap_or(0)
        })
        .collect();
    let twins = (0..groups.len()).any(|p| groups.iter().filter(|&&g| g == p).count() > 1);
    if twins { groups } else { vec![0; starts.len()] }
}

/// Every start has the same economy round it, near and far. Where a
/// start stands on the map (how far the Heart and the nearest foe are)
/// is fair between the starts the symmetry makes alike (`alike`, see
/// [`alike_starts`]): on a six-player map under `Rot2`, the middle two are
/// nearer the Heart than the corners, as on most six-player maps, and
/// three against three across the middle is fair.
fn fairness(map: &Map, from: &[Search], alike: &[usize], r: &mut Report) {
    let every = vec![0; alike.len()];
    let hamlets = [Kind::Village, Kind::Town, Kind::Bazaar];
    // (name, kinds, only unowned, how many nearest to add up, whether it is
    // where a start stands rather than what is round it, so compared only
    // between the alike starts)
    let metrics: [(&str, &[Kind], bool, usize, bool); 6] = [
        ("4 nearest seeps", &[Kind::GrubSeep], false, 4, false),
        ("nearest vein", &[Kind::MarrowVein], false, 1, false),
        ("Heart", &[Kind::Heart], false, 1, true),
        ("nearest hamlet", &hamlets, false, 1, false),
        (
            "2 nearest contested",
            &[Kind::GrubSeep, Kind::MarrowVein],
            true,
            2,
            false,
        ),
        ("nearest Shrine", &[Kind::Shrine], false, 1, false),
    ];
    for (name, kinds, unowned_only, k, positional) in metrics {
        let v: Vec<u32> = from
            .iter()
            .map(|s| nearest(map, s, kinds, unowned_only, k))
            .collect();
        let groups = if positional { alike } else { &every };
        compare(r, name, &v, groups, tenths);
    }
    let foes: Vec<u32> = (0..map.players)
        .map(|p| {
            let start = map.start_of(p).unwrap();
            map.objects_of(Kind::Start)
                .filter(|o| o.at != start)
                .filter_map(|o| from[p as usize].to_object(o.at))
                .min()
                .unwrap_or(0)
        })
        .collect();
    compare(r, "nearest enemy start", &foes, alike, tenths);

    let curves: Vec<[u32; 3]> = (0..map.players)
        .map(|p| income(map, &from[p as usize], p))
        .collect();
    for (i, minutes) in MINUTES.iter().enumerate() {
        let v: Vec<u32> = curves.iter().map(|c| c[i]).collect();
        compare(r, &format!("income by minute {minutes}"), &v, &every, |v| {
            v.to_string()
        });
    }
}

/// When the economy check looks, in minutes.
const MINUTES: [u32; 3] = [5, 10, 15];

/// A first-pass economy: what a start has gathered (Grub plus Marrow) by
/// minutes 5, 10 and 15 if it takes nodes nearest-first. The base nodes come
/// online at 60 s, each expansion 120 s after the last, plus the walk there.
/// Rates from docs/04: a seep 200 Grub a minute, a vein 50 Marrow, a
/// fresh seep holds 7,500 and a vein 3,000. Meant to be replaced by the real
/// economy in the sim when M3 lands.
fn income(map: &Map, s: &Search, player: u8) -> [u32; 3] {
    let start = map.start_of(player).unwrap();
    // (walk cost, in the base area, is a vein), nearest first.
    let mut nodes: Vec<(u32, bool, bool)> = map
        .objects
        .iter()
        .filter(|o| matches!(o.kind, Kind::GrubSeep | Kind::MarrowVein))
        .filter(|o| o.owner.is_none_or(|w| w == player))
        .filter_map(|o| {
            let near = euclid(o.at, start) <= BASE_AREA;
            s.to_object(o.at)
                .map(|d| (d, near, o.kind == Kind::MarrowVein))
        })
        .collect();
    nodes.sort_unstable();
    // Per node: when it comes online (seconds), its yield per second and its stock.
    let mut expansions = 0.0;
    let nodes: Vec<(f64, f64, f64)> = nodes
        .iter()
        .map(|&(d, near, vein)| {
            let tiles = f64::from(d) / 10.0;
            let online = if near {
                60.0
            } else {
                expansions += 1.0;
                60.0 + 120.0 * expansions + tiles * 0.8
            };
            // Base nodes feed a hall some way off; expansions get their own.
            let eff = if near {
                1.0 / (1.0 + tiles / 60.0)
            } else {
                0.9
            };
            let (rate, cap) = if vein {
                (50.0 / 60.0, 3000.0)
            } else {
                (200.0 / 60.0, 7500.0)
            };
            (online, rate * eff, cap)
        })
        .collect();
    MINUTES.map(|minutes| {
        let end = f64::from(minutes) * 60.0;
        let total: f64 = nodes
            .iter()
            .filter(|n| n.0 < end)
            .map(|&(online, rate, cap)| ((end - online) * rate).min(cap))
            .sum();
        total as u32
    })
}

/// Every base has more than one way out, and every rush has a counter-route.
fn routes(map: &Map, walk: &[bool], from: &[Search], r: &mut Report) {
    for p in 0..map.players {
        let start = map.start_of(p).unwrap();
        let gate = gate_width(&from[p as usize]);
        r.notes
            .push(format!("player {p} base gate: {} tiles wide", gate));
        if gate < MIN_GATE_TILES {
            r.err(format!("player {p}'s base can be sealed by a short wall (gate {gate} tiles, needs {MIN_GATE_TILES})"));
        }
        let Some((foe, cost)) = map
            .objects_of(Kind::Start)
            .filter(|o| o.at != start)
            .filter_map(|o| from[p as usize].to_object(o.at).map(|d| (o.at, d)))
            .min_by_key(|&(_, d)| d)
        else {
            continue;
        };
        let path = from[p as usize].path_to(foe);
        let mut closed = vec![false; walk.len()];
        for &t in &path {
            if euclid(t, start) <= 16 || euclid(t, foe) <= 16 {
                continue;
            }
            for dy in -BLOCK..=BLOCK {
                for dx in -BLOCK..=BLOCK {
                    let n = (t.0 + dx, t.1 + dy);
                    if map.in_bounds(n) {
                        closed[map.idx(n)] = true;
                    }
                }
            }
        }
        let alt = Search::run(map, walk, start, Some(&closed)).to_object(foe);
        match alt {
            None => r.err(format!(
                "player {p}'s rush route to the nearest enemy has no counter-route when blocked"
            )),
            Some(d) if d > cost * 22 / 10 => r.err(format!(
                "player {p}'s counter-route is {} tiles against {} for the rush route (limit 2.2×)",
                tenths(d),
                tenths(cost)
            )),
            Some(d) => r.notes.push(format!(
                "player {p} rush route {} tiles, counter-route {} tiles",
                tenths(cost),
                tenths(d)
            )),
        }
    }
}

/// How many tiles it takes to wall off a base: walkable tiles whose walk
/// from the start costs between 26 and 27.5 tiles. That ring is a cross-section
/// of every way out, so a base that opens through a narrow gap has a small one.
fn gate_width(s: &Search) -> usize {
    s.dist.iter().filter(|&&d| (260..=275).contains(&d)).count()
}

/// The author's set-piece list is real: two or more, each on ground that
/// suits it.
fn set_pieces(map: &Map, walk: &[bool], r: &mut Report) {
    if map.set_pieces.len() < 2 {
        r.err(format!(
            "{} set pieces; a map needs at least 2 spots for a great Field Battle",
            map.set_pieces.len()
        ));
    }
    for sp in &map.set_pieces {
        if !map.in_bounds(sp.at) || !walk[map.idx(sp.at)] {
            r.err(format!(
                "set piece '{}' at {:?} isn't walkable ground",
                sp.label, sp.at
            ));
            continue;
        }
        match sp.kind {
            SetPieceKind::High => {
                // Half a tile above the average of the ring ten tiles out.
                let here = i32::from(map.heights[map.idx(sp.at)]);
                let (mut sum, mut n) = (0, 0);
                for k in 0..16 {
                    let a = f64::from(k) * std::f64::consts::TAU / 16.0;
                    let q = (
                        sp.at.0 + (10.0 * a.cos()).round() as i32,
                        sp.at.1 + (10.0 * a.sin()).round() as i32,
                    );
                    if map.in_bounds(q) {
                        sum += i32::from(map.heights[map.idx(q)]);
                        n += 1;
                    }
                }
                if n == 0 || here - sum / n < gc_sim::terrain::HIGH_GROUND {
                    r.err(format!(
                        "set piece '{}' at {:?} isn't half a tile above its surroundings",
                        sp.label, sp.at
                    ));
                }
            }
            SetPieceKind::Ford => {
                let fords = (-4..=4)
                    .flat_map(|dy| (-4..=4).map(move |dx| (sp.at.0 + dx, sp.at.1 + dy)))
                    .filter(|&q| map.ground_at(q) == Ground::Ford)
                    .count();
                if fords < 12 {
                    r.err(format!(
                        "set piece '{}' at {:?} has only {fords} ford tiles around it",
                        sp.label, sp.at
                    ));
                }
            }
        }
    }
}
