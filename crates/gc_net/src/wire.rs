//! Byte encoding: little helpers for writing and reading packets, and the
//! compact form of simulation commands (docs/11-networking.md, "Bandwidth").
//!
//! Integers are LEB128 varints (signed ones zigzagged first), so small
//! numbers cost one byte. Readers never panic on bad input: every read
//! returns `None` once the bytes run out or make no sense, and a packet that
//! fails to decode is dropped.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::{Command, Formation, Mass, PlayerCommand, RegimentHandle, StructureHandle, UnitStats};

#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn new() -> Writer {
        Writer::default()
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }

    pub fn var(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.buf.push(v as u8 | 0x80);
            v >>= 7;
        }
        self.buf.push(v as u8);
    }

    pub fn ivar(&mut self, v: i64) {
        self.var(((v << 1) ^ (v >> 63)) as u64);
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.var(b.len() as u64);
        self.buf.extend_from_slice(b);
    }

    pub fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }

    pub fn fx(&mut self, v: Fx) {
        self.ivar(i64::from(v.raw()));
    }

    pub fn vec(&mut self, v: FxVec2) {
        self.fx(v.x);
        self.fx(v.y);
    }

    pub fn handle(&mut self, h: RegimentHandle) {
        self.var(u64::from(h.index));
        self.var(u64::from(h.generation));
    }

    /// A building's handle, laid out as a regiment's.
    pub fn structure(&mut self, h: StructureHandle) {
        self.var(u64::from(h.index));
        self.var(u64::from(h.generation));
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Reader<'a> {
        Reader { buf }
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn u8(&mut self) -> Option<u8> {
        let (&b, rest) = self.buf.split_first()?;
        self.buf = rest;
        Some(b)
    }

    pub fn bool(&mut self) -> Option<bool> {
        match self.u8()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    pub fn var(&mut self) -> Option<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    }

    /// A varint that must fit in `T`.
    pub fn var_as<T: TryFrom<u64>>(&mut self) -> Option<T> {
        T::try_from(self.var()?).ok()
    }

    pub fn ivar(&mut self) -> Option<i64> {
        let v = self.var()?;
        Some((v >> 1) as i64 ^ -((v & 1) as i64))
    }

    pub fn u64(&mut self) -> Option<u64> {
        let b = self.take(8)?;
        Some(u64::from_le_bytes(b.try_into().ok()?))
    }

    pub fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.buf.len() {
            return None;
        }
        let (head, rest) = self.buf.split_at(n);
        self.buf = rest;
        Some(head)
    }

    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.var_as::<usize>()?;
        self.take(n)
    }

    pub fn str(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?.to_vec()).ok()
    }

    pub fn fx(&mut self) -> Option<Fx> {
        Some(Fx::from_raw(i32::try_from(self.ivar()?).ok()?))
    }

    pub fn vec(&mut self) -> Option<FxVec2> {
        Some(FxVec2 {
            x: self.fx()?,
            y: self.fx()?,
        })
    }

    pub fn handle(&mut self) -> Option<RegimentHandle> {
        Some(RegimentHandle {
            index: self.var_as()?,
            generation: self.var_as()?,
        })
    }

    pub fn structure(&mut self) -> Option<StructureHandle> {
        Some(StructureHandle {
            index: self.var_as()?,
            generation: self.var_as()?,
        })
    }
}

fn write_stats(w: &mut Writer, s: &UnitStats) {
    w.fx(s.speed);
    w.var(u64::from(s.turn_rate));
    w.var(u64::from(s.models));
    w.var(u64::from(s.hp_per_model));
    w.var(u64::from(s.files));
    w.fx(s.file_spacing);
    w.fx(s.rank_spacing);
    w.u8(s.melee_attack);
    w.u8(s.melee_defence);
    w.var(u64::from(s.melee_damage));
    w.u8(s.charge_bonus);
    w.fx(s.range);
    w.var(u64::from(s.missile_damage));
    w.var(u64::from(s.reload_ticks));
    w.u8(s.accuracy);
    w.u8(s.morale);
    w.u8(s.mass as u8);
    w.u8(s.weapons());
}

