//! Faction powers on screen (Conquest, docs/17-unit-rosters.md): the row of
//! buttons for the powers the game has built, aiming one at its target, and
//! the glow on whatever a power has changed. The orders themselves are the
//! simulation's `Command::UsePower`.
//!
//! The sums (why a button is dim, what it says, which target a click means)
//! are plain functions on plain data; the `Running` methods feed them from
//! the world and the mouse.

use std::fmt::Write as _;

use glam::Vec2;

use gc_data::{Faction, PowerRecord};
use gc_sim::{
    BuildingId, Command, PowerEffect, PowerTarget, Refusal, RegimentHandle, StructureHandle,
};
use gc_ui::hud::SlotState;
use gc_ui::powers::POWER_KEYS;
use winit::keyboard::KeyCode;

use crate::hud::Armed;
use crate::{Running, buildings, fx_vec};

/// The keys that press the buttons, first to last (`POWER_KEYS` names them).
const KEY_CODES: [KeyCode; POWER_KEYS.len()] = [KeyCode::F2, KeyCode::F3, KeyCode::F4, KeyCode::F5];
/// How far from a click (in tiles) one of your regiments still counts as
/// the one clicked: closer when one of your buildings is under the click
/// too, since workers crowd round their hall.
const PICK_NEAR: f32 = 1.5;
const PICK_FAR: f32 = 3.0;
/// How bright the ring shown while aiming a Banner of the Ancestors is.
const AIM_GLOW: u8 = 150;
/// How long a haste's glow takes to swell and fade, in milliseconds, and
/// its dimmest and brightest.
const GLOW_PERIOD_MS: u64 = 900;
const GLOW_MIN: u8 = 90;
const GLOW_MAX: u8 = 230;

/// Why a power can't be used now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    /// The simulation's own refusal: tier, price or cooldown.
    Refused(Refusal),
    /// A shift or a march needs regiments to move.
    NoRegiments,
}

/// What the player does to aim a power once it is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
    /// Nothing: pressing the button uses it.
    Instant,
    /// Nothing: pressing the button uses it on the selected regiments.
    Selection,
    /// Click one of your regiments or buildings.
    Unit,
    /// Click one of your finished buildings of this type, to send the
    /// selected regiments there.
    Building(BuildingId),
    /// Click a spot on the ground; the power covers this many tiles round it.
    Ground(u16),
}

impl Aim {
    /// How an `effect` is aimed (none for a power not built).
    pub fn of(effect: PowerEffect) -> Option<Aim> {
        match effect {
            PowerEffect::NotBuilt => None,
            PowerEffect::Haste { .. } => Some(Aim::Unit),
            PowerEffect::Shift { to } => Some(Aim::Building(to)),
            PowerEffect::Grant { .. } => Some(Aim::Instant),
            PowerEffect::March { .. } => Some(Aim::Selection),
            PowerEffect::Steadfast { tiles } => Some(Aim::Ground(tiles)),
        }
    }

    /// Whether it works on the selected regiments.
    fn needs_selection(self) -> bool {
        matches!(self, Aim::Selection | Aim::Building(_))
    }

    /// Whether the button is only pressed: no click after it.
    fn at_once(self) -> bool {
        matches!(self, Aim::Instant | Aim::Selection)
    }
}

/// The button a key presses, if it is one of the power keys.
pub fn key_slot(code: KeyCode) -> Option<usize> {
    KEY_CODES.iter().position(|&c| c == code)
}

/// The powers of `faction` (none for `None`) that get a button: those the
/// game has built, as their place in the powers table (which is what
/// `UsePower` names).
pub fn offered(powers: &[PowerRecord], faction: Option<Faction>) -> impl Iterator<Item = u8> + '_ {
    (powers.iter().enumerate())
        .filter(move |(_, p)| p.built() && Some(p.faction()) == faction)
        .map(|(i, _)| i as u8)
        .take(POWER_KEYS.len())
}

