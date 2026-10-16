//! What travels between machines: the packet framing and every message of
//! the lobby and the match (docs/11-networking.md, "Session flow").
//!
//! A packet is a 4-byte header (`GC`, protocol, kind) and then either a
//! discovery query, a discovery beacon, or a link packet carrying reliable
//! messages (see [`crate::link`]).

use gc_sim::PlayerCommand;
use gc_ui::lobby::{
    Budget, Faction, GameMode, LobbySettings, MAPS, MAX_ARMY, MAX_PLAYERS, OnDrop, Slot, SlotEdit,
    SlotKind, Tempo,
};

use crate::wire::{Reader, Writer, read_player_commands, write_player_commands};

/// Bumped whenever any message changes shape. Machines on different
/// protocols cannot talk; the joiner is told which build the host runs.
pub const PROTOCOL: u8 = 7;

const MAGIC: [u8; 2] = *b"GC";

/// Packets stay under this size, which crosses any network unfragmented.
pub const MAX_PACKET: usize = 1200;

/// The largest single message; a link packet's header fits beside it.
pub const MAX_MESSAGE: usize = 1100;

/// Names longer than this are cut, so lobby messages stay small.
pub const MAX_NAME: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// "Any games here?", broadcast by the Join screen.
    Query = 0,
    /// A host's answer to a query.
    Beacon = 1,
    /// Reliable messages between a host and one of its players.
    Link = 2,
    /// A host that cannot speak the joiner's protocol says so, in a form
    /// every protocol version reads: the host's build as a string.
    WrongProtocol = 3,
    /// A joiner's public key, to begin securing the link
    /// ([`crate::crypt`]).
    Hi = 4,
    /// The host's public key in answer.
    KeyReply = 5,
    /// The host could not open a joiner's packets: the password differs.
    /// Carries whether the lobby has a password.
    WrongPassword = 6,
}

/// Bytes in a packet header: `GC`, the protocol and the kind.
pub const HEADER_LEN: usize = 4;

pub fn header(w: &mut Writer, kind: Kind) {
    w.buf.extend_from_slice(&MAGIC);
    w.u8(PROTOCOL);
    w.u8(kind as u8);
}

/// Checks the magic and returns the packet's protocol and kind.
pub fn read_header(r: &mut Reader) -> Option<(u8, u8)> {
    if r.take(2)? != MAGIC {
        return None;
    }
    Some((r.u8()?, r.u8()?))
}

/// What a host says about its lobby to the Join screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Beacon {
    pub host: String,
    pub map: String,
    pub players: u8,
    pub max_players: u8,
    pub version: String,
    /// Whether the match has started (then it can't be joined).
    pub playing: bool,
    /// Whether joining needs a password.
    pub locked: bool,
}

impl Beacon {
    pub fn write(&self, w: &mut Writer) {
        w.str(&self.host);
        w.str(&self.map);
        w.u8(self.players);
        w.u8(self.max_players);
        w.str(&self.version);
        w.bool(self.playing);
        w.bool(self.locked);
    }

    pub fn read(r: &mut Reader) -> Option<Beacon> {
        Some(Beacon {
            host: r.str()?,
            map: r.str()?,
            players: r.u8()?,
            max_players: r.u8()?,
            version: r.str()?,
            playing: r.bool()?,
            locked: r.bool()?,
        })
    }
}