fn read_stats(r: &mut Reader) -> Option<UnitStats> {
    UnitStats {
        speed: r.fx()?,
        turn_rate: r.var_as()?,
        models: r.var_as()?,
        hp_per_model: r.var_as()?,
        files: r.var_as()?,
        file_spacing: r.fx()?,
        rank_spacing: r.fx()?,
        melee_attack: r.u8()?,
        melee_defence: r.u8()?,
        melee_damage: r.var_as()?,
        charge_bonus: r.u8()?,
        range: r.fx()?,
        missile_damage: r.var_as()?,
        reload_ticks: r.var_as()?,
        accuracy: r.u8()?,
        morale: r.u8()?,
        mass: Mass::from_u8(r.u8()?)?,
        reach: false,
        swing_room: false,
        shield: false,
    }
    .with_weapons(r.u8()?)
}

pub fn write_command(w: &mut Writer, c: &Command) {
    match *c {
        Command::SpawnRegiment {
            unit,
            position,
            facing,
        } => {
            w.u8(0);
            w.var(u64::from(unit));
            w.vec(position);
            w.var(u64::from(facing.0));
        }
        Command::MoveTo {
            regiment,
            target,
            facing,
            files,
            pace,
        } => {
            w.u8(1);
            w.handle(regiment);
            w.vec(target);
            match facing {
                Some(a) => w.var(u64::from(a.0) + 1),
                None => w.var(0),
            }
            w.var(u64::from(files));
            w.fx(pace);
        }
        Command::Attack { regiment, target } => {
            w.u8(2);
            w.handle(regiment);
            w.handle(target);
        }
        Command::RemoveRegiment { regiment } => {
            w.u8(3);
            w.handle(regiment);
        }
        Command::TuneUnit { unit, stats } => {
            w.u8(4);
            w.var(u64::from(unit));
            write_stats(w, &stats);
        }
        Command::SetFormation {
            regiment,
            formation,
        } => {
            w.u8(5);
            w.handle(regiment);
            w.u8(formation as u8);
        }
        Command::DeployZone { center, radius } => {
            w.u8(6);
            w.vec(center);
            w.fx(radius);
        }
        Command::Ready => w.u8(7),
        Command::SpawnStructure { building, tile } => {
            w.u8(8);
            w.var(u64::from(building));
            // Two's complement, so tiles off the map's north-west edge
            // still travel (and are refused by the sim).
            w.var(u64::from(tile.0 as u16));
            w.var(u64::from(tile.1 as u16));
        }
        Command::AttackStructure {
            regiment,
            structure,
        } => {
            w.u8(9);
            w.handle(regiment);
            w.structure(structure);
        }
        Command::RemoveStructure { structure } => {
            w.u8(10);
            w.structure(structure);
        }
        Command::Gather { regiment, node } => {
            w.u8(11);
            w.handle(regiment);
            w.var(u64::from(node));
        }
        Command::Build {
            regiment,
            building,
            tile,
        } => {
            w.u8(12);
            w.handle(regiment);
            w.var(u64::from(building));
            w.var(u64::from(tile.0 as u16));
            w.var(u64::from(tile.1 as u16));
        }
        Command::Construct {
            regiment,
            structure,
        } => {
            w.u8(13);
            w.handle(regiment);
            w.structure(structure);
        }
        Command::CallOff { structure } => {
            w.u8(14);
            w.structure(structure);
        }
        Command::Train { structure, unit } => {
            w.u8(15);
            w.structure(structure);
            w.var(u64::from(unit));
        }
        Command::Untrain { structure } => {
            w.u8(16);
            w.structure(structure);
        }
        Command::Upgrade { structure } => {
            w.u8(17);
            w.structure(structure);
        }
        Command::Rally { structure, at } => {
            w.u8(18);
            w.structure(structure);
            w.vec(at);
        }
        Command::FallBack { regiment } => {
            w.u8(19);
            w.handle(regiment);
        }
        Command::Brace { regiment, on } => {
            w.u8(20);
            w.handle(regiment);
            w.bool(on);
        }
    }
}