/// Why a power of this `effect` can't be used now: the simulation's
/// `refusal` if it gave one, else a power for the selected regiments with
/// none (`selected`) to use it on.
pub fn hold(effect: PowerEffect, refusal: Option<Refusal>, selected: usize) -> Option<Hold> {
    let needs = Aim::of(effect).is_some_and(Aim::needs_selection);
    refusal
        .map(Hold::Refused)
        .or((needs && selected == 0).then_some(Hold::NoRegiments))
}

/// A button's state: a held power is dim but still clicks (to say why).
pub fn state(hold: Option<Hold>, armed: bool) -> SlotState {
    match (hold, armed) {
        (_, true) => SlotState::Armed,
        (Some(_), false) => SlotState::Blocked,
        (None, false) => SlotState::Ready,
    }
}

/// Whole seconds until a power is ready, rounded up.
pub fn cooling_secs(ticks: u32) -> u32 {
    ticks.div_ceil(gc_sim::TICKS_PER_SECOND)
}

/// A button's second line, in 20 characters, into `out`: what it costs in
/// the `meter` it is paid from ("Resonance", "Honor"), or what holds it
/// back.
pub fn detail(
    out: &mut String,
    (cost, tier, meter): (u8, u8, &str),
    hold: Option<Hold>,
    cooling_ticks: u32,
    armed: bool,
) {
    out.clear();
    // Writing to a String can't fail.
    let _ = match hold {
        _ if armed => write!(out, "Pick a target"),
        None => write!(out, "{cost} {meter}"),
        Some(Hold::NoRegiments) => write!(out, "Select regiments"),
        Some(Hold::Refused(Refusal::Cooling)) => {
            write!(out, "Ready in {}s", cooling_secs(cooling_ticks))
        }
        Some(Hold::Refused(Refusal::Tier)) => write!(out, "Needs tier {tier}"),
        Some(Hold::Refused(Refusal::Price)) => write!(out, "Needs {cost} {meter}"),
        Some(Hold::Refused(Refusal::NoShrine)) => write!(out, "Needs a Shrine"),
        Some(Hold::Refused(_)) => write!(out, "Unavailable"),
    };
}

/// What the player is told when they press a button that is held, `meter`
/// being the one the power is paid from.
pub fn hold_message(hold: Hold, cooling_ticks: u32, meter: &str) -> String {
    match hold {
        Hold::NoRegiments => "Select some regiments first.".to_string(),
        Hold::Refused(Refusal::Cooling) => format!(
            "That power isn't ready again yet: {}s left.",
            cooling_secs(cooling_ticks)
        ),
        Hold::Refused(Refusal::Price) => format!("You haven't the {meter} for that."),
        Hold::Refused(r) => buildings::why(r).to_string(),
    }
}

/// What to click once a power is chosen. `to` names the building a shift
/// goes to.
pub fn prompt(name: &str, aim: Aim, to: &str) -> String {
    match aim {
        Aim::Building(_) => format!("{name}: pick one of your {to}s"),
        Aim::Ground(_) => format!("{name}: pick a spot on the ground"),
        Aim::Unit | Aim::Instant | Aim::Selection => {
            format!("{name}: pick one of your buildings or regiments")
        }
    }
}

/// What a click on a building and/or a regiment of yours hastes: the
/// regiment if there is one, else the building.
pub fn haste_target(
    regiment: Option<RegimentHandle>,
    building: Option<StructureHandle>,
) -> Option<PowerTarget> {
    (regiment.map(PowerTarget::Regiment)).or(building.map(PowerTarget::Structure))
}

/// The orders a press sends for a power that needs no click: its one
/// order on the caster, or one per selected regiment (sent together, in
/// one tick, the first pays and the rest ride free). None for a power
/// that is aimed.
pub fn pressed(aim: Aim, power: u8, selected: &[RegimentHandle]) -> Vec<Command> {
    let order = |target| Command::UsePower { power, target };
    match aim {
        Aim::Instant => vec![order(PowerTarget::Caster)],
        Aim::Selection => (selected.iter())
            .map(|&h| order(PowerTarget::Regiment(h)))
            .collect(),
        Aim::Unit | Aim::Building(_) | Aim::Ground(_) => Vec::new(),
    }
}

