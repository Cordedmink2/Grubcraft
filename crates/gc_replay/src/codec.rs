//! Byte encoding of a replay's contents: varints, and the simulation's
//! commands, unit stats and Conquest tables.
//!
//! This is deliberately the replay file's own encoding, not the network's
//! (`gc_net::wire`): the wire format changes whenever the protocol does,
//! while a stored replay must keep reading. Changing anything here means
//! bumping [`crate::FORMAT`].

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::structures::{Armour, Breaker, Role, TowerAttack, VEINS};
use gc_sim::{
    BuildingStats, Command, ConquestUnit, Formation, Mass, Meter, NodeSetup, PlayerCommand,
    PowerEffect, PowerStats, PowerTarget, RegimentHandle, Resource, StructureHandle, TradePost,
    UnitStats,
};

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

    pub fn building(&mut self, b: &BuildingStats) {
        self.u8(b.role as u8);
        self.u8(b.tier);
        self.var(b.upgrades_from.map_or(0, |f| u64::from(f) + 1));
        self.var(u64::from(b.cost_grub));
        self.var(u64::from(b.cost_marrow));
        self.var(u64::from(b.build_ticks));
        self.var(u64::from(b.hp));
        self.u8(b.armour as u8);
        self.u8(b.width);
        self.u8(b.depth);
        self.u8(b.supply);
        self.var(u64::from(b.flags));
        match b.attack {
            Some(a) => {
                self.bool(true);
                self.fx(a.range);
                self.var(u64::from(a.damage));
                self.var(u64::from(a.reload_ticks));
                self.u8(a.shots);
            }
            None => self.bool(false),
        }
        self.u8(b.field);
        match b.tunes {
            Some((r, tiles)) => {
                self.u8(1 + r as u8);
                self.u8(tiles);
            }
            None => self.u8(0),
        }
        self.u8(b.links);
    }

    pub fn power(&mut self, p: &PowerStats) {
        self.u8(p.tier);
        self.u8(p.cost);
        self.meter(p.meter);
        self.var(u64::from(p.cooldown_ticks));
        self.var(u64::from(p.duration_ticks));
        match p.effect {
            PowerEffect::NotBuilt => self.u8(0),
            PowerEffect::Haste { percent } => {
                self.u8(1);
                self.u8(percent);
            }
            PowerEffect::Shift { to } => {
                self.u8(2);
                self.var(u64::from(to));
            }
            PowerEffect::Grant { grub } => {
                self.u8(3);
                self.var(u64::from(grub));
            }
            PowerEffect::March { percent } => {
                self.u8(4);
                self.u8(percent);
            }
            PowerEffect::Steadfast { tiles } => {
                self.u8(5);
                self.var(u64::from(tiles));
            }
            PowerEffect::Summon { unit } => {
                self.u8(6);
                self.var(u64::from(unit));
            }
        }
    }

    /// A faction meter (format 22 on).
    pub fn meter(&mut self, m: Meter) {
        self.u8(m as u8);
    }

    pub fn conquest_unit(&mut self, u: &ConquestUnit) {
        self.u8(u.breaker as u8);
        self.bool(u.worker);
        self.u8(u.supply);
        self.var(u64::from(u.cost_grub));
        self.var(u64::from(u.cost_marrow));
        self.var(u64::from(u.train_ticks));
        self.var(u.trained_at.map_or(0, |b| u64::from(b) + 1));
        self.u8(u.tier);
        self.bool(u.hatches);
        self.bool(u.consumed);
        self.bool(u.burrows);
        self.bool(u.builds_only);
        self.bool(u.mans_walls);
        self.var(u64::from(u.trade_base));
        self.var(u64::from(u.trade_tenths));
        self.var(u64::from(u.cannot_bank));
        let c = u.clearing;
        for i in 0..3 {
            self.var(u64::from(c.pace[i]));
            self.u8(c.pays[i]);
        }
        self.bool(c.burns);
    }

    /// A caravan's trade post (format 24 on).
    pub fn trade_post(&mut self, post: TradePost) {
        match post {
            TradePost::Hamlet(i) => {
                self.u8(0);
                self.var(u64::from(i));
            }
            TradePost::Structure(h) => {
                self.u8(1);
                self.structure(h);
            }
        }
    }

    pub fn node(&mut self, n: &NodeSetup) {
        self.u8(n.resource as u8);
        self.vec(n.at);
        self.var(u64::from(n.amount));
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
            Command::SpawnStructure {
                building,
                tile,
                turn,
            } => {
                // A turned one has a tag of its own (format 26), so files
                // from before turning read as they were.
                self.u8(if turn == 0 { 8 } else { 25 });
                self.var(u64::from(building));
                // Two's complement, as the network sends it.
                self.var(u64::from(tile.0 as u16));
                self.var(u64::from(tile.1 as u16));
                if turn != 0 {
                    self.u8(turn);
                }
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
            Command::Gather { regiment, node } => {
                self.u8(11);
                self.handle(regiment);
                self.var(u64::from(node));
            }
            Command::Trade { regiment, post } => {
                self.u8(23);
                self.handle(regiment);
                self.trade_post(post);
            }
            Command::Build {
                regiment,
                building,
                tile,
                turn,
            } => {
                self.u8(if turn == 0 { 12 } else { 26 });
                self.handle(regiment);
                self.var(u64::from(building));
                self.var(u64::from(tile.0 as u16));
                self.var(u64::from(tile.1 as u16));
                if turn != 0 {
                    self.u8(turn);
                }
            }
            Command::Clear {
                regiment,
                tile,
                size,
            } => {
                self.u8(27);
                self.handle(regiment);
                self.var(u64::from(tile.0 as u16));
                self.var(u64::from(tile.1 as u16));
                self.u8(size.0);
                self.u8(size.1);
            }
            Command::Construct {
                regiment,
                structure,
            } => {
                self.u8(13);
                self.handle(regiment);
                self.structure(structure);
            }
            Command::CallOff { structure } => {
                self.u8(14);
                self.structure(structure);
            }
            Command::Train { structure, unit } => {
                self.u8(15);
                self.structure(structure);
                self.var(u64::from(unit));
            }
            Command::Untrain { structure } => {
                self.u8(16);
                self.structure(structure);
            }
            Command::Upgrade { structure } => {
                self.u8(17);
                self.structure(structure);
            }
            Command::Rally { structure, at } => {
                self.u8(18);
                self.structure(structure);
                self.vec(at);
            }
            Command::FallBack { regiment } => {
                self.u8(19);
                self.handle(regiment);
            }
            Command::Brace { regiment, on } => {
                self.u8(20);
                self.handle(regiment);
                self.bool(on);
            }
            Command::Torches { regiment, lit } => {
                self.u8(36);
                self.handle(regiment);
                self.bool(lit);
            }
            Command::HandOver { to } => {
                self.u8(21);
                self.u8(to);
            }
            Command::BuildWall {
                regiment,
                building,
                from,
                to,
            } => {
                // A grown wall, with no worker, has a tag of its own.
                match regiment {
                    Some(r) => {
                        self.u8(29);
                        self.handle(r);
                    }
                    None => self.u8(30),
                }
                self.var(u64::from(building));
                for v in [from.0, from.1, to.0, to.1] {
                    self.var(u64::from(v as u16));
                }
            }
            Command::RaiseWall {
                structure,
                regiment,
            } => {
                // A grown wall, with no worker, has a tag of its own.
                match regiment {
                    Some(r) => {
                        self.u8(34);
                        self.structure(structure);
                        self.handle(r);
                    }
                    None => {
                        self.u8(35);
                        self.structure(structure);
                    }
                }
            }
            Command::PlanWall { building, from, to } => {
                self.u8(31);
                self.var(u64::from(building));
                for v in [from.0, from.1, to.0, to.1] {
                    self.var(u64::from(v as u16));
                }
            }
            Command::DropPlan { from, to } => {
                self.u8(32);
                for v in [from.0, from.1, to.0, to.1] {
                    self.var(u64::from(v as u16));
                }
            }
            Command::WorkPlan { regiment } => {
                self.u8(33);
                self.handle(regiment);
            }
            Command::ManWall {
                regiment,
                structure,
            } => {
                self.u8(37);
                self.handle(regiment);
                self.structure(structure);
            }
            Command::Climb {
                regiment,
                structure,
            } => {
                self.u8(38);
                self.handle(regiment);
                self.structure(structure);
            }
            Command::SetGate { structure, mode } => {
                self.u8(28);
                self.structure(structure);
                self.u8(mode as u8);
            }
            Command::FocusTower { structure, target } => {
                self.u8(24);
                self.structure(structure);
                match target {
                    None => self.u8(0),
                    Some(h) => {
                        self.u8(1);
                        self.handle(h);
                    }
                }
            }
            Command::UsePower { power, target } => {
                self.u8(22);
                self.u8(power);
                match target {
                    PowerTarget::Structure(h) => {
                        self.u8(0);
                        self.structure(h);
                    }
                    PowerTarget::Regiment(h) => {
                        self.u8(1);
                        self.handle(h);
                    }
                    PowerTarget::Shift { regiment, to } => {
                        self.u8(2);
                        self.handle(regiment);
                        self.structure(to);
                    }
                    PowerTarget::Caster => self.u8(3),
                    PowerTarget::Point(at) => {
                        self.u8(4);
                        self.vec(at);
                    }
                }
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
            discipline: false,
        }
        .with_weapons(self.u8()?)
    }

    /// 0 for none, else the id plus one.
    fn maybe_id(&mut self) -> Option<Option<u16>> {
        let v: u16 = self.var_as()?;
        Some(v.checked_sub(1))
    }

    /// A building as file format `format` wrote it.
    pub fn building(&mut self, format: u16) -> Option<BuildingStats> {
        let mut b = BuildingStats {
            role: Role::from_u8(self.u8()?)?,
            tier: self.u8()?,
            upgrades_from: self.maybe_id()?,
            cost_grub: self.var_as()?,
            cost_marrow: self.var_as()?,
            build_ticks: self.var_as()?,
            hp: self.var_as()?,
            armour: Armour::from_u8(self.u8()?)?,
            width: self.u8()?,
            depth: self.u8()?,
            supply: self.u8()?,
            flags: if format >= 24 {
                self.var_as()?
            } else {
                u16::from(self.u8()?)
            },
            attack: if self.bool()? {
                Some(TowerAttack {
                    range: self.fx()?,
                    damage: self.var_as()?,
                    reload_ticks: self.var_as()?,
                    shots: self.u8()?,
                })
            } else {
                None
            },
            field: 0,
            tunes: None,
            links: 0,
        };
        if format >= 19 {
            b.field = self.u8()?;
            b.tunes = match self.u8()? {
                0 => None,
                r => Some((Resource::from_u8(r - 1)?, self.u8()?)),
            };
            if format >= 20 {
                b.links = self.u8()?;
            }
        } else if b.flags & VEINS != 0 {
            // Before fields were data, root-veins reached 15 tiles from a
            // main building and 8 from any other.
            b.field = if b.role == Role::Main { 15 } else { 8 };
        }
        Some(b)
    }

    /// A faction power as file format `format` (21 on) wrote it: before 22,
    /// every power was paid from Resonance, and summons came with 23.
    pub fn power(&mut self, format: u16) -> Option<PowerStats> {
        Some(PowerStats {
            tier: self.u8()?,
            cost: self.u8()?,
            meter: if format >= 22 {
                self.meter()?
            } else {
                Meter::Resonance
            },
            cooldown_ticks: self.var_as()?,
            duration_ticks: self.var_as()?,
            effect: match self.u8()? {
                0 => PowerEffect::NotBuilt,
                1 => PowerEffect::Haste {
                    percent: self.u8()?,
                },
                2 => PowerEffect::Shift { to: self.var_as()? },
                3 => PowerEffect::Grant {
                    grub: self.var_as()?,
                },
                4 => PowerEffect::March {
                    percent: self.u8()?,
                },
                5 => PowerEffect::Steadfast {
                    tiles: self.var_as()?,
                },
                6 if format >= 23 => PowerEffect::Summon {
                    unit: self.var_as()?,
                },
                _ => return None,
            },
        })
    }

    /// A faction meter (format 22 on).
    pub fn meter(&mut self) -> Option<Meter> {
        Meter::from_u8(self.u8()?)
    }

    /// A Conquest unit as file format `format` wrote it.
    pub fn conquest_unit(&mut self, format: u16) -> Option<ConquestUnit> {
        let mut u = ConquestUnit {
            breaker: Breaker::from_u8(self.u8()?)?,
            worker: self.bool()?,
            supply: self.u8()?,
            cost_grub: self.var_as()?,
            cost_marrow: self.var_as()?,
            train_ticks: self.var_as()?,
            trained_at: self.maybe_id()?,
            tier: self.u8()?,
            ..ConquestUnit::default()
        };
        if format >= 17 {
            u.hatches = self.bool()?;
            u.consumed = self.bool()?;
        }
        if format >= 18 {
            u.burrows = self.bool()?;
        }
        if format >= 19 {
            u.builds_only = self.bool()?;
        }
        if format >= 39 {
            u.mans_walls = self.bool()?;
        }
        if format >= 24 {
            u.trade_base = self.var_as()?;
            u.trade_tenths = self.var_as()?;
            u.cannot_bank = self.var_as()?;
        }
        if format >= 30 {
            for i in 0..3 {
                u.clearing.pace[i] = self.var_as()?;
                u.clearing.pays[i] = self.u8()?;
            }
            u.clearing.burns = self.bool()?;
        }
        Some(u)
    }

    /// A caravan's trade post (format 24 on).
    pub fn trade_post(&mut self) -> Option<TradePost> {
        Some(match self.u8()? {
            0 => TradePost::Hamlet(self.var_as()?),
            1 => TradePost::Structure(self.structure()?),
            _ => return None,
        })
    }

    pub fn node(&mut self) -> Option<NodeSetup> {
        Some(NodeSetup {
            resource: Resource::from_u8(self.u8()?)?,
            at: self.vec()?,
            amount: self.var_as()?,
        })
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
            tag @ (8 | 25) => Command::SpawnStructure {
                building: self.var_as()?,
                tile: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
                turn: if tag == 25 { self.u8()? } else { 0 },
            },
            9 => Command::AttackStructure {
                regiment: self.handle()?,
                structure: self.structure()?,
            },
            10 => Command::RemoveStructure {
                structure: self.structure()?,
            },
            11 => Command::Gather {
                regiment: self.handle()?,
                node: self.var_as()?,
            },
            tag @ (12 | 26) => Command::Build {
                regiment: self.handle()?,
                building: self.var_as()?,
                tile: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
                turn: if tag == 26 { self.u8()? } else { 0 },
            },
            27 => Command::Clear {
                regiment: self.handle()?,
                tile: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
                size: (self.u8()?, self.u8()?),
            },
            23 => Command::Trade {
                regiment: self.handle()?,
                post: self.trade_post()?,
            },
            13 => Command::Construct {
                regiment: self.handle()?,
                structure: self.structure()?,
            },
            14 => Command::CallOff {
                structure: self.structure()?,
            },
            15 => Command::Train {
                structure: self.structure()?,
                unit: self.var_as()?,
            },
            16 => Command::Untrain {
                structure: self.structure()?,
            },
            17 => Command::Upgrade {
                structure: self.structure()?,
            },
            18 => Command::Rally {
                structure: self.structure()?,
                at: self.vec()?,
            },
            19 => Command::FallBack {
                regiment: self.handle()?,
            },
            20 => Command::Brace {
                regiment: self.handle()?,
                on: self.bool()?,
            },
            21 => Command::HandOver { to: self.u8()? },
            36 => Command::Torches {
                regiment: self.handle()?,
                lit: self.bool()?,
            },
            tag @ (29 | 30) => Command::BuildWall {
                regiment: if tag == 29 {
                    Some(self.handle()?)
                } else {
                    None
                },
                building: self.var_as()?,
                from: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
                to: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
            },
            31 => Command::PlanWall {
                building: self.var_as()?,
                from: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
                to: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
            },
            32 => Command::DropPlan {
                from: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
                to: (self.var_as::<u16>()? as i16, self.var_as::<u16>()? as i16),
            },
            33 => Command::WorkPlan {
                regiment: self.handle()?,
            },
            37 => Command::ManWall {
                regiment: self.handle()?,
                structure: self.structure()?,
            },
            38 => Command::Climb {
                regiment: self.handle()?,
                structure: self.structure()?,
            },
            tag @ (34 | 35) => Command::RaiseWall {
                structure: self.structure()?,
                regiment: if tag == 34 {
                    Some(self.handle()?)
                } else {
                    None
                },
            },
            28 => Command::SetGate {
                structure: self.structure()?,
                mode: gc_sim::gates::GateMode::from_u8(self.u8()?)?,
            },
            24 => Command::FocusTower {
                structure: self.structure()?,
                target: match self.u8()? {
                    0 => None,
                    1 => Some(self.handle()?),
                    _ => return None,
                },
            },
            22 => Command::UsePower {
                power: self.u8()?,
                target: match self.u8()? {
                    0 => PowerTarget::Structure(self.structure()?),
                    1 => PowerTarget::Regiment(self.handle()?),
                    2 => PowerTarget::Shift {
                        regiment: self.handle()?,
                        to: self.structure()?,
                    },
                    3 => PowerTarget::Caster,
                    4 => PowerTarget::Point(self.vec()?),
                    _ => return None,
                },
            },
            _ => return None,
        };
        Some(PlayerCommand { player, command })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A building's flags were a byte before format 24 and are a
    /// variable-length number after it (the Waystation's trade post flag is
    /// bit 256); a number under 128 takes one byte either way, so a file of
    /// format 23 reads as it did.
    #[test]
    fn a_buildings_flags_read_as_older_formats_wrote_them() {
        let building = |flags| BuildingStats {
            role: Role::Outpost,
            tier: 1,
            upgrades_from: None,
            cost_grub: 200,
            cost_marrow: 0,
            build_ticks: 1200,
            hp: 1200,
            armour: Armour::Structure,
            width: 4,
            depth: 4,
            supply: 0,
            flags,
            attack: None,
            field: 0,
            tunes: None,
            links: 0,
        };
        let drop_off_marrow = 2;
        let trade_post = gc_sim::structures::TRADE_POST;
        for (flags, formats) in [
            (drop_off_marrow, 23..=24),
            (drop_off_marrow | trade_post, 24..=24),
        ] {
            let mut w = Writer::default();
            w.building(&building(flags));
            for format in formats {
                let mut r = Reader::new(&w.buf);
                let read = r.building(format).expect("it reads");
                assert_eq!(read.flags, flags, "format {format}");
                assert_eq!(r.remaining(), 0, "format {format}");
            }
        }
        // A flag past a byte takes two.
        let mut one = Writer::default();
        one.building(&building(drop_off_marrow));
        let mut two = Writer::default();
        two.building(&building(trade_post));
        assert_eq!(two.buf.len(), one.buf.len() + 1);
    }

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
                turn: 0,
            },
            Command::SpawnStructure {
                building: 4,
                tile: (12, 7),
                turn: 3,
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
            Command::Gather {
                regiment: RegimentHandle {
                    index: 7,
                    generation: 3,
                },
                node: 65_535,
            },
            Command::Trade {
                regiment: RegimentHandle {
                    index: 7,
                    generation: 3,
                },
                post: TradePost::Hamlet(300),
            },
            Command::Trade {
                regiment: RegimentHandle {
                    index: 8,
                    generation: 0,
                },
                post: TradePost::Structure(StructureHandle {
                    index: 12,
                    generation: 5,
                }),
            },
            Command::Build {
                regiment: RegimentHandle {
                    index: 8,
                    generation: 1,
                },
                building: 2,
                tile: (40, -5),
                turn: 0,
            },
            Command::Build {
                regiment: RegimentHandle {
                    index: 8,
                    generation: 1,
                },
                building: 2,
                tile: (40, -5),
                turn: 1,
            },
            Command::Clear {
                regiment: RegimentHandle {
                    index: 8,
                    generation: 1,
                },
                tile: (12, -3),
                size: (16, 1),
            },
            Command::Construct {
                regiment: RegimentHandle {
                    index: 9,
                    generation: 0,
                },
                structure: StructureHandle {
                    index: 3,
                    generation: 4,
                },
            },
            Command::CallOff {
                structure: StructureHandle {
                    index: 900,
                    generation: 7,
                },
            },
            Command::SetGate {
                structure: StructureHandle {
                    index: 4,
                    generation: 2,
                },
                mode: gc_sim::gates::GateMode::Open,
            },
            Command::BuildWall {
                regiment: Some(RegimentHandle {
                    index: 8,
                    generation: 1,
                }),
                building: 2,
                from: (40, -5),
                to: (12, 3),
            },
            Command::BuildWall {
                regiment: None,
                building: 5,
                from: (3, 9),
                to: (9, 3),
            },
            Command::RaiseWall {
                structure: StructureHandle {
                    index: 6,
                    generation: 1,
                },
                regiment: Some(RegimentHandle {
                    index: 2,
                    generation: 3,
                }),
            },
            Command::RaiseWall {
                structure: StructureHandle {
                    index: 7,
                    generation: 0,
                },
                regiment: None,
            },
            Command::Torches {
                regiment: RegimentHandle {
                    index: 4,
                    generation: 2,
                },
                lit: true,
            },
            Command::PlanWall {
                building: 2,
                from: (-4, 7),
                to: (20, 7),
            },
            Command::DropPlan {
                from: (1, 2),
                to: (3, -4),
            },
            Command::WorkPlan {
                regiment: RegimentHandle {
                    index: 9,
                    generation: 3,
                },
            },
            Command::Climb {
                regiment: RegimentHandle {
                    index: 5,
                    generation: 1,
                },
                structure: StructureHandle {
                    index: 13,
                    generation: 4,
                },
            },
            Command::ManWall {
                regiment: RegimentHandle {
                    index: 4,
                    generation: 2,
                },
                structure: StructureHandle {
                    index: 12,
                    generation: 5,
                },
            },
            Command::FocusTower {
                structure: StructureHandle {
                    index: 4,
                    generation: 2,
                },
                target: Some(RegimentHandle {
                    index: 11,
                    generation: 6,
                }),
            },
            Command::FocusTower {
                structure: StructureHandle {
                    index: 4,
                    generation: 2,
                },
                target: None,
            },
            Command::Train {
                structure: StructureHandle {
                    index: 2,
                    generation: 1,
                },
                unit: 13,
            },
            Command::Untrain {
                structure: StructureHandle {
                    index: 2,
                    generation: 1,
                },
            },
            Command::Upgrade {
                structure: StructureHandle {
                    index: 0,
                    generation: 65535,
                },
            },
            Command::Rally {
                structure: StructureHandle {
                    index: 1,
                    generation: 2,
                },
                at: FxVec2::from_ints(-8, 120),
            },
            Command::HandOver { to: 3 },
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