/// The lobby as the host sends it: everything but who is reading it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LobbyWire {
    pub slots: [Slot; MAX_PLAYERS],
    pub settings: LobbySettings,
    pub host_version: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// Joiner → host: who I am.
    Hello {
        version: String,
        data_hash: u64,
        name: String,
        faction: Faction,
        color: u8,
    },
    /// Host → joiner: you're in, in this slot.
    Welcome { slot: u8 },
    /// Host → joiner: you can't join (full, playing, different data).
    Refuse { reason: String },
    /// Host → everyone: the lobby changed.
    Lobby(Box<LobbyWire>),
    /// Player → host: change a slot (checked by the host).
    Edit { slot: u8, edit: SlotEdit },
    /// Host → everyone: the match begins. `players` are the occupied slots
    /// in order; `you` is the reader's position among them. Every machine
    /// builds the same world from `seed`; `map_hash` lets each check its
    /// map came out identical to the host's.
    Start {
        seed: u64,
        map_hash: u64,
        settings: LobbySettings,
        players: Vec<Slot>,
        you: u8,
        input_delay: u8,
    },
    /// Player → host: my world is built (or why it could not be).
    Loaded { error: Option<String> },
    /// Host → everyone: the commands for one turn, in the order to apply them.
    Bundle {
        turn: u32,
        commands: Vec<PlayerCommand>,
    },
    /// Player → host: my orders, to be put in a coming turn.
    Orders { commands: Vec<PlayerCommand> },
    /// Player → host: my world's state hash after `tick` ticks.
    Hash { tick: u32, hash: u64 },
    /// Host → everyone: something to show the players.
    Notice { text: String },
    /// Either way: I'm leaving, and why.
    Bye { reason: String },
    /// Host → everyone: orders now land this many turns after they reach
    /// the host (it adapts to the slowest connection).
    Delay { turns: u8 },
    /// Host → everyone: the match is paused while `who` reconnects, for
    /// at most `seconds` more. An empty `who` means it carries on.
    Wait { who: String, seconds: u16 },
    /// Host → everyone: a player's world stopped matching the host's at
    /// `tick`; everyone saves a desync report.
    Desync { tick: u32 },
}

fn write_slot(w: &mut Writer, s: &Slot) {
    w.u8(match s.kind {
        SlotKind::Open => 0,
        SlotKind::Human => 1,
        SlotKind::Ai => 2,
    });
    w.str(&s.name);
    w.u8(s.faction.index() as u8);
    w.u8(s.color);
    w.u8(s.team);
    w.bool(s.ready);
    w.str(&s.version);
    w.u8(s.ai_level);
    write_army(w, &s.army);
}

fn write_army(w: &mut Writer, army: &[u16]) {
    w.var(army.len() as u64);
    for &u in army {
        w.var(u64::from(u));
    }
}

fn read_army(r: &mut Reader) -> Option<Vec<u16>> {
    let n: usize = r.var_as()?;
    if n > MAX_ARMY {
        return None;
    }
    (0..n).map(|_| r.var_as()).collect()
}

fn read_faction(r: &mut Reader) -> Option<Faction> {
    Faction::ALL.get(r.u8()? as usize).copied()
}

fn read_slot(r: &mut Reader) -> Option<Slot> {
    Some(Slot {
        kind: match r.u8()? {
            0 => SlotKind::Open,
            1 => SlotKind::Human,
            2 => SlotKind::Ai,
            _ => return None,
        },
        name: r.str()?,
        faction: read_faction(r)?,
        color: r.u8()?,
        team: r.u8().filter(|&t| usize::from(t) < MAX_PLAYERS)?,
        ready: r.bool()?,
        version: r.str()?,
        ai_level: r.u8()?,
        army: read_army(r)?,
    })
}

fn write_settings(w: &mut Writer, s: &LobbySettings) {
    w.var(s.map as u64);
    w.u8(GameMode::ALL.iter().position(|&m| m == s.mode).unwrap_or(0) as u8);
    w.u8(Tempo::ALL.iter().position(|&t| t == s.tempo).unwrap_or(0) as u8);
    w.u8(OnDrop::ALL
        .iter()
        .position(|&d| d == s.on_drop)
        .unwrap_or(0) as u8);
    w.u8(Budget::ALL.iter().position(|&b| b == s.budget).unwrap_or(0) as u8);
    w.bool(s.champions);
}

fn read_settings(r: &mut Reader) -> Option<LobbySettings> {
    Some(LobbySettings {
        map: r.var_as()?,
        mode: *GameMode::ALL.get(r.u8()? as usize)?,
        tempo: *Tempo::ALL.get(r.u8()? as usize)?,
        on_drop: *OnDrop::ALL.get(r.u8()? as usize)?,
        budget: *Budget::ALL.get(r.u8()? as usize)?,
        champions: r.bool()?,
    })
}

