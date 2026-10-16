//! The map as the checks see it: `gc_map`'s map, with each tile's ground
//! worked out once from the walkable-ground grid the simulation uses, so
//! the checker and the game agree on every cliff and ford.

use gc_map::def::{MapDef, Pos};
pub use gc_map::{Ground, Placed};

pub struct Map {
    pub map: gc_map::Map,
    /// Tiles per side, signed for the checks' arithmetic.
    pub n: i32,
    /// The walkable-ground grid, built once.
    pub nav: Vec<u8>,
    /// Each tile's ground ([`gc_map::Map::tile_ground`]).
    pub ground: Vec<Ground>,
}

impl std::ops::Deref for Map {
    type Target = gc_map::Map;
    fn deref(&self) -> &gc_map::Map {
        &self.map
    }
}

impl Map {
    pub fn build(def: &MapDef) -> Map {
        let map = gc_map::Map::from_def(def);
        let n = map.tiles as i32;
        let nav = map.nav_costs();
        let ground = map.tile_ground(&nav);
        Map {
            map,
            n,
            nav,
            ground,
        }
    }

    pub fn idx(&self, p: Pos) -> usize {
        (p.1 * self.n + p.0) as usize
    }

    pub fn in_bounds(&self, p: Pos) -> bool {
        (0..self.n).contains(&p.0) && (0..self.n).contains(&p.1)
    }

    pub fn ground_at(&self, p: Pos) -> Ground {
        if self.in_bounds(p) {
            self.ground[self.idx(p)]
        } else {
            Ground::Blocked
        }
    }

    /// Tiles a regiment's centre can use: walkable, and not touching a
    /// blocked tile (the sim keeps blocks about a tile off cliffs).
    pub fn walkable(&self) -> Vec<bool> {
        self.map.standable(&self.ground)
    }

    pub fn start_of(&self, player: u8) -> Option<Pos> {
        self.objects_of(gc_map::Kind::Start)
            .find(|o| o.owner == Some(player))
            .map(|o| o.at)
    }

    /// The fingerprint the game compares between machines, in hex.
    pub fn hash(&self) -> String {
        format!("{:016x}", gc_map::map_hash(&self.nav, &self.map.ground()))
    }
}
