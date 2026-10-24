//! Soldier and building models, baked from glTF by `tools/gc_artbake` into
//! `assets/models.pak` and `assets/buildings/*.pak`, joined into one pack by
//! the build script and embedded in the executable, and the choice of
//! which animation frame each soldier shows.

use gc_assets::anim::{Act, Weapon};
use gc_assets::detail::Detail;
use gc_assets::model::Models;
use gc_assets::{Aligned, Pack};

use crate::moves::{MoveClip, Moves, Want};
use crate::puppets::{KillTiming, Kills, Move};

static PACK: &Aligned<[u8]> = &Aligned(*include_bytes!(concat!(env!("OUT_DIR"), "/models.pak")));

/// The embedded models. The pack is checked when the game starts; a failure
/// means the committed pack is broken, so the game runs with boxes instead.
pub fn load() -> Result<Models<'static>, String> {
    let pack = Pack::parse(&PACK.0).map_err(|e| format!("embedded model pack: {e}"))?;
    Models::from_pack(&pack).map_err(|e| format!("embedded model pack: {e}"))
}

static DETAIL: &Aligned<[u8]> = &Aligned(*include_bytes!("../../../assets/detail.pak"));

/// The embedded surface detail textures (`assets/detail.pak`), layered over
/// the models' own. A failure means the committed pack is broken, so the
/// models draw without detail.
pub fn load_detail() -> Result<Detail<'static>, String> {
    let pack = Pack::parse(&DETAIL.0).map_err(|e| format!("embedded detail pack: {e}"))?;
    Detail::from_pack(&pack).map_err(|e| format!("embedded detail pack: {e}"))
}

/// What a soldier is doing, which picks the clip he plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Idle,
    Walk,
    Run,
    Attack,
    Attack2,
    Shoot,
    Cheer,
    /// Reeling from a blow.
    Hit,
    /// Guarding against a blow.
    Block,
}

/// The two bone texture rows to blend between and how far, or `None` when
/// the model lacks every clip that could stand in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub rows: [u16; 2],
    pub blend: f32,
}

/// A soldier instance's `flags`: the blend toward the second frame, the
/// selection `highlight`, and whether it lies `flat`, as 0 to 255.
pub fn pose_flags(pose: Pose, highlight: u8, flat: bool) -> [u8; 4] {
    [
        (pose.blend * 255.0) as u8,
        highlight,
        u8::from(flat) * 255,
        0,
    ]
}

const ACTIONS: [Action; 9] = [
    Action::Idle,
    Action::Walk,
    Action::Run,
    Action::Attack,
    Action::Attack2,
    Action::Shoot,
    Action::Cheer,
    Action::Hit,
    Action::Block,
];

/// What the move picker is asked for an action.
fn want(action: Action) -> Want {
    let (act, seed) = match action {
        Action::Idle => (Act::Idle, 0),
        Action::Walk => (Act::Walk, 0),
        Action::Run => (Act::Run, 0),
        Action::Attack => (Act::Attack, 0),
        // The model's second attack, or its only one.
        Action::Attack2 => (Act::Attack, 1),
        Action::Shoot => (Act::Shoot, 0),
        Action::Cheer => (Act::Cheer, 0),
        Action::Hit => (Act::Hit, 0),
        Action::Block => (Act::Block, 0),
    };
    Want {
        seed,
        ..Want::new(act)
    }
}

/// How far into the attack clip a parry raises the weapon to meet the blow,
/// and into the block clip a soldier ducks behind his guard.
const PARRY_DEPTH: f32 = 0.35;
const BLOCK_DEPTH: f32 = 0.25;
/// A knockdown spends this much of its time falling, and this much getting
/// up; between the two he lies still.
const DOWN_FALL: f32 = 0.25;
const DOWN_RISE: f32 = 0.35;