fn write_edit(w: &mut Writer, e: &SlotEdit) {
    match *e {
        SlotEdit::Faction(f) => {
            w.u8(0);
            w.u8(f.index() as u8);
        }
        SlotEdit::Color(c) => {
            w.u8(1);
            w.u8(c);
        }
        SlotEdit::Team(t) => {
            w.u8(2);
            w.u8(t);
        }
        SlotEdit::Ready(b) => {
            w.u8(3);
            w.bool(b);
        }
        SlotEdit::Kind(k) => {
            w.u8(4);
            w.u8(match k {
                SlotKind::Open => 0,
                SlotKind::Human => 1,
                SlotKind::Ai => 2,
            });
        }
        SlotEdit::AiLevel(l) => {
            w.u8(5);
            w.u8(l);
        }
        SlotEdit::Army(ref army) => {
            w.u8(6);
            write_army(w, army);
        }
    }
}

fn read_edit(r: &mut Reader) -> Option<SlotEdit> {
    Some(match r.u8()? {
        0 => SlotEdit::Faction(read_faction(r)?),
        1 => SlotEdit::Color(r.u8()?),
        2 => SlotEdit::Team(r.u8()?),
        3 => SlotEdit::Ready(r.bool()?),
        4 => SlotEdit::Kind(match r.u8()? {
            0 => SlotKind::Open,
            1 => SlotKind::Human,
            2 => SlotKind::Ai,
            _ => return None,
        }),
        5 => SlotEdit::AiLevel(r.u8()?),
        6 => SlotEdit::Army(read_army(r)?),
        _ => return None,
    })
}

impl Msg {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            Msg::Hello {
                version,
                data_hash,
                name,
                faction,
                color,
            } => {
                w.u8(0);
                w.str(version);
                w.u64(*data_hash);
                w.str(name);
                w.u8(faction.index() as u8);
                w.u8(*color);
            }
            Msg::Welcome { slot } => {
                w.u8(1);
                w.u8(*slot);
            }
            Msg::Refuse { reason } => {
                w.u8(2);
                w.str(reason);
            }
            Msg::Lobby(l) => {
                w.u8(3);
                for s in &l.slots {
                    write_slot(&mut w, s);
                }
                write_settings(&mut w, &l.settings);
                w.str(&l.host_version);
            }
            Msg::Edit { slot, edit } => {
                w.u8(4);
                w.u8(*slot);
                write_edit(&mut w, edit);
            }
            Msg::Start {
                seed,
                map_hash,
                settings,
                players,
                you,
                input_delay,
            } => {
                w.u8(5);
                w.u64(*seed);
                w.u64(*map_hash);
                write_settings(&mut w, settings);
                w.var(players.len() as u64);
                for s in players {
                    write_slot(&mut w, s);
                }
                w.u8(*you);
                w.u8(*input_delay);
            }
            Msg::Loaded { error } => {
                w.u8(6);
                match error {
                    None => w.bool(false),
                    Some(e) => {
                        w.bool(true);
                        w.str(e);
                    }
                }
            }
            Msg::Bundle { turn, commands } => {
                w.u8(7);
                w.var(u64::from(*turn));
                write_player_commands(&mut w, commands);
            }
            Msg::Orders { commands } => {
                w.u8(8);
                write_player_commands(&mut w, commands);
            }
            Msg::Hash { tick, hash } => {
                w.u8(9);
                w.var(u64::from(*tick));
                w.u64(*hash);
            }
            Msg::Notice { text } => {
                w.u8(10);
                w.str(text);
            }
            Msg::Bye { reason } => {
                w.u8(11);
                w.str(reason);
            }
            Msg::Delay { turns } => {
                w.u8(12);
                w.u8(*turns);
            }
            Msg::Wait { who, seconds } => {
                w.u8(13);
                w.str(who);
                w.var(u64::from(*seconds));
            }
            Msg::Desync { tick } => {
                w.u8(14);
                w.var(u64::from(*tick));
            }
        }
        w.buf
    }

    pub fn decode(bytes: &[u8]) -> Option<Msg> {
        let mut r = Reader::new(bytes);
        let msg = match r.u8()? {
            0 => Msg::Hello {
                version: r.str()?,
                data_hash: r.u64()?,
                name: r.str()?,
                faction: read_faction(&mut r)?,
                color: r.u8()?,
            },
            1 => Msg::Welcome {
                slot: r.u8().filter(|&s| usize::from(s) < MAX_PLAYERS)?,
            },
            2 => Msg::Refuse { reason: r.str()? },
            3 => Msg::Lobby(Box::new(LobbyWire {
                slots: [
                    read_slot(&mut r)?,
                    read_slot(&mut r)?,
                    read_slot(&mut r)?,
                    read_slot(&mut r)?,
                ],
                settings: read_settings(&mut r)?,
                host_version: r.str()?,
            })),
            4 => Msg::Edit {
                slot: r.u8()?,
                edit: read_edit(&mut r)?,
            },
            5 => {
                let seed = r.u64()?;
                let map_hash = r.u64()?;
                // A lobby may list a map another build lacks (the version
                // line stops it starting); a match must be on one we have.
                let settings = read_settings(&mut r).filter(|s| s.map < MAPS.len())?;
                let n = r.var_as::<usize>()?;
                if n > MAX_PLAYERS {
                    return None;
                }
                let mut players = Vec::with_capacity(n);
                for _ in 0..n {
                    players.push(read_slot(&mut r)?);
                }
                Msg::Start {
                    seed,
                    map_hash,
                    settings,
                    you: r.u8().filter(|&you| usize::from(you) < players.len())?,
                    players,
                    input_delay: r.u8()?,
                }
            }
            6 => Msg::Loaded {
                error: if r.bool()? { Some(r.str()?) } else { None },
            },
            7 => Msg::Bundle {
                turn: r.var_as()?,
                commands: read_player_commands(&mut r)?,
            },
            8 => Msg::Orders {
                commands: read_player_commands(&mut r)?,
            },
            9 => Msg::Hash {
                tick: r.var_as()?,
                hash: r.u64()?,
            },
            10 => Msg::Notice { text: r.str()? },
            11 => Msg::Bye { reason: r.str()? },
            12 => Msg::Delay { turns: r.u8()? },
            13 => Msg::Wait {
                who: r.str()?,
                seconds: r.var_as()?,
            },
            14 => Msg::Desync { tick: r.var_as()? },
            _ => return None,
        };
        r.is_empty().then_some(msg)
    }
}

