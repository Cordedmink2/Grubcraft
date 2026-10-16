//! Byte encoding of a replay's contents: varints, and the simulation's
//! commands and unit stats.
//!
//! This is deliberately the replay file's own encoding, not the network's
//! (`gc_net::wire`): the wire format changes whenever the protocol does,
//! while a stored replay must keep reading. Changing anything here means
//! bumping [`crate::FORMAT`].

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::{Command, Formation, Mass, PlayerCommand, RegimentHandle, StructureHandle, UnitStats};

#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
}

impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }

    /// Unsigned LEB128.
    pub fn var(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.buf.push(v as u8 | 0x80);
            v >>= 7;
        }
        self.buf.push(v as u8);
    }

    /// Zigzag, then LEB128, so small negative numbers stay small.
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

    pub fn stats(&mut self, s: &UnitStats) {
        self.fx(s.speed);
        self.var(u64::from(s.turn_rate));
        self.var(u64::from(s.models));
        self.var(u64::from(s.hp_per_model));
        self.var(u64::from(s.files));
        self.fx(s.file_spacing);
        self.fx(s.rank_spacing);
        self.u8(s.melee_attack);
        self.u8(s.melee_defence);
        self.var(u64::from(s.melee_damage));
        self.u8(s.charge_bonus);
        self.fx(s.range);
        self.var(u64::from(s.missile_damage));
        self.var(u64::from(s.reload_ticks));
        self.u8(s.accuracy);
        self.u8(s.morale);
        self.u8(s.mass as u8);
        self.u8(s.weapons());
    }

    pub fn command(&mut self, pc: &PlayerCommand) {
        self.u8(pc.player);
        match pc.command {
            Command::SpawnRegiment {
                unit,
                position,
                facing,
            } => {
                self.u8(0);
                self.var(u64::from(unit));
                self.vec(position);
                self.var(u64::from(facing.0));
            }
            Command::MoveTo {
                regiment,
                target,
                facing,
                files,
                pace,
            } => {
                self.u8(1);
                self.handle(regiment);
                self.vec(target);
                self.var(facing.map_or(0, |a| u64::from(a.0) + 1));
                self.var(u64::from(files));
                self.fx(pace);
            }
            Command::Attack { regiment, target } => {
                self.u8(2);
                self.handle(regiment);
                self.handle(target);
            }
            Command::RemoveRegiment { regiment } => {
                self.u8(3);
                self.handle(regiment);
            }
            Command::TuneUnit { unit, stats } => {
                self.u8(4);
                self.var(u64::from(unit));
                self.stats(&stats);
            }
            Command::SetFormation {
                regiment,
                formation,
            } => {
                self.u8(5);
                self.handle(regiment);
                self.u8(formation as u8);
            }
            Command::DeployZone { center, radius } => {
                self.u8(6);
                self.vec(center);
                self.fx(radius);
            }
            Command::Ready => self.u8(7),
            Command::SpawnStructure { building, tile } => {
                self.u8(8);
                self.var(u64::from(building));
                // Two's complement, as the network sends it.
                self.var(u64::from(tile.0 as u16));
                self.var(u64::from(tile.1 as u16));
            }
            Command::AttackStructure {
                regiment,
                structure,
            } => {
                self.u8(9);
                self.handle(regiment);
                self.structure(structure);
            }
            Command::RemoveStructure { structure } => {
                self.u8(10);
                self.structure(structure);
            }
        }
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Reader<'a> {
        Reader { buf }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len()
    }

    pub fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.buf.len() < n {
            return None;
        }
        let (head, rest) = self.buf.split_at(n);
        self.buf = rest;
        Some(head)
    }

    pub fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
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

    pub fn var_as<T: TryFrom<u64>>(&mut self) -> Option<T> {
        T::try_from(self.var()?).ok()
    }

    pub fn ivar(&mut self) -> Option<i64> {
        let v = self.var()?;
        Some((v >> 1) as i64 ^ -((v & 1) as i64))
    }

    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.var_as()?;
        self.take(n)
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

    pub fn stats(&mut self) -> Option<UnitStats> {
        UnitStats {
            speed: self.fx()?,
            turn_rate: self.var_as()?,
            models: self.var_as()?,
            hp_per_model: self.var_as()?,
            files: self.var_as()?,
            file_spacing: self.fx()?,
            rank_spacing: self.fx()?,
            melee_attack: self.u8()?,
            melee_defence: self.u8()?,
            melee_damage: self.var_as()?,
            charge_bonus: self.u8()?,
            range: self.fx()?,
            missile_damage: self.var_as()?,
            reload_ticks: self.var_as()?,
            accuracy: self.u8()?,
            morale: self.u8()?,
            mass: Mass::from_u8(self.u8()?)?,
            reach: false,
            swing_room: false,
            shield: false,
        }
        .with_weapons(self.u8()?)
    }

    pub fn command(&mut self) -> Option<PlayerCommand> {
        let player = self.u8()?;
        let command = match self.u8()? {
            0 => Command::SpawnRegiment {
                unit: self.var_as()?,
                position: self.vec()?,
                facing: Angle(self.var_as()?),
            },
            1 => Command::MoveTo {
                regiment: self.handle()?,
                target: self.vec()?,
                facing: match self.var()? {
                    0 => None,
                    a => Some(Angle(u16::try_from(a - 1).ok()?)),
                },
                files: self.var_as()?,
                pace: self.fx()?,
            },
            2 => Command::Attack {
                regiment: self.handle()?,
                target: self.handle()?,
            },
            3 => Command::RemoveRegiment {
                regiment: self.handle()?,
            },
            4 => Command::TuneUnit {
                unit: self.var_as()?,
                stats: self.stats()?,
            },
            5 => Command::SetFormation {
                regiment: self.handle()?,
                formation: Formation::from_u8(self.u8()?)?,
            },
            6 => Command::DeployZone {
                center: self.vec()?,
                radius: self.fx()?,
            },
            7 => Command::Ready,
            8 => Command::SpawnStructure {
                building: self.var_as()?,
                tile: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
            },
            9 => Command::AttackStructure {
                regiment: self.handle()?,
                structure: self.structure()?,
            },
            10 => Command::RemoveStructure {
                structure: self.structure()?,
            },
            _ => return None,
        };
        Some(PlayerCommand { player, command })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_and_building_commands_round_trip() {
        let commands = [
            Command::DeployZone {
                center: FxVec2::from_ints(40, -7),
                radius: Fx::from_int(30),
            },
            Command::Ready,
            Command::SpawnStructure {
                building: 3,
                tile: (-2, 140),
            },
            Command::AttackStructure {
                regiment: RegimentHandle {
                    index: 5,
                    generation: 2,
                },
                structure: StructureHandle {
                    index: 900,
                    generation: 7,
                },
            },
            Command::RemoveStructure {
                structure: StructureHandle {
                    index: 1,
                    generation: 0,
                },
            },
        ];
        let mut w = Writer::default();
        for command in commands {
            w.command(&PlayerCommand { player: 2, command });
        }
        let mut r = Reader::new(&w.buf);
        for command in commands {
            assert_eq!(r.command(), Some(PlayerCommand { player: 2, command }));
        }
        assert_eq!(r.remaining(), 0);
    }
}