/// One model's moves for what soldiers do, picked once.
struct ModelClips {
    /// In the order of `ACTIONS`, which is the enum's.
    actions: [Option<MoveClip>; ACTIONS.len()],
    die: Option<MoveClip>,
    /// The picks for a duel's parry and stagger: moves made for them when
    /// the model has them, else what the picker falls back to (an attack
    /// raised partway, a hit).
    parry: Option<MoveClip>,
    stagger: Option<MoveClip>,
}

/// A matched kill: the killer's and the victim's halves of one move, made
/// together, as long as each other and played as made (not retimed like an
/// attack): started at once and played at one speed, they meet `contact` of
/// the way through. A kill is longer than a swing, since the victim falls
/// and lies still in it; its own length is `frames / fps`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KillPair {
    pub kill: MoveClip,
    pub killed: MoveClip,
    /// How far apart the two men stand, origin to origin, in tiles (the
    /// world's units, which models are baked in), the victim facing the
    /// killer. Warp both towards this before the contact.
    pub reach: f32,
    /// How far through both halves (0 to 1, as [`MoveClip::at`] counts) the
    /// blow lands.
    pub contact: f32,
}

impl KillPair {
    /// How the puppets time it.
    fn timing(&self) -> KillTiming {
        KillTiming {
            seconds: self.kill.seconds(),
            contact: self.contact,
            reach: self.reach,
        }
    }
}

/// A kill a model can play.
#[derive(Clone, Copy, Debug)]
struct KillFor {
    /// The skeleton the victim's half is made on.
    victim: u32,
    /// How well it fits the killer's weapon (as the move picker scores it).
    fit: u32,
    /// Its index in [`Clips::pairs`].
    id: u16,
}

/// How far a victim's size may be from the killer's for a kill made for two
/// men of one size to still meet.
const KILL_SIZES: core::ops::RangeInclusive<f32> = 0.9..=1.1;

/// Per model, the move for each action, picked once by the move picker
/// (`moves.rs`).
pub struct Clips {
    by_model: Vec<ModelClips>,
    /// Per model posed from a skeleton, the skeleton and how much bigger
    /// the model is than it.
    body: Vec<Option<(u32, f32)>>,
    /// Per model, the kills it can play, their reach in its size.
    kills: Vec<Vec<KillFor>>,
    /// Every model's kills, by [`KillFor::id`]: the index the puppets name
    /// one by ([`Move::Kill`], [`crate::puppets::Corpse::kill`]).
    pairs: Vec<KillPair>,
}

impl Clips {
    pub fn new(models: &Models<'_>) -> Clips {
        let moves = Moves::new(models);
        let by_model = (0..models.models.len())
            .map(|m| {
                // A model that names what it holds plays the moves made for it.
                let weapon = models.anim.weapon_of(m);
                let pick = |w: Want| moves.pick(m, Want { weapon, ..w });
                ModelClips {
                    actions: ACTIONS.map(|a| pick(want(a))),
                    die: pick(Want::new(Act::Death)),
                    parry: pick(Want::new(Act::Parry)),
                    stagger: pick(Want::new(Act::Stagger)),
                }
            })
            .collect();
        let anim = &models.anim;
        let body: Vec<Option<(u32, f32)>> = (0..models.models.len())
            .map(|m| {
                let fit = anim.fit(m)?;
                // The model's first fix sizes the skeleton's space into it.
                let f = anim.fixes.get(fit.first_fix as usize)?;
                Some((
                    fit.skeleton,
                    (f[0] * f[0] + f[4] * f[4] + f[8] * f[8]).sqrt(),
                ))
            })
            .collect();
        let (mut kills, mut pairs) = (Vec::with_capacity(body.len()), Vec::new());
        for (m, body) in body.iter().enumerate() {
            let mut own = Vec::new();
            if let Some((skeleton, scale)) = *body {
                let weapon = anim.weapon_of(m);
                for p in anim.pairs {
                    let (kill, killed) =
                        (&anim.moves[p.kill as usize], &anim.moves[p.killed as usize]);
                    let fit = crate::moves::fits(kill.weapon(), weapon, Weapon::Any);
                    let (Some(fit), Ok(id)) = (fit, u16::try_from(pairs.len())) else {
                        continue;
                    };
                    if u32::from(kill.skeleton) != skeleton {
                        continue;
                    }
                    own.push(KillFor {
                        victim: u32::from(killed.skeleton),
                        fit,
                        id,
                    });
                    pairs.push(KillPair {
                        kill: MoveClip::of_move(kill),
                        killed: MoveClip::of_move(killed),
                        reach: p.reach * scale,
                        contact: p.contact,
                    });
                }
            }
            kills.push(own);
        }
        Clips {
            by_model,
            body,
            kills,
            pairs,
        }
    }

