//! The Grubfolk's villages, towns and bazaar (docs/34-ambient-life.md,
//! stage 5a): until now these sites were only a clearing in the scenery.
//! Each gets its houses: huts and cottages round a green with a well, a
//! barn, haystacks and runs of wattle fence, and market stalls in a town or
//! bazaar. They are scenery models placed once from the map's own objects
//! (a hash of the site's tile, so a place always looks the same), picture
//! only: nothing blocks soldiers and nothing is stored in the simulation.

use gc_map::Map;
use gc_map::def::Kind;
use glam::{Vec2, Vec3};

use crate::scenery::{Extra, Model, turn_of};

/// The ground under a building may vary this much over its footprint (in
/// tiles), and a building must stand this far above the water.
const LEVEL: f32 = 0.45;
const DRY: f32 = 0.2;
/// Slack when two things' footprints meet, in tiles.
const TOUCH: f32 = 0.3;

/// What a site is: how big its hamlet is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Size {
    Village,
    Town,
    Bazaar,
}

/// The things a hamlet is made of, with the radius of the ground each
/// takes, in tiles.
fn radius(model: Model) -> f32 {
    match model {
        Model::GrubfolkHut => 1.9,
        Model::GrubfolkHouse => 2.4,
        Model::GrubfolkBarn => 3.0,
        Model::GrubfolkWell => 0.9,
        Model::Haystack => 1.1,
        Model::WattleFence => 1.0,
        Model::MarketStall => 1.3,
        _ => 1.0,
    }
}

/// A small random stream from a seed (xorshift).
struct Rng(u32);

impl Rng {
    fn new(site: (i32, i32), size: Size) -> Rng {
        let mut x = (site.0 as u32)
            .wrapping_mul(0x9e37_79b1)
            .wrapping_add((site.1 as u32).wrapping_mul(0x85eb_ca6b))
            .wrapping_add(size as u32 * 0x27d4_eb2f)
            | 1;
        for _ in 0..4 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
        }
        Rng(x)
    }

    fn unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }
}

/// The ground the hamlet is laid out on.
struct Ground<'a> {
    map: &'a Map,
}

impl Ground<'_> {
    fn height(&self, p: Vec2) -> f32 {
        self.map.height_at(p.x, p.y).max(self.map.water_level())
    }

    /// Whether something of `reach` radius may stand at `p`: dry, off the
    /// woods, and the ground under it level enough.
    fn suits(&self, p: Vec2, reach: f32) -> bool {
        let r = reach * 0.7;
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        let side = self.map.tiles as i32;
        for d in [
            Vec2::ZERO,
            Vec2::X,
            Vec2::NEG_X,
            Vec2::Y,
            Vec2::NEG_Y,
            Vec2::ONE,
            Vec2::NEG_ONE,
            Vec2::new(1.0, -1.0),
            Vec2::new(-1.0, 1.0),
        ] {
            let q = p + d * r;
            let h = self.map.height_at(q.x, q.y);
            low = low.min(h);
            high = high.max(h);
            let (tx, ty) = (q.x.floor() as i32, q.y.floor() as i32);
            if (0..side).contains(&tx)
                && (0..side).contains(&ty)
                && self.wooded((ty * side + tx) as usize)
            {
                return false;
            }
        }
        low > self.map.water_level() + DRY && high - low <= LEVEL
    }

    /// Whether a tile (row by row) is woods, thicket or not.
    fn wooded(&self, tile: usize) -> bool {
        self.map.cover.get(tile) == Some(&(gc_sim::Cover::Wood as u8)) || self.map.is_thicket(tile)
    }
}

/// What has been put down so far in one hamlet.
struct Hamlet<'a> {
    ground: Ground<'a>,
    centre: Vec2,
    rng: Rng,
    placed: Vec<(Vec2, f32)>,
    out: Vec<Extra>,
}

