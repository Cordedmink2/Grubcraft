//! Who fights whom and who commands whom: the one hostility test every
//! rule asks (docs/18-ai.md, engine hook 1), and the one command-authority
//! test every order asks (hook 2).
//!
//! Parties on the same team are allies: their regiments don't fight each
//! other, can't be ordered to attack each other, and count as friends for
//! morale. Everyone else is hostile. The teams are filled from the lobby's
//! when the match starts and stay fixed for the match, so like the map they
//! are setup data rather than hashed state. Later they grow into the
//! relations ledger, where stances change during play.
//!
//! Each party commands its own army until it hands it to an ally
//! ([`crate::Command::HandOver`]): then the ally gives its orders, until the
//! party takes it back. That is how a teammate (or the host's computer
//! player) takes over for a player who dropped out, and later how a hired
//! company takes orders. Who commands whom changes during play, so it is
//! hashed.

use alloc::vec::Vec;

use crate::{MAX_PARTIES, PlayerId, StateHasher};

/// Parties' teams and commanders, and the tests that read them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stances {
    /// Team of each party, by party id. A party past the end has no team:
    /// it is hostile to everyone but itself. Empty means every party fights
    /// every other, which is what tests and the sandbox use.
    team: Vec<u8>,
    /// Who commands each party's army, by party id; empty until the first
    /// hand-over, and a party past the end commands its own.
    commander: Vec<PlayerId>,
}

impl Stances {
    /// `teams[p]` is party `p`'s team; parties with the same number are allies.
    pub fn from_teams(teams: &[u8]) -> Stances {
        Stances {
            team: teams.to_vec(),
            commander: Vec::new(),
        }
    }

    /// Whether `a`'s regiments fight `b`'s.
    pub fn hostile(&self, a: PlayerId, b: PlayerId) -> bool {
        if a == b {
            return false;
        }
        match (self.team.get(a as usize), self.team.get(b as usize)) {
            (Some(x), Some(y)) => x != y,
            _ => true,
        }
    }

    /// The party whose orders `party`'s regiments take: itself, unless it
    /// handed them to an ally.
    pub fn commander(&self, party: PlayerId) -> PlayerId {
        self.commander
            .get(usize::from(party))
            .copied()
            .unwrap_or(party)
    }

    /// Whether `player` may order a regiment of `owner`'s.
    pub fn can_command(&self, player: PlayerId, owner: PlayerId) -> bool {
        self.commander(owner) == player
    }

    /// `party` hands its army to `to`, or takes it back (`to` is itself).
    /// Refused for a party the sim has no room for, or one it fights.
    pub(crate) fn hand_over(&mut self, party: PlayerId, to: PlayerId) -> bool {
        let fits = |p: PlayerId| usize::from(p) < MAX_PARTIES;
        if !fits(party) || !fits(to) || self.hostile(party, to) {
            return false;
        }
        if self.commander.is_empty() {
            self.commander = (0..MAX_PARTIES as PlayerId).collect();
        }
        self.commander[usize::from(party)] = to;
        true
    }

    /// Every army handed to a party in the bitmask `out` goes back to its
    /// owner, so a party that is out commands nothing.
    pub(crate) fn take_back_from(&mut self, out: u8) {
        for (p, c) in self.commander.iter_mut().enumerate() {
            if out & (1 << *c) != 0 {
                *c = p as PlayerId;
            }
        }
    }

    /// Hash who commands whom, and nothing while every party commands its
    /// own, so matches with no hand-over hash as they always have.
    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        let handed = (self.commander.iter().enumerate()).any(|(p, &c)| usize::from(c) != p);
        if handed {
            for &c in &self.commander {
                h.u8(c);
            }
        }
    }

    /// Team of each party, as given at the start of the match.
    pub fn teams(&self) -> &[u8] {
        &self.team
    }

    /// Bit `s` is set when a party whose id is `s` modulo `slots` is an ally
    /// of `party` (or `party` itself). The combat grid files regiments by
    /// `owner % slots`, so a search for friends only visits these.
    pub(crate) fn friend_slots(&self, party: PlayerId, slots: usize) -> u32 {
        let mut mask = 1 << (party as usize % slots);
        if let Some(&mine) = self.team.get(party as usize) {
            for (p, &t) in self.team.iter().enumerate() {
                if t == mine {
                    mask |= 1 << (p % slots);
                }
            }
        }
        mask
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_teams_everyone_fights_everyone_else() {
        let s = Stances::default();
        assert!(!s.hostile(2, 2));
        assert!(s.hostile(0, 1));
        assert!(s.hostile(7, 200));
        assert_eq!(s.friend_slots(3, 8), 1 << 3);
    }

    #[test]
    fn teammates_are_allies() {
        // Two against two: players 0 and 2 against 1 and 3.
        let s = Stances::from_teams(&[0, 1, 0, 1]);
        assert!(!s.hostile(0, 2));
        assert!(!s.hostile(3, 1));
        assert!(s.hostile(0, 1));
        assert!(s.hostile(2, 3));
        // A party the lobby didn't list is on its own.
        assert!(s.hostile(0, 9));
        assert!(s.hostile(9, 0));
        assert_eq!(s.friend_slots(0, 8), 0b0101);
        assert_eq!(s.friend_slots(9, 8), 1 << 1);
    }

    #[test]
    fn a_party_hands_its_army_to_an_ally_and_takes_it_back() {
        let mut s = Stances::from_teams(&[0, 1, 0, 1]);
        assert!(s.can_command(0, 0));
        assert!(!s.can_command(2, 0));
        let hash = |s: &Stances| {
            let mut h = StateHasher::new();
            s.hash_into(&mut h);
            h.finish()
        };
        let untouched = hash(&s);
        assert_eq!(untouched, hash(&Stances::default()));
        // Not to an enemy, nor to a party past the table.
        assert!(!s.hand_over(0, 1));
        assert!(!s.hand_over(0, MAX_PARTIES as PlayerId));
        assert!(s.hand_over(0, 2));
        assert!(s.can_command(2, 0));
        assert!(!s.can_command(0, 0));
        assert_eq!(s.commander(0), 2);
        assert_ne!(hash(&s), untouched);
        assert!(s.hand_over(0, 0));
        assert!(s.can_command(0, 0));
        assert_eq!(hash(&s), untouched);
    }

    #[test]
    fn armies_handed_to_a_party_that_is_out_go_back() {
        let mut s = Stances::from_teams(&[0, 1, 0, 1]);
        assert!(s.hand_over(0, 2));
        assert!(s.hand_over(1, 3));
        s.take_back_from(1 << 2);
        assert_eq!(s.commander(0), 0);
        assert_eq!(s.commander(1), 3);
    }
}