    /// The kills a man of `killer_model` may play on one of `victim_model`
    /// (see [`Clips::kill_pair`]): those that fit best, in a fixed order.
    fn best_kills(
        &self,
        killer_model: usize,
        victim_model: usize,
    ) -> impl Iterator<Item = &KillFor> + '_ {
        let fits = match (self.body.get(killer_model), self.body.get(victim_model)) {
            (Some(&Some((_, size))), Some(&Some((victim, theirs))))
                if KILL_SIZES.contains(&(theirs / size)) =>
            {
                self.kills.get(killer_model).map(|k| (k.as_slice(), victim))
            }
            _ => None,
        };
        let (kills, victim) = fits.unwrap_or((&[], 0));
        let best = kills
            .iter()
            .filter(|k| k.victim == victim)
            .map(|k| k.fit)
            .max();
        kills
            .iter()
            .filter(move |k| k.victim == victim && Some(k.fit) == best)
    }

    /// The matched kill a man of `killer_model` plays on one of
    /// `victim_model` (model indices, as for [`Clips::clip`]), if the two
    /// have one made for them and the killer's weapon, and are near enough
    /// one size for it to meet; otherwise `None`, and
    /// the fight plays a plain attack with the victim's own hit and death.
    /// `seed` picks among the kills that fit best (made for the killer's
    /// weapon before made for any). The battle takes them all at once,
    /// through [`Clips::kills_of`]; the tests ask one at a time.
    #[cfg(test)]
    pub fn kill_pair(
        &self,
        killer_model: usize,
        victim_model: usize,
        seed: u32,
    ) -> Option<KillPair> {
        let n = self.best_kills(killer_model, victim_model).count();
        let k = self
            .best_kills(killer_model, victim_model)
            .nth(seed as usize % n.max(1))?;
        Some(self.pairs[usize::from(k.id)])
    }

    /// Every matched kill between the unit types whose models are `units`,
    /// for the puppets: each kill named by its index here, as [`Move::Kill`]
    /// and [`crate::puppets::Corpse::kill`] give it back.
    pub fn kills_of(&self, units: &[Option<usize>]) -> Kills {
        let mut kills = Kills::new(self.pairs.iter().map(KillPair::timing).collect());
        let mut ids = Vec::new();
        for (k, killer) in units.iter().enumerate() {
            let Some(killer) = *killer else { continue };
            if self.kills.get(killer).is_none_or(Vec::is_empty) {
                continue;
            }
            for (v, victim) in units.iter().enumerate() {
                let Some(victim) = *victim else { continue };
                ids.clear();
                ids.extend(self.best_kills(killer, victim).map(|k| k.id));
                kills.add(k as u16, v as u16, &ids);
            }
        }
        kills
    }

    /// A half of a matched kill for `model` to show (the showroom): the
    /// killer's half of its first kill, else the victim's half of the first
    /// made for its skeleton.
    pub fn kill_half(&self, model: usize) -> Option<MoveClip> {
        if let Some(k) = self.kills.get(model)?.first() {
            return Some(self.pairs[k.id as usize].kill);
        }
        let (victim, _) = (*self.body.get(model)?)?;
        let mut all = self.kills.iter().flatten();
        let k = all.find(|k| k.victim == victim)?;
        Some(self.pairs[k.id as usize].killed)
    }

    /// The move `model` plays for `action`.
    pub fn clip(&self, model: usize, action: Action) -> Option<MoveClip> {
        self.by_model.get(model)?.actions[action as usize]
    }

    /// The pose for `action` `seconds` into its move, looping.
    pub fn pose(&self, model: usize, action: Action, seconds: f32) -> Option<Pose> {
        Some(self.clip(model, action)?.looping(seconds))
    }

    /// The pose `fraction` (0 to 1) of the way through the move for
    /// `action`, played once whatever its length.
    pub fn at(&self, model: usize, action: Action, fraction: f32) -> Option<Pose> {
        Some(self.clip(model, action)?.at(fraction))
    }

    /// The pose `seconds` after falling dead (negative while he still
    /// stands, waiting for the blow), and whether to lay the model flat: see
    /// `fall`. A man who fell to matched kill `kill` plays its victim's
    /// half, which began with his killer's, its contact before the blow.
    pub fn death(&self, model: usize, seconds: f32, kill: Option<u16>) -> Option<(Pose, bool)> {
        match kill.and_then(|k| self.pairs.get(usize::from(k))) {
            Some(p) => Some((p.killed.at(seconds / p.killed.seconds() + p.contact), false)),
            None => self.fall(model, |die| seconds * die.fps, seconds >= 0.0),
        }
    }

    /// The death move at the frame `frame` picks, held on its first and
    /// last frames, or for a model without one its idle pose, laid flat
    /// when `down`.
    fn fall(
        &self,
        model: usize,
        frame: impl FnOnce(&MoveClip) -> f32,
        down: bool,
    ) -> Option<(Pose, bool)> {
        match self.by_model.get(model)?.die {
            Some(die) => Some((die.at_frame(frame(&die)), false)),
            None => Some((self.pose(model, Action::Idle, 0.0)?, down)),
        }
    }

    /// The pose for a soldier's move in a duel, and whether to lay the model
    /// flat (a knockdown on a model with no death clip). `seconds` keeps the
    /// guard's idle loop going.
    pub fn fight(&self, model: usize, fight: Move, seconds: f32) -> Option<(Pose, bool)> {
        let standing = |pose: Option<Pose>| Some((pose?, false));
        let own = self.by_model.get(model)?;
        match fight {
            Move::Guard => standing(self.pose(model, Action::Idle, seconds)),
            Move::Cheer => standing(self.pose(model, Action::Cheer, seconds)),
            Move::Kill { pair, at } => {
                standing(Some(self.pairs.get(usize::from(pair))?.kill.at(at)))
            }
            Move::Killed { pair, at } => {
                standing(Some(self.pairs.get(usize::from(pair))?.killed.at(at)))
            }
            Move::Swing { at, alt } => {
                let attack = if alt { Action::Attack2 } else { Action::Attack };
                standing(self.at(model, attack, at))
            }
            Move::Parry(f) => {
                let parry = own.parry?;
                if parry.act == Act::Parry {
                    standing(Some(parry.at(f)))
                } else {
                    // Raise the weapon to meet the blow and recover.
                    standing(Some(parry.at(PARRY_DEPTH * peak(f))))
                }
            }
            // Duck behind the shield and come back up.
            Move::Block(f) => standing(self.at(model, Action::Block, BLOCK_DEPTH * peak(f))),
            // A stagger, or the hit standing in for one.
            Move::Stagger(f) => standing(Some(own.stagger?.at(f))),
            // Fall, lie still a moment and get up: the death move played
            // forward, then back.
            Move::Down(f) => {
                let depth = down_depth(f);
                self.fall(model, |die| depth * (die.frames - 1) as f32, depth > 0.5)
            }
        }
    }
}