impl Hamlet<'_> {
    /// Stand `model` at `at`, its front toward `facing`, if the ground
    /// suits it and it keeps off what is there.
    fn put(&mut self, model: Model, at: Vec2, facing: Vec2) -> bool {
        let scale = 0.96 + 0.1 * self.rng.unit();
        let r = radius(model) * scale;
        if !self.ground.suits(at, r)
            || self
                .placed
                .iter()
                .any(|&(p, pr)| p.distance(at) < r + pr - TOUCH)
        {
            return false;
        }
        self.out.push(Extra {
            model,
            at: Vec3::new(at.x, self.ground.height(at), at.y),
            turn: turn_of(facing),
            scale,
            tint: [128; 3],
            phase: 0,
            lean: 0,
            fall: 0,
        });
        self.placed.push((at, r));
        true
    }

    /// A point `radius` out from the middle at `angle`.
    fn on(&self, radius: f32, angle: f32) -> Vec2 {
        self.centre + Vec2::from_angle(angle) * radius
    }

    /// A ring of `models` round the middle at `round`, their fronts
    /// toward it, spread evenly from `start` with a little give.
    fn ring(&mut self, models: &[Model], round: f32, start: f32) {
        let n = models.len();
        // The biggest first, so a barn is not squeezed out by its neighbours.
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| radius(models[b]).total_cmp(&radius(models[a])));
        for k in order {
            let m = models[k];
            let jitter = (self.rng.unit() - 0.5) * 0.2;
            let angle = start + (k as f32 + jitter) * std::f32::consts::TAU / n as f32;
            let at = self.on(round, angle);
            let facing = (self.centre - at).normalize_or(Vec2::Y);
            self.put(m, at, facing);
        }
    }

    /// Haystacks and then fence runs out at `radius`, where the ring of
    /// buildings has left room.
    fn edge(&mut self, haystacks: usize, radius: f32) {
        let start = self.rng.unit() * std::f32::consts::TAU;
        let mut made = 0;
        for k in 0..16 {
            if made >= haystacks {
                break;
            }
            let angle = start + k as f32 * 0.83;
            let at = self.on(radius - 0.6, angle);
            if self.put(Model::Haystack, at, Vec2::from_angle(angle)) {
                made += 1;
            }
        }
        let n = (std::f32::consts::TAU * radius / 2.0) as usize;
        for k in 0..n {
            if self.rng.unit() < 0.55 {
                continue;
            }
            let angle = start + k as f32 * std::f32::consts::TAU / n as f32;
            let at = self.on(radius, angle);
            // Along the circle, the front facing out.
            self.put(Model::WattleFence, at, Vec2::from_angle(angle));
        }
    }
}

/// The models of a hamlet of `size` laid round `centre`.
fn lay(map: &Map, centre: Vec2, site: (i32, i32), size: Size) -> Vec<Extra> {
    use Model::*;
    let mut h = Hamlet {
        ground: Ground { map },
        centre,
        rng: Rng::new(site, size),
        placed: Vec::new(),
        out: Vec::new(),
    };
    let start = h.rng.unit() * std::f32::consts::TAU;
    // The well, or the market's well, in the middle.
    h.put(GrubfolkWell, centre, Vec2::Y);
    match size {
        Size::Village => {
            h.ring(
                &[
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkBarn,
                ],
                5.8,
                start,
            );
            h.edge(2, 8.5);
        }
        Size::Town => {
            for k in 0..3 {
                let angle = start + k as f32 * std::f32::consts::TAU / 3.0 + 0.5;
                let at = h.on(2.9, angle);
                h.put(MarketStall, at, (centre - at).normalize_or(Vec2::Y));
            }
            h.ring(
                &[
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkBarn,
                ],
                6.8,
                start,
            );
            h.edge(2, 9.0);
        }
        Size::Bazaar => {
            h.ring(&[MarketStall; 7], 3.4, start);
            h.ring(
                &[
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                ],
                7.4,
                start + 0.3,
            );
            h.edge(2, 9.0);
        }
    }
    h.out
}

/// Whether a building stands on the ground under `e`: any tile under the
/// middle of its footprint is occupied.
fn built_on(e: &Extra, occupied: &impl Fn((i32, i32)) -> bool) -> bool {
    let r = (radius(e.model) * e.scale * 0.8).max(0.5);
    let (x0, x1) = ((e.at.x - r).floor() as i32, (e.at.x + r).floor() as i32);
    let (y0, y1) = ((e.at.z - r).floor() as i32, (e.at.z + r).floor() as i32);
    (y0..=y1).any(|y| (x0..=x1).any(|x| occupied((x, y))))
}