pub fn read_command(r: &mut Reader) -> Option<Command> {
    Some(match r.u8()? {
        0 => Command::SpawnRegiment {
            unit: r.var_as()?,
            position: r.vec()?,
            facing: Angle(r.var_as()?),
        },
        1 => Command::MoveTo {
            regiment: r.handle()?,
            target: r.vec()?,
            facing: match r.var()? {
                0 => None,
                a => Some(Angle(u16::try_from(a - 1).ok()?)),
            },
            files: r.var_as()?,
            pace: r.fx()?,
        },
        2 => Command::Attack {
            regiment: r.handle()?,
            target: r.handle()?,
        },
        3 => Command::RemoveRegiment {
            regiment: r.handle()?,
        },
        4 => Command::TuneUnit {
            unit: r.var_as()?,
            stats: read_stats(r)?,
        },
        5 => Command::SetFormation {
            regiment: r.handle()?,
            formation: Formation::from_u8(r.u8()?)?,
        },
        6 => Command::DeployZone {
            center: r.vec()?,
            radius: r.fx()?,
        },
        7 => Command::Ready,
        8 => Command::SpawnStructure {
            building: r.var_as()?,
            tile: (r.var_as::<u16>()? as i16, r.var_as::<u16>()? as i16),
        },
        9 => Command::AttackStructure {
            regiment: r.handle()?,
            structure: r.structure()?,
        },
        10 => Command::RemoveStructure {
            structure: r.structure()?,
        },
        11 => Command::Gather {
            regiment: r.handle()?,
            node: r.var_as()?,
        },
        12 => Command::Build {
            regiment: r.handle()?,
            building: r.var_as()?,
            tile: (r.var_as::<u16>()? as i16, r.var_as::<u16>()? as i16),
        },
        13 => Command::Construct {
            regiment: r.handle()?,
            structure: r.structure()?,
        },
        14 => Command::CallOff {
            structure: r.structure()?,
        },
        15 => Command::Train {
            structure: r.structure()?,
            unit: r.var_as()?,
        },
        16 => Command::Untrain {
            structure: r.structure()?,
        },
        17 => Command::Upgrade {
            structure: r.structure()?,
        },
        18 => Command::Rally {
            structure: r.structure()?,
            at: r.vec()?,
        },
        19 => Command::FallBack {
            regiment: r.handle()?,
        },
        20 => Command::Brace {
            regiment: r.handle()?,
            on: r.bool()?,
        },
        _ => return None,
    })
}

/// Commands with the player who issued each.
pub fn write_player_commands(w: &mut Writer, cmds: &[PlayerCommand]) {
    w.var(cmds.len() as u64);
    for c in cmds {
        w.u8(c.player);
        write_command(w, &c.command);
    }
}

pub fn read_player_commands(r: &mut Reader) -> Option<Vec<PlayerCommand>> {
    let n = r.var_as::<usize>()?;
    // Each command takes at least two bytes: don't trust a huge count.
    let mut out = Vec::with_capacity(n.min(r.buf.len() / 2));
    for _ in 0..n {
        out.push(PlayerCommand {
            player: r.u8()?,
            command: read_command(r)?,
        });
    }
    Some(out)
}