/// How far down a knockdown `f` (0 to 1) through has him: falling, lying
/// still a moment, then getting up.
pub fn down_depth(f: f32) -> f32 {
    (f / DOWN_FALL).min((1.0 - f) / DOWN_RISE).clamp(0.0, 1.0)
}

/// Seconds over which a dying soldier's feet leave the ground's hold.
const FEET_FADE: f32 = 0.3;

/// [`gc_render::SoldierInstance::feet`] for a soldier `share` (0 to 1) of the
/// way to keeping his feet as the move puts them.
pub fn feet_as_posed(share: f32) -> u8 {
    (share.clamp(0.0, 1.0) * f32::from(gc_render::FEET_AS_POSED)).round() as u8
}

/// [`gc_render::SoldierInstance::feet`] for a soldier `seconds` into his
/// death (negative while he still stands, waiting for the blow).
pub fn dying_feet(seconds: f32) -> u8 {
    feet_as_posed(seconds / FEET_FADE)
}

/// Small differences between soldiers of one model, so a regiment does not
/// look cloned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// For the last byte of [`gc_render::SoldierInstance::color`]
    /// ([`gc_render::TONE_NEUTRAL`] leaves him as painted).
    pub tone: u8,
    /// Playback rate of his idle, walk and run loops, 0.93 to 1.07.
    pub rate: f32,
    /// His height, 0.97 to 1.03.
    pub scale: f32,
}