/// The hamlets' scenery, kept for the match and handed to the scenery pass
/// near the camera.
#[derive(Default)]
pub struct Hamlets {
    /// Every model of every hamlet, grouped by hamlet: the middle of its
    /// site and the models laid there.
    sites: Vec<(Vec2, Vec<Extra>)>,
}

/// How far from its middle a hamlet's models reach, in tiles.
const SPREAD: f32 = 12.0;

impl Hamlets {
    /// Lay out the hamlets of `map`'s Villages, Towns and Bazaar.
    pub fn new(map: &Map) -> Hamlets {
        let sites = map
            .objects
            .iter()
            .filter_map(|o| {
                let size = match o.kind {
                    Kind::Village => Size::Village,
                    Kind::Town => Size::Town,
                    Kind::Bazaar => Size::Bazaar,
                    _ => return None,
                };
                let centre = Vec2::new(o.at.0 as f32 + 0.5, o.at.1 as f32 + 0.5);
                Some((centre, lay(map, centre, o.at, size)))
            })
            .collect();
        Hamlets { sites }
    }

    /// The models of the hamlets within `reach` tiles of `focus`, but not
    /// those whose ground a player has built on: `occupied` says whether a
    /// tile has a building (or its margin) on it.
    pub fn near<'a>(
        &'a self,
        focus: Vec2,
        reach: f32,
        occupied: impl Fn((i32, i32)) -> bool + 'a,
    ) -> impl Iterator<Item = Extra> + 'a {
        self.sites
            .iter()
            .filter(move |(c, _)| c.distance(focus) <= reach + SPREAD)
            .flat_map(|(_, extras)| extras.iter().copied())
            .filter(move |e| !built_on(e, &occupied))
    }

    /// The middle of the first site, if there is one.
    pub fn first(&self) -> Option<Vec2> {
        self.sites.first().map(|(c, _)| *c)
    }

    /// How many sites have been laid out.
    #[cfg(test)]
    fn count(&self) -> usize {
        self.sites.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_map::Placed;

    /// Every shipped Conquest map and a Wilds land.
    fn maps() -> Vec<Map> {
        (0..=gc_map::SHIPPED.len() + 1)
            .map(|choice| crate::conquest::make_map(choice, 7))
            .collect()
    }

    /// The sites of a test map: the kind and the tile.
    type Sites = Vec<(Kind, (i32, i32))>;

    /// A level, bare, dry map with one site of each size, far apart.
    fn flat() -> (Map, Sites) {
        let mut map = crate::conquest::make_map(0, 7);
        let side = map.tiles as i32;
        let top = map.max_height();
        let sample = ((map.water_level() + 2.0) / top * f32::from(u16::MAX)) as u16;
        map.samples.fill(sample);
        map.cover.fill(0);
        map.thicket.clear();
        let sites = vec![
            (Kind::Village, (side / 4, side / 4)),
            (Kind::Town, (side / 2, side / 2)),
            (Kind::Bazaar, (side * 3 / 4, side * 3 / 4)),
        ];
        map.objects = sites
            .iter()
            .map(|&(kind, at)| Placed {
                kind,
                at,
                owner: None,
                label: String::new(),
            })
            .collect();
        (map, sites)
    }

    fn count(extras: &[Extra], model: Model) -> usize {
        extras.iter().filter(|e| e.model == model).count()
    }

    fn site(h: &Hamlets, at: (i32, i32)) -> &[Extra] {
        let centre = Vec2::new(at.0 as f32 + 0.5, at.1 as f32 + 0.5);
        &h.sites.iter().find(|(c, _)| *c == centre).unwrap().1
    }

    #[test]
    fn every_village_gets_houses_on_dry_ground_and_the_same_each_time() {
        let (mut sites, mut with_houses) = (0, 0);
        for map in maps() {
            let (a, b) = (Hamlets::new(&map), Hamlets::new(&map));
            let wanted = map
                .objects
                .iter()
                .filter(|o| matches!(o.kind, Kind::Village | Kind::Town | Kind::Bazaar))
                .count();
            assert_eq!(a.count(), wanted);
            sites += wanted;
            for ((ca, ea), (_, eb)) in a.sites.iter().zip(&b.sites) {
                assert_eq!(ea, eb);
                with_houses += usize::from(
                    ea.iter()
                        .any(|e| matches!(e.model, Model::GrubfolkHut | Model::GrubfolkHouse)),
                );
                for e in ea {
                    assert!(e.at.y > map.water_level());
                    assert!(Vec2::new(e.at.x, e.at.z).distance(*ca) < SPREAD);
                }
            }
        }
        assert!(sites > 0, "no test map has a village");
        assert!(with_houses > 0);
    }

    #[test]
    fn no_house_stands_in_a_wood() {
        for map in maps() {
            let side = map.tiles as i32;
            for (_, extras) in &Hamlets::new(&map).sites {
                for e in extras {
                    let tile = e.at.z.floor() as i32 * side + e.at.x.floor() as i32;
                    let wood = map.cover[tile as usize] == gc_sim::Cover::Wood as u8
                        || map.is_thicket(tile as usize);
                    assert!(!wood, "{e:?} stands in a wood");
                }
            }
        }
    }

    #[test]
    fn nothing_in_a_hamlet_overlaps_its_neighbours() {
        for map in maps() {
            for (_, extras) in &Hamlets::new(&map).sites {
                for (i, a) in extras.iter().enumerate() {
                    for b in &extras[i + 1..] {
                        let d = Vec2::new(a.at.x - b.at.x, a.at.z - b.at.z).length();
                        let (ra, rb) = (radius(a.model) * a.scale, radius(b.model) * b.scale);
                        assert!(d >= ra + rb - TOUCH - 1e-3, "{a:?} {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn level_dry_ground_gives_each_size_its_whole_hamlet() {
        let (map, sites) = flat();
        let h = Hamlets::new(&map);
        for (kind, at) in sites {
            let e = site(&h, at);
            let cottages = count(e, Model::GrubfolkHut) + count(e, Model::GrubfolkHouse);
            let (cottages_want, stalls, barns) = match kind {
                Kind::Village => (4, 0, 1),
                Kind::Town => (5, 3, 1),
                _ => (5, 7, 0),
            };
            assert_eq!(cottages, cottages_want, "{kind:?}");
            assert_eq!(count(e, Model::MarketStall), stalls, "{kind:?}");
            assert_eq!(count(e, Model::GrubfolkBarn), barns, "{kind:?}");
            assert_eq!(count(e, Model::GrubfolkWell), 1, "{kind:?}");
            assert!(count(e, Model::Haystack) > 0, "{kind:?}");
            assert!(count(e, Model::WattleFence) > 0, "{kind:?}");
        }
    }

    #[test]
    fn a_site_in_a_wood_or_under_water_gets_nothing() {
        let (mut map, _) = flat();
        map.cover.fill(gc_sim::Cover::Wood as u8);
        assert!(
            Hamlets::new(&map)
                .sites
                .iter()
                .all(|(_, extras)| extras.is_empty())
        );
        let (mut map, _) = flat();
        map.samples.fill(0);
        assert!(
            Hamlets::new(&map)
                .sites
                .iter()
                .all(|(_, extras)| extras.is_empty())
        );
    }

    #[test]
    fn only_hamlets_near_the_camera_and_off_players_buildings_are_handed_over() {
        let (map, sites) = flat();
        let h = Hamlets::new(&map);
        let none = |_| false;
        let all = h.near(Vec2::ZERO, 1.0e6, none).count();
        assert!(all > 0);
        assert_eq!(h.near(Vec2::splat(-1.0e6), 100.0, none).count(), 0);
        // A building on the village green hides what stands on it, not the
        // far side of the village or other hamlets.
        let (_, at) = sites[0];
        let green = |t: (i32, i32)| t == at;
        let left = h.near(Vec2::ZERO, 1.0e6, green).count();
        assert!(left < all && left > all - site(&h, at).len());
        assert_eq!(h.near(Vec2::ZERO, 1.0e6, |_| true).count(), 0);
    }
}