/// `name` cut to [`MAX_NAME`] bytes on a character boundary.
pub fn short_name(name: &str) -> String {
    let mut end = name.len().min(MAX_NAME);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_fixed::{Angle, Fx, FxVec2};
    use gc_sim::{Command, RegimentHandle};

    fn round_trip(m: Msg) {
        let bytes = m.encode();
        assert!(bytes.len() <= MAX_MESSAGE);
        assert_eq!(Msg::decode(&bytes), Some(m.clone()), "{m:?}");
        // Cut short, it is rejected rather than misread.
        if !bytes.is_empty() {
            assert_eq!(Msg::decode(&bytes[..bytes.len() - 1]), None, "{m:?}");
        }
        // Damaged anywhere, it never panics (it may still read as
        // something: links check what arrives, not this).
        for i in 0..bytes.len() {
            for flip in [0x01, 0x80, 0xff] {
                let mut bad = bytes.clone();
                bad[i] ^= flip;
                let _ = Msg::decode(&bad);
            }
        }
    }

    #[test]
    fn garbage_never_panics() {
        let mut seed = 0x9e37_79b9u32;
        for len in 0..400 {
            let bytes: Vec<u8> = (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    seed as u8
                })
                .collect();
            let _ = Msg::decode(&bytes);
            let _ = Beacon::read(&mut Reader::new(&bytes));
            let _ = read_header(&mut Reader::new(&bytes));
        }
    }

    #[test]
    fn every_message_round_trips() {
        let mut slots = [Slot::open(0), Slot::open(1), Slot::open(2), Slot::open(3)];
        slots[0] = Slot {
            kind: SlotKind::Human,
            name: "Mario".into(),
            ready: true,
            version: "0.1.0 abc".into(),
            ..Slot::open(0)
        };
        slots[2].kind = SlotKind::Ai;
        // The biggest armies, every slot full: still one message.
        for (i, s) in slots.iter_mut().enumerate() {
            s.army = (0..MAX_ARMY as u16).map(|u| u * 300 + i as u16).collect();
        }
        let grand = LobbySettings {
            budget: Budget::GrandBattle,
            ..LobbySettings::default()
        };
        round_trip(Msg::Hello {
            version: "0.1.0".into(),
            data_hash: 0xdead_beef_0123_4567,
            name: "Alex".into(),
            faction: Faction::Choir,
            color: 3,
        });
        round_trip(Msg::Welcome { slot: 2 });
        round_trip(Msg::Refuse {
            reason: "full".into(),
        });
        round_trip(Msg::Lobby(Box::new(LobbyWire {
            slots: slots.clone(),
            settings: grand.clone(),
            host_version: "0.1.0".into(),
        })));
        for edit in [
            SlotEdit::Faction(Faction::Brood),
            SlotEdit::Color(2),
            SlotEdit::Team(1),
            SlotEdit::Ready(true),
            SlotEdit::Kind(SlotKind::Ai),
            SlotEdit::AiLevel(2),
            SlotEdit::Army(vec![0, 4, 4, 1000]),
        ] {
            let bytes = Msg::Edit { slot: 1, edit }.encode();
            // SlotEdit has no PartialEq; compare the bytes.
            assert_eq!(Msg::decode(&bytes).map(|m| m.encode()), Some(bytes));
        }
        // Places and maps out of range are refused, so a bad host can't
        // send a joiner indexing past the lobby's lists.
        let refused = [
            Msg::Welcome {
                slot: MAX_PLAYERS as u8,
            },
            Msg::Start {
                seed: 7,
                map_hash: 0,
                settings: grand.clone(),
                players: slots[..2].to_vec(),
                you: 2,
                input_delay: 2,
            },
            Msg::Start {
                seed: 7,
                map_hash: 0,
                settings: LobbySettings {
                    map: MAPS.len(),
                    ..grand.clone()
                },
                players: slots.to_vec(),
                you: 1,
                input_delay: 2,
            },
        ];
        for m in refused {
            assert_eq!(Msg::decode(&m.encode()), None, "{m:?}");
        }
        round_trip(Msg::Start {
            seed: 7,
            map_hash: u64::MAX,
            settings: grand,
            players: slots.to_vec(),
            you: 1,
            input_delay: 2,
        });
        round_trip(Msg::Loaded { error: None });
        round_trip(Msg::Loaded {
            error: Some("map".into()),
        });
        round_trip(Msg::Bundle {
            turn: 123_456,
            commands: vec![PlayerCommand {
                player: 1,
                command: Command::MoveTo {
                    regiment: RegimentHandle {
                        index: 5,
                        generation: 1,
                    },
                    target: FxVec2 {
                        x: Fx::from_raw(100),
                        y: Fx::from_raw(-100),
                    },
                    facing: Some(Angle(9)),
                    files: 8,
                    pace: Fx::ZERO,
                },
            }],
        });
        round_trip(Msg::Orders { commands: vec![] });
        round_trip(Msg::Hash { tick: 10, hash: 99 });
        round_trip(Msg::Notice { text: "hi".into() });
        round_trip(Msg::Delay { turns: 3 });
        round_trip(Msg::Wait {
            who: "Alex".into(),
            seconds: 90,
        });
        round_trip(Msg::Desync { tick: 1230 });
        round_trip(Msg::Bye {
            reason: "bye".into(),
        });
    }

    #[test]
    fn an_oversized_army_is_refused() {
        let mut w = Writer::new();
        w.var(MAX_ARMY as u64 + 1);
        for _ in 0..=MAX_ARMY {
            w.var(0);
        }
        assert_eq!(read_army(&mut Reader::new(&w.buf)), None);
    }

    #[test]
    fn names_are_cut_on_a_character_boundary() {
        assert_eq!(short_name("Mario"), "Mario");
        let long = "é".repeat(20);
        let cut = short_name(&long);
        assert!(cut.len() <= MAX_NAME);
        assert_eq!(cut, "é".repeat(12));
    }
}