/// The order that plants a power on the ground at `at` (in tiles).
pub fn planted(power: u8, at: gc_fixed::FxVec2) -> Command {
    Command::UsePower {
        power,
        target: PowerTarget::Point(at),
    }
}

/// One `UsePower` per regiment, shifting each to `to`. Sent together, in
/// one tick, the first pays and the rest ride free.
pub fn shifts(
    power: u8,
    regiments: &[RegimentHandle],
    to: StructureHandle,
) -> impl Iterator<Item = Command> + '_ {
    regiments.iter().map(move |&regiment| Command::UsePower {
        power,
        target: PowerTarget::Shift { regiment, to },
    })
}

/// How bright a haste's glow is `ms` milliseconds into the match: it swells
/// and fades, so a hasted thing is easy to pick out.
pub fn haste_glow(ms: u64) -> u8 {
    let phase = (ms % GLOW_PERIOD_MS) as f32 / GLOW_PERIOD_MS as f32;
    let swell = 1.0 - (2.0 * phase - 1.0).abs();
    GLOW_MIN + (f32::from(GLOW_MAX - GLOW_MIN) * swell) as u8
}

impl Running {
    /// The powers of the side you command that get a button: while you
    /// command a Conquest side with the HUD up (a replay gives no orders).
    pub fn offered_powers(&self) -> impl Iterator<Item = u8> + '_ {
        let on = self.conquest_units
            && self.watching.is_none()
            && self.match_started
            && self.showroom.is_none();
        offered(
            self.data.powers,
            on.then(|| self.faction_of(self.sandbox.side)),
        )
    }

    /// What `power` does, if the match has it.
    fn power_effect(&self, power: u8) -> Option<PowerEffect> {
        self.world
            .powers()
            .get(usize::from(power))
            .map(|p| p.effect)
    }

    /// How `power` is aimed, if the match has it.
    fn power_aim(&self, power: u8) -> Option<Aim> {
        Aim::of(self.power_effect(power)?)
    }

    /// The name of the meter `power` is paid from.
    fn meter_name(&self, power: u8) -> &'static str {
        (self.data.powers.get(usize::from(power))).map_or("", PowerRecord::meter_name)
    }

    /// What holds `power` back for the side you command, if anything.
    fn power_hold(&self, power: u8) -> Option<Hold> {
        let effect = self.power_effect(power)?;
        let refusal = self.world.can_use_power(self.sandbox.side, power).err();
        hold(effect, refusal, self.selected.len())
    }

    /// Each offered power as its button: its name into `names`, its second
    /// line into `details` and its state; also how many there are.
    pub fn describe_powers(
        &self,
        names: &mut [String; POWER_KEYS.len()],
        details: &mut [String; POWER_KEYS.len()],
    ) -> ([SlotState; POWER_KEYS.len()], usize) {
        let mut states = [SlotState::Off; POWER_KEYS.len()];
        let mut shown = 0;
        for (i, power) in self.offered_powers().enumerate() {
            let (Some(record), Some(stats)) = (
                self.data.powers.get(usize::from(power)),
                self.world.powers().get(usize::from(power)),
            ) else {
                continue;
            };
            let hold = self.power_hold(power);
            let armed = self.hud.armed == Some(Armed::Power(power));
            let cooling = self.world.power_cooling(self.sandbox.side, power);
            names[i].clear();
            names[i].push_str(record.name());
            detail(
                &mut details[i],
                (stats.cost, stats.tier, record.meter_name()),
                hold,
                cooling,
                armed,
            );
            states[i] = state(hold, armed);
            shown = i + 1;
        }
        (states, shown)
    }

    /// A power's button (or key) pressed: use it if it needs no aiming, aim
    /// it if it does, or say why it can't be used. Pressing an aimed one
    /// again puts it down.
    pub fn power_press(&mut self, slot: usize) {
        let Some(power) = self.offered_powers().nth(slot) else {
            return;
        };
        if self.hud.armed == Some(Armed::Power(power)) {
            self.hud.armed = None;
        } else if let Some(hold) = self.power_hold(power) {
            let cooling = self.world.power_cooling(self.sandbox.side, power);
            self.say(hold_message(hold, cooling, self.meter_name(power)));
        } else if let Some(aim) = self.power_aim(power) {
            if aim.at_once() {
                self.orders(pressed(aim, power, &self.selected));
            } else if let Some(text) = self.power_prompt(power) {
                self.hud.armed = Some(Armed::Power(power));
                self.say(text);
            }
        }
    }

    /// The line that says what to click for `power`.
    fn power_prompt(&self, power: u8) -> Option<String> {
        let aim = self.power_aim(power)?;
        let name = self.data.powers.get(usize::from(power))?.name();
        let to = match aim {
            Aim::Building(to) => self.buildings.name(to),
            _ => "",
        };
        Some(prompt(name, aim, to))
    }

    /// The prompt for the power being aimed, if one is, for the screen.
    pub fn power_hint(&self) -> Option<String> {
        let Some(Armed::Power(power)) = self.hud.armed else {
            return None;
        };
        let text = self.power_prompt(power)?;
        Some(format!("{text} - Esc or right-click to cancel"))
    }

    /// Put down a power being aimed that can no longer be: one that went
    /// away, or one for the selection with no regiments left selected.
    pub fn drop_stale_power(&mut self) {
        let Some(Armed::Power(power)) = self.hud.armed else {
            return;
        };
        let stale = match self.power_aim(power) {
            Some(aim) if aim.at_once() => true,
            Some(aim) => aim.needs_selection() && self.selected.is_empty(),
            None => true,
        };
        if stale || self.watching.is_some() {
            self.hud.armed = None;
        }
    }

    /// The click after choosing a power: send it at what was clicked. A
    /// click on anything else does nothing, and the power stays chosen.
    pub fn finish_power(&mut self, power: u8) {
        let Some(aim) = self.power_aim(power) else {
            self.hud.armed = None;
            return;
        };
        let commands: Vec<Command> = match aim {
            Aim::Unit => self
                .haste_target_at_cursor()
                .map(|target| Command::UsePower { power, target })
                .into_iter()
                .collect(),
            Aim::Building(to) => self
                .resonator_at_cursor(to)
                .map(|to| shifts(power, &self.selected, to).collect())
                .unwrap_or_default(),
            Aim::Ground(_) => (self.ground_under_cursor())
                .map(|at| planted(power, fx_vec(at)))
                .into_iter()
                .collect(),
            Aim::Instant | Aim::Selection => Vec::new(),
        };
        if commands.is_empty() {
            return;
        }
        self.orders(commands);
        self.hud.armed = None;
    }

    /// What a click would haste: one of your regiments, else one of your
    /// buildings.
    fn haste_target_at_cursor(&self) -> Option<PowerTarget> {
        let side = self.sandbox.side;
        let building = self.building_under_cursor(|o| o == side);
        let reach = if building.is_some() {
            PICK_NEAR
        } else {
            PICK_FAR
        };
        haste_target(self.own_regiment_at_cursor(reach), building)
    }

    /// One of your finished buildings of type `kind` under the mouse.
    fn resonator_at_cursor(&self, kind: BuildingId) -> Option<StructureHandle> {
        let side = self.sandbox.side;
        self.pick_building(|s| s.owner == side && s.finished() && s.building == kind)
    }

    /// The ground a power being aimed would cover under the mouse: its
    /// centre and radius in tiles, if one is aimed at the ground.
    pub fn power_aim_ring(&self) -> Option<(Vec2, f32)> {
        let Some(Armed::Power(power)) = self.hud.armed else {
            return None;
        };
        let Some(Aim::Ground(tiles)) = self.power_aim(power) else {
            return None;
        };
        Some((self.ground_under_cursor()?, f32::from(tiles)))
    }

    /// Ring the ground a power being aimed would cover.
    pub fn push_power_aim(&mut self) {
        if let Some((at, tiles)) = self.power_aim_ring() {
            buildings::push_aim_ring(at, tiles, AIM_GLOW, &mut self.instances);
        }
    }

    /// How bright hasted things glow now.
    pub fn haste_glow(&self) -> u8 {
        haste_glow(self.now_ms())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIFT: PowerEffect = PowerEffect::Shift { to: 3 };
    const HASTE: PowerEffect = PowerEffect::Haste { percent: 50 };
    const GRANT: PowerEffect = PowerEffect::Grant { grub: 300 };
    const MARCH: PowerEffect = PowerEffect::March { percent: 30 };
    const BANNER: PowerEffect = PowerEffect::Steadfast { tiles: 12 };

    fn regiment(index: u32) -> RegimentHandle {
        RegimentHandle {
            index,
            generation: 0,
        }
    }

    fn structure(index: u32) -> StructureHandle {
        StructureHandle {
            index,
            generation: 0,
        }
    }

    #[test]
    fn the_choir_is_offered_its_two_built_powers_and_nobody_gets_an_unbuilt_one() {
        let data = crate::data::load();
        let names = |f| {
            offered(data.powers, Some(f))
                .map(|i| data.powers[usize::from(i)].name())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Faction::Choir), ["Overclock", "Phase Shift"]);
        assert_eq!(
            offered(data.powers, None).count(),
            0,
            "no faction, no powers"
        );
        for f in Faction::ALL {
            for i in offered(data.powers, Some(f)) {
                let p = &data.powers[usize::from(i)];
                assert!(p.built() && p.faction() == f, "{}", p.name());
            }
        }
    }

    #[test]
    fn the_clans_are_offered_their_three_edicts_paid_in_honor() {
        let data = crate::data::load();
        let edicts: Vec<_> = offered(data.powers, Some(Faction::EmberClans))
            .map(|i| &data.powers[usize::from(i)])
            .collect();
        let names: Vec<_> = edicts.iter().map(|p| p.name()).collect();
        assert_eq!(
            names,
            ["Rice Tax", "Forced March", "Banner of the Ancestors"]
        );
        assert!(edicts.iter().all(|p| p.meter_name() == "Honor"));
        for p in offered(data.powers, Some(Faction::Choir)) {
            assert_eq!(data.powers[usize::from(p)].meter_name(), "Resonance");
        }
        assert!(edicts.len() <= POWER_KEYS.len());
    }

    #[test]
    fn each_effect_says_how_it_is_aimed() {
        assert_eq!(Aim::of(PowerEffect::NotBuilt), None);
        assert_eq!(Aim::of(HASTE), Some(Aim::Unit));
        assert_eq!(Aim::of(SHIFT), Some(Aim::Building(3)));
        assert_eq!(Aim::of(GRANT), Some(Aim::Instant));
        assert_eq!(Aim::of(MARCH), Some(Aim::Selection));
        assert_eq!(Aim::of(BANNER), Some(Aim::Ground(12)));
        // Only a grant and a march go off on the press.
        for (effect, at_once) in [(HASTE, false), (SHIFT, false), (GRANT, true), (MARCH, true)] {
            assert_eq!(Aim::of(effect).unwrap().at_once(), at_once, "{effect:?}");
        }
        assert!(!Aim::Ground(12).at_once());
    }

    #[test]
    fn the_power_keys_are_the_ones_the_buttons_show() {
        for (i, code) in KEY_CODES.iter().enumerate() {
            assert_eq!(key_slot(*code), Some(i));
        }
        assert_eq!(key_slot(KeyCode::KeyQ), None);
        assert_eq!(POWER_KEYS, ["F2", "F3", "F4", "F5"]);
    }

    #[test]
    fn a_power_is_held_by_the_simulations_refusal_or_by_having_nobody_to_move() {
        let held = hold;
        assert_eq!(held(HASTE, None, 0), None, "a haste needs no selection");
        assert_eq!(held(SHIFT, None, 2), None);
        assert_eq!(held(SHIFT, None, 0), Some(Hold::NoRegiments));
        assert_eq!(held(MARCH, None, 2), None);
        assert_eq!(held(MARCH, None, 0), Some(Hold::NoRegiments));
        assert_eq!(held(GRANT, None, 0), None, "a grant needs no selection");
        assert_eq!(
            held(BANNER, None, 0),
            None,
            "a banner is aimed at the ground"
        );
        assert_eq!(
            held(GRANT, Some(Refusal::Price), 0),
            Some(Hold::Refused(Refusal::Price))
        );
        assert_eq!(
            held(MARCH, Some(Refusal::Cooling), 0),
            Some(Hold::Refused(Refusal::Cooling))
        );
        assert_eq!(
            held(HASTE, Some(Refusal::Price), 0),
            Some(Hold::Refused(Refusal::Price))
        );
        // The refusal is the more basic reason.
        assert_eq!(
            held(SHIFT, Some(Refusal::Tier), 0),
            Some(Hold::Refused(Refusal::Tier))
        );
    }

    #[test]
    fn a_held_button_is_dim_but_clicks_and_an_aimed_one_is_lit() {
        let held = Some(Hold::NoRegiments);
        assert_eq!(state(None, false), SlotState::Ready);
        assert_eq!(state(held, false), SlotState::Blocked);
        assert_eq!(state(None, true), SlotState::Armed);
        assert_eq!(state(held, true), SlotState::Armed);
    }

    #[test]
    fn a_button_shows_its_price_or_what_holds_it() {
        let line = |hold, ticks, armed| {
            let mut out = String::from("stale");
            detail(&mut out, (60, 2, "Resonance"), hold, ticks, armed);
            out
        };
        let refused = |r| Some(Hold::Refused(r));
        assert_eq!(line(None, 0, false), "60 Resonance");
        assert_eq!(line(None, 0, true), "Pick a target");
        assert_eq!(line(refused(Refusal::Tier), 0, false), "Needs tier 2");
        assert_eq!(
            line(refused(Refusal::Price), 0, false),
            "Needs 60 Resonance"
        );
        assert_eq!(line(Some(Hold::NoRegiments), 0, false), "Select regiments");
        // Cooldown seconds round up: 281 ticks of 20 a second is 15 s.
        assert_eq!(line(refused(Refusal::Cooling), 281, false), "Ready in 15s");
        assert_eq!(line(refused(Refusal::NoShrine), 0, false), "Needs a Shrine");
        assert_eq!(line(refused(Refusal::Cooling), 1, false), "Ready in 1s");
        assert_eq!(line(refused(Refusal::Cooling), 280, false), "Ready in 14s");
        assert_eq!(line(refused(Refusal::Unknown), 0, false), "Unavailable");
        // The meter is the power's own.
        let mut honor = String::new();
        detail(&mut honor, (25, 1, "Honor"), None, 0, false);
        assert_eq!(honor, "25 Honor");
        detail(
            &mut honor,
            (35, 2, "Honor"),
            Some(Hold::Refused(Refusal::Price)),
            0,
            false,
        );
        assert_eq!(honor, "Needs 35 Honor");
        // Every line fits a button (20 characters).
        for hold in [
            None,
            Some(Hold::NoRegiments),
            refused(Refusal::Tier),
            refused(Refusal::Price),
            refused(Refusal::Cooling),
            refused(Refusal::Unknown),
        ] {
            assert!(line(hold, 1800, false).chars().count() <= 20, "{hold:?}");
        }
    }

    #[test]
    fn pressing_a_held_button_says_why() {
        assert_eq!(
            hold_message(Hold::NoRegiments, 0, "Honor"),
            "Select some regiments first."
        );
        assert_eq!(
            hold_message(Hold::Refused(Refusal::Cooling), 41, "Honor"),
            "That power isn't ready again yet: 3s left."
        );
        assert_eq!(
            hold_message(Hold::Refused(Refusal::Tier), 0, "Honor"),
            "Your tier is too low for that."
        );
        assert_eq!(
            hold_message(Hold::Refused(Refusal::Price), 0, "Resonance"),
            "You haven't the Resonance for that."
        );
        assert_eq!(
            hold_message(Hold::Refused(Refusal::Price), 0, "Honor"),
            "You haven't the Honor for that."
        );
    }

    #[test]
    fn the_prompt_says_what_to_click() {
        let aim = |e| Aim::of(e).unwrap();
        assert_eq!(
            prompt("Overclock", aim(HASTE), ""),
            "Overclock: pick one of your buildings or regiments"
        );
        assert_eq!(
            prompt("Phase Shift", aim(SHIFT), "Resonator"),
            "Phase Shift: pick one of your Resonators"
        );
        assert_eq!(
            prompt("Banner of the Ancestors", aim(BANNER), ""),
            "Banner of the Ancestors: pick a spot on the ground"
        );
    }

    #[test]
    fn a_haste_goes_to_the_regiment_clicked_else_the_building() {
        let (r, b) = (regiment(4), structure(9));
        assert_eq!(
            haste_target(Some(r), Some(b)),
            Some(PowerTarget::Regiment(r))
        );
        assert_eq!(haste_target(Some(r), None), Some(PowerTarget::Regiment(r)));
        assert_eq!(haste_target(None, Some(b)), Some(PowerTarget::Structure(b)));
        assert_eq!(haste_target(None, None), None, "anything else does nothing");
    }

    #[test]
    fn a_shift_is_one_command_for_each_selected_regiment() {
        let picked = [regiment(1), regiment(5), regiment(2)];
        let to = structure(7);
        let commands: Vec<Command> = shifts(1, &picked, to).collect();
        assert_eq!(commands.len(), 3);
        for (c, &regiment) in commands.iter().zip(&picked) {
            assert_eq!(
                *c,
                Command::UsePower {
                    power: 1,
                    target: PowerTarget::Shift { regiment, to }
                }
            );
        }
        assert_eq!(shifts(1, &[], to).count(), 0);
    }

    #[test]
    fn a_grant_is_one_order_on_the_caster_and_a_march_one_for_each_selected_regiment() {
        let picked = [regiment(1), regiment(5), regiment(2)];
        assert_eq!(
            pressed(Aim::Instant, 0, &picked),
            [Command::UsePower {
                power: 0,
                target: PowerTarget::Caster
            }],
            "a grant ignores the selection"
        );
        assert_eq!(pressed(Aim::Instant, 0, &[]).len(), 1);
        let marches = pressed(Aim::Selection, 1, &picked);
        assert_eq!(marches.len(), 3);
        for (c, &h) in marches.iter().zip(&picked) {
            assert_eq!(
                *c,
                Command::UsePower {
                    power: 1,
                    target: PowerTarget::Regiment(h)
                }
            );
        }
        assert!(pressed(Aim::Selection, 1, &[]).is_empty());
        // Aimed powers send nothing on the press.
        for aim in [Aim::Unit, Aim::Building(3), Aim::Ground(12)] {
            assert!(pressed(aim, 2, &picked).is_empty(), "{aim:?}");
        }
    }

    #[test]
    fn a_banner_is_planted_at_the_point_clicked() {
        let at = gc_fixed::FxVec2::from_ints(40, 25);
        assert_eq!(
            planted(2, at),
            Command::UsePower {
                power: 2,
                target: PowerTarget::Point(at)
            }
        );
    }

    #[test]
    fn a_haste_glow_swells_and_fades_within_bounds() {
        let glows: Vec<u8> = (0..GLOW_PERIOD_MS).step_by(10).map(haste_glow).collect();
        assert!(glows.iter().all(|g| (GLOW_MIN..=GLOW_MAX).contains(g)));
        assert_eq!(glows.iter().min(), Some(&GLOW_MIN));
        assert!(*glows.iter().max().unwrap() >= GLOW_MAX - 2);
        assert_eq!(haste_glow(37), haste_glow(37 + 5 * GLOW_PERIOD_MS));
        assert!(haste_glow(GLOW_PERIOD_MS / 2) > haste_glow(GLOW_PERIOD_MS / 8));
    }
}