/// The look of a soldier whose stable look value is `seed`
/// (`puppets::Soldier::look`).
pub fn look(seed: u8) -> Look {
    let mix = |salt| crate::puppets::mix(u32::from(seed), 0, 0, salt);
    Look {
        tone: seed,
        rate: 0.93 + 0.14 * mix(1),
        scale: 0.97 + 0.06 * mix(2),
    }
}

/// 0 at both ends of `f` (0 to 1), rising to 1 halfway.
fn peak(f: f32) -> f32 {
    1.0 - (2.0 * f - 1.0).abs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_assets::model::ClipKind;

    #[test]
    fn looks_stay_in_range_and_vary() {
        let looks: Vec<Look> = (0..=255).map(look).collect();
        assert!(looks.iter().all(|l| (0.93..=1.07).contains(&l.rate)));
        assert!(looks.iter().all(|l| (0.97..=1.03).contains(&l.scale)));
        assert!(looks.iter().any(|l| l.rate < 0.96) && looks.iter().any(|l| l.rate > 1.04));
        assert!(looks.iter().any(|l| l.scale < 0.98) && looks.iter().any(|l| l.scale > 1.02));
    }

    #[test]
    fn embedded_models_load_and_dress_every_unit_type() {
        let models = load().unwrap();
        let data = crate::data::load();
        for u in data.units {
            assert!(
                models.by_name(u.name()).is_some(),
                "no model for {}",
                u.name()
            );
        }
    }

    #[test]
    fn actions_are_listed_in_order() {
        for (k, a) in ACTIONS.into_iter().enumerate() {
            assert_eq!(a as usize, k, "{a:?}");
        }
    }

    /// The clips the game picked before the move picker, best first: plain
    /// clips must still play exactly these.
    fn old_candidates(action: Action) -> &'static [ClipKind] {
        use ClipKind::*;
        match action {
            Action::Idle => &[Idle],
            Action::Walk => &[Walk, Run, Idle],
            Action::Run => &[Run, Walk, Idle],
            Action::Attack => &[Attack, Attack2, Idle],
            Action::Attack2 => &[Attack2, Attack, Idle],
            Action::Shoot => &[Shoot, Attack, Idle],
            Action::Cheer => &[Cheer, Idle],
            Action::Hit => &[Hit, Idle],
            Action::Block => &[Block, Idle],
        }
    }

    #[test]
    fn the_picker_plays_the_clips_the_game_always_did() {
        let models = load().unwrap();
        let clips = Clips::new(&models);
        let rows = |c: Option<MoveClip>| c.map(|c| (c.first_row, c.frames));
        let old = |m, kinds: &[ClipKind]| {
            kinds
                .iter()
                .find_map(|&k| models.clip(m, k))
                .map(|c| (c.first_row, c.frames))
        };
        // A model that names what it holds may play moves made for it
        // instead (see `weapon_moves_are_played_by_the_models_holding_them`).
        let made_for = |m: usize, c: Option<MoveClip>| {
            let weapon = models.anim.weapon_of(m);
            weapon != Weapon::Any && c.is_some_and(|c| c.weapon == weapon)
        };
        for m in 0..models.models.len() {
            for a in ACTIONS {
                let c = clips.clip(m, a);
                if !made_for(m, c) {
                    assert_eq!(rows(c), old(m, old_candidates(a)), "{m} {a:?}");
                }
            }
            let own = &clips.by_model[m];
            assert_eq!(rows(own.die), old(m, &[ClipKind::Die]));
            if !made_for(m, own.parry) {
                assert_eq!(rows(own.parry), old(m, old_candidates(Action::Attack)));
            }
            assert_eq!(rows(own.stagger), old(m, old_candidates(Action::Hit)));
        }
    }

    #[test]
    fn weapon_moves_are_played_by_the_models_holding_them() {
        let models = load().unwrap();
        let clips = Clips::new(&models);
        let by = |name| models.by_name(name).unwrap();
        let weapon = |name, a| clips.clip(by(name), a).map(|c| (c.act, c.weapon));
        // The Ember men's fight library: katana and spear moves.
        let katana = Some((Act::Attack, Weapon::TwoHanded));
        assert_eq!(weapon("Katana Samurai", Action::Attack), katana);
        assert_eq!(weapon("The Ronin", Action::Attack), katana);
        let spear = Some((Act::Attack, Weapon::Spear));
        assert_eq!(weapon("Yari Ashigaru", Action::Attack), spear);
        let parry = clips.by_model[by("Yari Ashigaru")].parry.unwrap();
        assert_eq!((parry.act, parry.weapon), (Act::Parry, Weapon::Spear));
        // Walking is still the model's own clip, and a kanabo, with no moves
        // made for it, keeps the Oni-Bound's own attack.
        assert_eq!(
            weapon("Katana Samurai", Action::Walk),
            Some((Act::Walk, Weapon::Any))
        );
        assert_eq!(
            weapon("Oni-Bound", Action::Attack),
            Some((Act::Attack, Weapon::Any))
        );
    }

    #[test]
    fn a_kill_is_picked_for_the_victims_skeleton_and_the_killers_weapon() {
        let models = load().unwrap();
        let mut clips = Clips::new(&models);
        let any = clips.clip(0, Action::Attack).unwrap();
        // Kill `id` is the one whose killer's half starts at row 10 * (id + 1).
        clips.pairs = (1..=4)
            .map(|n| KillPair {
                kill: MoveClip {
                    first_row: 10 * n,
                    ..any
                },
                killed: any,
                reach: 1.0,
                contact: 0.3,
            })
            .collect();
        let kill = |victim, fit, row: u32| KillFor {
            victim,
            fit,
            id: (row / 10 - 1) as u16,
        };
        // Model 0 is posed from skeleton 7, model 1 from 8 and model 3 too
        // but half as big again; model 2 is not posed from one.
        clips.body = vec![Some((7, 1.0)), Some((8, 1.05)), None, Some((8, 1.5))];
        clips.kills = vec![vec![
            kill(8, 1, 10),
            kill(8, 2, 20),
            kill(8, 2, 30),
            kill(7, 2, 40),
        ]];
        let row = |victim, seed| clips.kill_pair(0, victim, seed).map(|p| p.kill.first_row);
        // The kills made for the weapon beat one made for any; the seed picks.
        assert_eq!(row(1, 0), Some(20));
        assert_eq!(row(1, 1), Some(30));
        assert_eq!(row(0, 5), Some(40));
        // No kill on a victim posed from no skeleton or of another size, nor
        // by a model without.
        assert_eq!(row(2, 0), None);
        assert_eq!(row(3, 0), None);
        assert_eq!(clips.kill_pair(1, 0, 0), None);
        assert_eq!(clips.kill_pair(99, 0, 0), None);
    }

    #[test]
    fn katana_and_spear_men_have_matched_kills_on_any_man() {
        let models = load().unwrap();
        let clips = Clips::new(&models);
        let by = |name| models.by_name(name).unwrap();
        let kill = |killer, victim| clips.kill_pair(by(killer), by(victim), 0);
        for (killer, weapon) in [
            ("Katana Samurai", Weapon::TwoHanded),
            ("The Ronin", Weapon::TwoHanded),
            ("Yari Ashigaru", Weapon::Spear),
        ] {
            for victim in ["Yari Ashigaru", "Katana Samurai", "The Ronin"] {
                let p = kill(killer, victim).unwrap_or_else(|| panic!("{killer} on {victim}"));
                assert_eq!((p.kill.act, p.kill.weapon), (Act::Kill, weapon));
                assert_eq!(p.killed.act, Act::Killed);
                assert_eq!(p.kill.frames, p.killed.frames);
                assert!(
                    p.contact > 0.1 && p.contact < 0.4,
                    "{killer}: {}",
                    p.contact
                );
                // A man's length apart for a katana, a spear's for a spear.
                assert!((0.8..4.5).contains(&p.reach), "{killer}: {}", p.reach);
            }
        }
        // The victims' halves were given to every Choir and Concord man on
        // foot, on rigs of their own that share the men's joints.
        for victim in [
            "The Onmyoji",
            "Yumi Ashigaru",
            "Tuner",
            "Shardlings",
            "Prism Lancers",
            "Conductor",
            "The Maestro",
            "The Refractor",
            "The Hollow Bell",
            "Ledger Clerk",
            "Bone Pikes",
            "Crossbow Auditors",
            "Gravewardens",
            "Lantern Wraiths",
            "The Insolvent",
            "Bound Dead",
            "Grubfolk Militia",
            "The Magnate",
            "The Undertaker",
            "The Usurer",
        ] {
            for killer in ["Katana Samurai", "Yari Ashigaru"] {
                let p = kill(killer, victim).unwrap_or_else(|| panic!("{killer} on {victim}"));
                assert_eq!(p.kill.frames, p.killed.frames);
            }
        }
        // The two katana kills both come up as the seed varies.
        let rows: std::collections::BTreeSet<u32> = (0..8)
            .map(|seed| clips.kill_pair(by("Katana Samurai"), by("Yari Ashigaru"), seed))
            .map(|p| p.unwrap().kill.first_row)
            .collect();
        assert_eq!(rows.len(), 2);
        // A kanabo has none made for it, and the Oni-Bound, on the men's rig
        // but half as big again, is too big for a kill made for two men.
        assert_eq!(kill("Oni-Bound", "Yari Ashigaru"), None);
        assert_eq!(kill("Katana Samurai", "Oni-Bound"), None);
    }

    #[test]
    fn unit_types_get_their_matched_kills_by_index() {
        let models = load().unwrap();
        let clips = Clips::new(&models);
        let by = |name| models.by_name(name);
        // Unit types 0 to 3: a katana man, a spearman, an Oni-Bound, and a
        // type with no model.
        let units = [
            by("Katana Samurai"),
            by("Yari Ashigaru"),
            by("Oni-Bound"),
            None,
        ];
        let kills = clips.kills_of(&units);
        // Both katana kills are there to pick, under their indices.
        let rows: std::collections::BTreeSet<u32> = (0..10)
            .filter_map(|n| kills.pick(0, 1, n as f32 / 10.0))
            .map(|(k, _)| clips.pairs[usize::from(k)].kill.first_row)
            .collect();
        assert_eq!(rows.len(), 2, "{rows:?}");
        let seeded = |seed| clips.kill_pair(units[0].unwrap(), units[1].unwrap(), seed);
        assert!((0..2).all(|seed| rows.contains(&seeded(seed).unwrap().kill.first_row)));
        assert!(kills.pick(1, 0, 0.5).is_some());
        assert!(kills.pick(1, 1, 0.5).is_some());
        // None on or by the Oni-Bound, or a type without a model.
        for (k, v) in [(0, 2), (2, 1), (3, 1), (0, 3)] {
            assert_eq!(kills.pick(k, v, 0.5), None, "{k} on {v}");
        }
    }

    #[test]
    fn every_model_can_stand_walk_and_fight() {
        let models = load().unwrap();
        let clips = Clips::new(&models);
        for m in 0..models.models.len() {
            for a in ACTIONS {
                assert!(clips.pose(m, a, 1.3).is_some(), "model {m} {a:?}");
            }
        }
    }

    #[test]
    fn duel_moves_play_their_clips() {
        let models = load().unwrap();
        let clips = Clips::new(&models);
        let m = models.by_name("Bone Pikes").unwrap();
        let clip = |k| *models.clip(m, k).unwrap();
        let (die, duck, hit) = (
            clip(ClipKind::Die),
            clip(ClipKind::Block),
            clip(ClipKind::Hit),
        );
        let rows = |f: Move| clips.fight(m, f, 0.0).unwrap();
        // A knockdown falls, lies on the death clip's last frame, and gets
        // up again; it never lays the model flat when it has a death clip.
        let (start, flat) = rows(Move::Down(0.0));
        assert_eq!((start.rows[0], flat), (die.first_row as u16, false));
        let lying = (die.first_row + die.frames - 1) as u16;
        assert_eq!(rows(Move::Down(0.5)).0.rows, [lying; 2]);
        assert_eq!(rows(Move::Down(1.0)).0.rows[0], die.first_row as u16);
        // A block ducks and comes back up; a stagger plays the hit clip.
        assert_eq!(rows(Move::Block(0.0)).0.rows[0], duck.first_row as u16);
        let low = u32::from(rows(Move::Block(0.5)).0.rows[0]);
        assert!(low > duck.first_row && low < duck.first_row + duck.frames / 2);
        assert_eq!(
            rows(Move::Stagger(1.0)).0.rows,
            [(hit.first_row + hit.frames - 1) as u16; 2]
        );

        // A model without a death clip lies flat while knocked down (every
        // model has one now, so take the Skitterlings' away).
        let bug = models.by_name("Skitterlings").unwrap();
        let mut clips = Clips::new(&models);
        clips.by_model[bug].die = None;
        assert!(clips.fight(bug, Move::Down(0.5), 0.0).unwrap().1);
        assert!(!clips.fight(bug, Move::Down(0.05), 0.0).unwrap().1);
    }

    #[test]
    fn loops_wrap_and_deaths_hold() {
        let clip = MoveClip {
            first_row: 10,
            frames: 4,
            fps: 10.0,
            loops: true,
            additive: false,
            mask: gc_assets::anim::Mask::All,
            act: Act::Walk,
            weapon: gc_assets::anim::Weapon::Any,
            side: gc_assets::anim::Side::Any,
        };
        let p = clip.looping(0.35);
        assert_eq!(p.rows, [13, 10]);
        assert!((p.blend - 0.5).abs() < 1e-4);
        assert_eq!(clip.looping(0.4).rows, [10, 11]);
        assert_eq!(clip.looping(-0.05).rows, [13, 10]);

        let models = load().unwrap();
        let clips = Clips::new(&models);
        let m = (0..models.models.len())
            .find(|&m| models.clip(m, ClipKind::Die).is_some())
            .unwrap();
        let die = models.clip(m, ClipKind::Die).unwrap();
        let (end, flat) = clips.death(m, 100.0, None).unwrap();
        assert_eq!(end.rows, [(die.first_row + die.frames - 1) as u16; 2]);
        assert!(!flat);
    }
}