/// Bytes `c` takes on the wire, for packing bundles under the packet size.
pub fn command_size(c: &PlayerCommand) -> usize {
    let mut w = Writer::new();
    w.u8(c.player);
    write_command(&mut w, &c.command);
    w.buf.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(index: u32, generation: u16) -> RegimentHandle {
        RegimentHandle { index, generation }
    }

    fn v(x: i32, y: i32) -> FxVec2 {
        FxVec2 {
            x: Fx::from_raw(x),
            y: Fx::from_raw(y),
        }
    }

    const U_CASES: [u64; 7] = [0, 1, 127, 128, 300, u32::MAX as u64, u64::MAX];
    const I_CASES: [i64; 8] = [0, -1, 1, -64, 64, i32::MIN as i64, i64::MAX, i64::MIN];

    #[test]
    fn varints_round_trip() {
        let mut w = Writer::new();
        for x in U_CASES {
            w.var(x);
        }
        for x in I_CASES {
            w.ivar(x);
        }
        let mut r = Reader::new(&w.buf);
        for x in U_CASES {
            assert_eq!(r.var(), Some(x));
        }
        for x in I_CASES {
            assert_eq!(r.ivar(), Some(x));
        }
        assert!(r.is_empty());
        assert_eq!(r.u8(), None);
    }

    #[test]
    fn every_command_round_trips() {
        let stats = gc_sim::scenario::DRILL_UNITS[0];
        let cmds = [
            Command::SpawnRegiment {
                unit: 3,
                position: v(-5000, 70_000),
                facing: Angle(65_535),
            },
            Command::MoveTo {
                regiment: h(1535, 9),
                target: v(1 << 20, -(1 << 20)),
                facing: Some(Angle(0)),
                files: 12,
                pace: Fx::ZERO,
            },
            Command::MoveTo {
                regiment: h(0, 0),
                target: v(0, 0),
                facing: None,
                files: 0,
                pace: Fx::from_raw(77),
            },
            Command::Attack {
                regiment: h(4, 1),
                target: h(900, 65_535),
            },
            Command::RemoveRegiment { regiment: h(2, 2) },
            Command::TuneUnit { unit: 1, stats },
            Command::TuneUnit {
                unit: 2,
                stats: UnitStats {
                    mass: Mass::Heavy,
                    reach: true,
                    shield: true,
                    ..stats
                },
            },
            Command::SetFormation {
                regiment: h(7, 3),
                formation: Formation::ShieldWall,
            },
            Command::DeployZone {
                center: FxVec2::new(Fx::from_raw(-5), Fx::from_int(200)),
                radius: Fx::from_int(30),
            },
            Command::Ready,
            Command::SpawnStructure {
                building: 40,
                tile: (-1, 4000),
            },
            Command::AttackStructure {
                regiment: h(3, 1),
                structure: StructureHandle {
                    index: 1023,
                    generation: 65_535,
                },
            },
            Command::RemoveStructure {
                structure: StructureHandle {
                    index: 0,
                    generation: 0,
                },
            },
            Command::Gather {
                regiment: h(2, 9),
                node: 300,
            },
            Command::Build {
                regiment: h(4, 2),
                building: 300,
                tile: (-3, 4000),
            },
            Command::Construct {
                regiment: h(1, 1),
                structure: StructureHandle {
                    index: 7,
                    generation: 2,
                },
            },
            Command::CallOff {
                structure: StructureHandle {
                    index: 1023,
                    generation: 1,
                },
            },
            Command::Train {
                structure: StructureHandle {
                    index: 4,
                    generation: 9,
                },
                unit: 200,
            },
            Command::Untrain {
                structure: StructureHandle {
                    index: 5,
                    generation: 0,
                },
            },
            Command::Upgrade {
                structure: StructureHandle {
                    index: 6,
                    generation: 65_535,
                },
            },
            Command::Rally {
                structure: StructureHandle {
                    index: 0,
                    generation: 3,
                },
                at: FxVec2::new(Fx::from_raw(-9), Fx::from_int(300)),
            },
            Command::FallBack { regiment: h(5, 9) },
            Command::Brace {
                regiment: h(6, 0),
                on: true,
            },
            Command::Brace {
                regiment: h(6, 1),
                on: false,
            },
        ];
        let pcs: Vec<PlayerCommand> = cmds
            .iter()
            .enumerate()
            .map(|(i, &command)| PlayerCommand {
                player: i as u8 % 4,
                command,
            })
            .collect();
        let mut w = Writer::new();
        write_player_commands(&mut w, &pcs);
        let mut r = Reader::new(&w.buf);
        assert_eq!(read_player_commands(&mut r).as_deref(), Some(&pcs[..]));
        assert!(r.is_empty());
        // A move order for one regiment stays small.
        assert!(command_size(&pcs[1]) <= 16, "{}", command_size(&pcs[1]));
    }

    #[test]
    fn garbage_never_panics() {
        let mut seed = 0x1234_5678u32;
        for len in 0..200 {
            let bytes: Vec<u8> = (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect();
            let _ = read_player_commands(&mut Reader::new(&bytes));
        }
    }
}
