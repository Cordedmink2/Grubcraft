//! Conquest's command card (docs/08-ui-ux.md, "Command card"): what the
//! grid offers for a selected building, for workers and in their build
//! menu, and carrying it out. The orders themselves are the simulation's
//! (`Command::Build`, `Train`, `Upgrade`, `CallOff`, `Untrain`, `Rally`,
//! `Gather`, `Construct`, `Trade` and `Clear`).

use std::fmt::Write;

use gc_data::ConquestUnitRecord;
use gc_data::conquest::UNIT_CONSUMED;
use gc_sim::gates::GateMode;
use gc_sim::structures::{CANCEL_REFUND_PERCENT, Refusal};
use gc_sim::{
    BuildingId, Command, PlayerCommand, PlayerId, RegimentHandle, Regiments, StructureHandle,
    StructureView, TradePost, UnitId, WallRaise,
};
use gc_ui::hud::{GRID_SLOTS, SlotState};
use glam::Vec2;

use crate::hud::Armed;
use crate::{Running, buildings, from_fx_vec, fx_vec, sounds};

/// A worker's Build button: F, the special slot.
pub const SLOT_BUILD: usize = 7;
/// A worker's Clear button (S): fell the woods and grub out scrub.
const SLOT_CLEAR: usize = 5;
/// A worker's Expand button (D): lay a new main building.
const SLOT_EXPAND: usize = 6;
/// A worker's Work plans button (A): build the walls your side has planned.
const SLOT_WORK_PLANS: usize = 4;
/// A building's last slot (V): its tier upgrade, or calling it off.
const SLOT_LAST: usize = GRID_SLOTS - 1;
/// A right-click this close to a seep or vein (in tiles) sends workers there.
const NODE_REACH: f32 = 1.5;
/// A right-click this close to a neutral trade post (in tiles) sends
/// caravans there.
const POST_REACH: f32 = 3.0;

/// What one slot of the card does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the workers' build menu.
    OpenBuild,
    /// Choose ground for the workers to clear.
    Clear,
    /// Set the selected workers to build the walls your side has planned.
    WorkPlans,
    /// Choose where a worker lays this building.
    Lay(BuildingId),
    /// Queue this regiment at the selected building.
    Train(UnitId),
    /// Start the selected building's tier upgrade into this.
    Upgrade(BuildingId),
    /// Raise the selected wall tile's whole joined stretch to this next
    /// level (the sim pays for every tile now).
    RaiseWall(BuildingId),
    /// Call off the selected building or its upgrade.
    CallOff,
    /// Set the selected gate to this mode.
    SetGate(GateMode),
}

/// The grid as Conquest fills it: an action per slot. With `whole`, the card
/// is the whole grid (a building, or the build menu); otherwise its actions
/// go over the battle grid.
#[derive(Clone, Copy, Debug)]
pub struct Card {
    pub slots: [Option<Action>; GRID_SLOTS],
    pub whole: bool,
}

impl Running {
    /// The Conquest card for what is selected, if it has one.
    pub fn conquest_card(&self) -> Option<Card> {
        if !self.conquest_match() || self.watching.is_some() {
            return None;
        }
        let mut slots = [None; GRID_SLOTS];
        let structures = self.world.structures()?;
        if let Some(s) = self.picked_structure().and_then(|h| structures.get(h)) {
            // Your gates take their mode in the first slots (Q, W, E), going
            // up or standing; they train nothing.
            if s.gate.is_some() && s.owner == self.sandbox.side {
                for (slot, mode) in slots.iter_mut().zip(GateMode::ALL) {
                    *slot = Some(Action::SetGate(mode));
                }
            }
            // Your own drawn wall's tiles are raised a stretch at a time, not
            // upgraded one by one; a top-level wall has nothing to offer.
            let own_wall =
                s.owner == self.sandbox.side && s.gate.is_none() && self.is_wall(s.building);
            if own_wall && s.finished() && s.upgrading.is_none() {
                slots[SLOT_LAST] = structures.upgrade_of(s.building).map(Action::RaiseWall);
            } else if !s.finished() || (own_wall && s.upgrading.is_some()) {
                slots[SLOT_LAST] = Some(Action::CallOff);
            } else {
                // It still trains while it upgrades.
                for (slot, u) in slots[..SLOT_LAST].iter_mut().zip(self.trains(s.building)) {
                    *slot = Some(Action::Train(u));
                }
                slots[SLOT_LAST] = match s.upgrading {
                    Some(_) => Some(Action::CallOff),
                    None => structures.upgrade_of(s.building).map(Action::Upgrade),
                };
            }
            return Some(Card { slots, whole: true });
        }
        let worker = self.selected_workers().next()?;
        let faction = self.world.regiments.get(worker).and_then(|r| {
            self.data
                .conquest_units
                .get(usize::from(r.unit))
                .map(ConquestUnitRecord::faction)
        })?;
        if !self.hud.build_menu {
            slots[SLOT_BUILD] = Some(Action::OpenBuild);
            slots[SLOT_CLEAR] = Some(Action::Clear);
            slots[SLOT_EXPAND] = self.buildings.expansion(faction).map(Action::Lay);
            slots[SLOT_WORK_PLANS] = self.has_plans().then_some(Action::WorkPlans);
            return Some(Card {
                slots,
                whole: false,
            });
        }
        for (slot, b) in slots.iter_mut().zip(self.buildings.buildable(faction)) {
            *slot = Some(Action::Lay(b));
        }
        Some(Card { slots, whole: true })
    }

    /// What a building of type `building` trains: what the simulation lets
    /// it, which is what its whole upgrade chain does.
    fn trains(&self, building: BuildingId) -> impl Iterator<Item = UnitId> + '_ {
        let structures = self.world.structures();
        (self.data.conquest_units.iter().enumerate())
            .filter(move |(_, u)| structures.is_some_and(|s| s.grew_from(building, u.trained_at)))
            .map(|(i, _)| i as UnitId)
    }

    /// The selected regiments that are workers.
    pub fn selected_workers(&self) -> impl Iterator<Item = RegimentHandle> + '_ {
        let (regiments, units) = (&self.world.regiments, &self.data.conquest_units);
        (self.selected.iter().copied()).filter(|&h| is_worker(regiments, units, h))
    }

    /// Whether something is selected and all of it is workers.
    pub fn selection_all_workers(&self) -> bool {
        !self.selected.is_empty() && self.selected_workers().count() == self.selected.len()
    }

    /// The selected worker nearest `spot`.
    pub fn nearest_worker(&self, spot: Vec2) -> Option<RegimentHandle> {
        self.nearest_to(self.selected_workers(), spot)
    }

    /// Whichever of the regiments `from` stands nearest `spot`.
    fn nearest_to(
        &self,
        from: impl Iterator<Item = RegimentHandle>,
        spot: Vec2,
    ) -> Option<RegimentHandle> {
        let regiments = &self.world.regiments;
        let dist = |h| {
            regiments
                .position(h)
                .map_or(f32::MAX, |p| from_fx_vec(p).distance(spot))
        };
        from.min_by(|&a, &b| dist(a).total_cmp(&dist(b)))
    }

    /// Whether your side has walls planned (blueprints) to work.
    pub fn has_plans(&self) -> bool {
        let side = self.sandbox.side;
        self.world.plans().iter().any(|p| p.owner == side)
    }

    /// The selected workers go to work your side's plans: each takes the
    /// nearest planned tile whenever it has nothing else to do.
    pub fn work_plans(&mut self) {
        let workers: Vec<RegimentHandle> = self.selected_workers().collect();
        if workers.is_empty() {
            return;
        }
        if !self.has_plans() {
            return self.say("Your side has nothing planned.".into());
        }
        let n = workers.len();
        self.orders(
            workers
                .into_iter()
                .map(|regiment| Command::WorkPlan { regiment }),
        );
        self.say(format!(
            "{n} {} set to your planned walls. Any other order stops {}.",
            buildings::plural(n, "worker", "workers"),
            buildings::plural(n, "it", "them"),
        ));
    }

    /// Whether `building` is a wall drawn as a line.
    pub fn is_wall(&self, building: BuildingId) -> bool {
        self.buildings.is_wall(building)
    }

    /// Open or close the workers' build menu; it opens only for workers.
    /// Opening it puts down an armed order; closing it puts down a building
    /// chosen to lay.
    pub fn set_build_menu(&mut self, open: bool) {
        if open && self.selected_workers().next().is_none() {
            return;
        }
        self.hud.build_menu = open;
        if open || matches!(self.hud.armed, Some(Armed::Lay(_) | Armed::Wall(..))) {
            self.hud.armed = None;
        }
    }

    /// What raising the selected wall's stretch would take, if `action`
    /// raises one: asked of the simulation once, for the check and the label.
    fn wall_raise(&self, action: Action) -> Option<Result<WallRaise, Refusal>> {
        let Action::RaiseWall(_) = action else {
            return None;
        };
        let h = self.picked_structure()?;
        Some(self.world.wall_level(self.sandbox.side, h))
    }

    /// Why `action` can't be done now, if it can't: the simulation's own
    /// check, in the panel's words. `raise` is `wall_raise(action)`.
    fn blocked(
        &self,
        action: Action,
        raise: Option<Result<WallRaise, Refusal>>,
    ) -> Option<&'static str> {
        let side = self.sandbox.side;
        let structure = self.picked_structure();
        match action {
            // A wall can be armed while broke, to plan blueprints.
            Action::Lay(b) => match self.world.may_lay(side, b) {
                Err(Refusal::Price) if self.is_wall(b) => None,
                r => r.err(),
            },
            Action::Train(u) => structure.and_then(|h| self.world.can_train(side, h, u).err()),
            Action::Upgrade(_) => structure.and_then(|h| self.world.can_upgrade(side, h).err()),
            Action::RaiseWall(_) => match raise {
                Some(Err(e)) => Some(e),
                Some(Ok(w)) if !w.afford => Some(Refusal::Price),
                _ => None,
            },
            Action::OpenBuild
            | Action::Clear
            | Action::WorkPlans
            | Action::CallOff
            | Action::SetGate(_) => None,
        }
        .map(buildings::why)
    }

    /// Each slot of `card` as the grid shows it: its name into `labels`,
    /// its price into `details`, and whether it can be pressed.
    pub fn describe_card(
        &self,
        card: &Card,
        labels: &mut [String; GRID_SLOTS],
        details: &mut [String; GRID_SLOTS],
    ) -> [SlotState; GRID_SLOTS] {
        let mut states = [SlotState::Off; GRID_SLOTS];
        let raise = (card.slots.iter().flatten()).find_map(|&a| self.wall_raise(a));
        let gate_mode = self.gate_mode();
        for (i, action) in card.slots.iter().enumerate() {
            let (label, detail) = (&mut labels[i], &mut details[i]);
            label.clear();
            detail.clear();
            let Some(action) = *action else {
                continue;
            };
            match action {
                Action::OpenBuild => {
                    label.push_str("Build");
                    detail.push('B');
                }
                Action::Clear => {
                    label.push_str("Clear");
                    detail.push('S');
                }
                Action::WorkPlans => {
                    label.push_str("Plans");
                    detail.push('A');
                }
                Action::Lay(b) => {
                    if let Some(s) = self.buildings.stats.get(usize::from(b)) {
                        price(detail, s.cost_grub, s.cost_marrow);
                    }
                    label.push_str(short(self.buildings.name(b)));
                }
                Action::Upgrade(b) => {
                    if let Some(s) = self.buildings.stats.get(usize::from(b)) {
                        price(detail, s.cost_grub, s.cost_marrow);
                        let _ = write!(label, "Tier {}", s.tier);
                    }
                }
                Action::RaiseWall(_) => {
                    // The whole stretch is paid at once, so its full price
                    // shows even when the side can't afford it yet.
                    if let Some(Ok(w)) = raise {
                        price(detail, w.grub, w.marrow);
                    }
                    label.push_str("Raise");
                }
                Action::Train(u) => {
                    if let Some(r) = self.data.conquest_units.get(usize::from(u)) {
                        price(detail, r.cost_grub, r.cost_marrow);
                    }
                    label.push_str(short(
                        self.units
                            .names
                            .get(usize::from(u))
                            .map_or("", String::as_str),
                    ));
                }
                Action::CallOff => {
                    label.push_str("Cancel");
                    let _ = write!(detail, "{CANCEL_REFUND_PERCENT}%");
                }
                Action::SetGate(mode) => label.push_str(gate_name(mode)),
            }
            let action_raise = raise.filter(|_| matches!(action, Action::RaiseWall(_)));
            let armed = match action {
                Action::Lay(b) => {
                    matches!(self.hud.armed, Some(Armed::Lay(a) | Armed::Wall(a, _)) if a == b)
                }
                Action::Clear => self.hud.armed == Some(Armed::Clear),
                // The mode it is set to now.
                Action::SetGate(mode) => gate_mode == Some(mode),
                _ => false,
            };
            states[i] = if armed {
                SlotState::Armed
            } else if self.blocked(action, action_raise).is_some() {
                SlotState::Blocked
            } else {
                SlotState::Ready
            };
        }
        states
    }

    /// Carry out the card's slot `slot`; false if the card has nothing there
    /// and the battle grid should have it.
    pub fn card_press(&mut self, slot: usize) -> bool {
        let Some(card) = self.conquest_card() else {
            return false;
        };
        let Some(action) = card.slots[slot] else {
            return card.whole;
        };
        if let Some(why) = self.blocked(action, self.wall_raise(action)) {
            self.say(why.into());
            return true;
        }
        match action {
            Action::OpenBuild => {
                self.set_build_menu(true);
                return true;
            }
            Action::Clear => {
                // Pressing it again puts it down.
                if self.hud.armed == Some(Armed::Clear) {
                    self.hud.armed = None;
                } else {
                    self.hud.armed = Some(Armed::Clear);
                    self.say("Drag over the ground to clear.".into());
                }
                return true;
            }
            Action::WorkPlans => {
                self.work_plans();
                return true;
            }
            Action::Lay(b) if self.is_wall(b) => {
                // Pressing it again puts it down.
                if matches!(self.hud.armed, Some(Armed::Wall(a, _)) if a == b) {
                    self.hud.armed = None;
                } else {
                    self.arm_wall(b);
                    self.say(format!(
                        "Click where the {} starts, then where it ends or bends",
                        self.buildings.name(b)
                    ));
                }
                return true;
            }
            Action::Lay(b) => {
                // Pressing it again puts it down.
                if self.hud.armed == Some(Armed::Lay(b)) {
                    self.hud.armed = None;
                } else {
                    self.hud.armed = Some(Armed::Lay(b));
                    self.say(format!(
                        "Where should the {} go? R or the wheel turns it",
                        self.buildings.name(b)
                    ));
                }
                return true;
            }
            Action::RaiseWall(_) => {
                if let Some(structure) = self.picked_structure() {
                    self.raise_wall(structure);
                }
                return true;
            }
            Action::Train(_) | Action::Upgrade(_) | Action::CallOff | Action::SetGate(_) => {}
        }
        // The rest are the selected building's.
        let Some(structure) = self.picked_structure() else {
            return true;
        };
        self.order(match action {
            Action::SetGate(mode) => Command::SetGate { structure, mode },
            Action::Train(unit) => Command::Train { structure, unit },
            Action::Upgrade(_) => Command::Upgrade { structure },
            _ => Command::CallOff { structure },
        });
        true
    }

    /// Raise the stretch of wall joined to tile `structure` a level, naming
    /// your nearest idle worker to go and work along it. A grown wall grows
    /// by itself, so it names none.
    fn raise_wall(&mut self, structure: StructureHandle) {
        let side = self.sandbox.side;
        let Some(structures) = self.world.structures() else {
            return;
        };
        let Some(s) = structures.get(structure) else {
            return;
        };
        let spot = buildings::centre_of(s.tile, s.width, s.depth);
        let by_hand = structures.raised_by_hand(s.building);
        let regiment = if by_hand {
            self.nearest_idle_worker(side, spot)
        } else {
            None
        };
        self.order(Command::RaiseWall {
            structure,
            regiment,
        });
        if by_hand && regiment.is_none() {
            self.say("Paid: send a worker to raise it (right-click the wall).".into());
        }
    }

    /// `side`'s idle worker nearest `spot`, if it has one.
    fn nearest_idle_worker(&self, side: PlayerId, spot: Vec2) -> Option<RegimentHandle> {
        self.nearest_to(self.world.idle_workers(side), spot)
    }

    /// The `U` key: raises the selected wall if that is what the card
    /// offers; false if it has nothing to raise (U then opens the guide).
    pub fn raise_key(&mut self) -> bool {
        let raises = matches!(
            self.conquest_card().map(|c| c.slots[SLOT_LAST]),
            Some(Some(Action::RaiseWall(_)))
        );
        raises && self.card_press(SLOT_LAST)
    }

    /// The selected building takes the last regiment off its queue.
    pub fn untrain(&mut self) {
        if let Some(structure) = self.picked_structure()
            && self.watching.is_none()
        {
            self.order(Command::Untrain { structure });
        }
    }

    /// The click after choosing a building to lay: the selected worker
    /// nearest the spot lays it there. Shift keeps the building chosen.
    pub fn finish_laying(&mut self, building: BuildingId) {
        let Some((stats, tile, why)) = self.building_placement(building) else {
            return;
        };
        if let Some(why) = why {
            self.say(why.into());
            return;
        }
        let turn = self.sandbox.building_turn;
        let (width, depth) = stats.size(turn);
        let spot = buildings::centre_of(tile, width, depth);
        let Some(regiment) = self.nearest_worker(spot) else {
            return;
        };
        self.order(Command::Build {
            regiment,
            building,
            tile: (tile.0 as i16, tile.1 as i16),
            turn,
        });
        if !self.input.shift {
            self.set_build_menu(false);
        }
    }

    /// How the selected building is set, if it is a gate.
    pub fn gate_mode(&self) -> Option<GateMode> {
        self.world.structures()?.get(self.picked_structure()?)?.gate
    }

    /// Whether `s` is a finished building of yours that shoots.
    fn own_tower(&self, s: &StructureView) -> bool {
        s.owner == self.sandbox.side
            && s.finished()
            && (self.buildings.stats.get(usize::from(s.building)))
                .is_some_and(|b| b.attack.is_some())
    }

    /// The regiment your tower `h` was told to shoot first, if any.
    pub fn tower_focus(&self, h: StructureHandle) -> Option<RegimentHandle> {
        self.world.structures()?.focus_of(h)
    }

    /// A right-click with one of your finished towers selected: it shoots
    /// the enemy regiment clicked (`body` if one was hit, else the one at
    /// `at`) first, or chooses for itself again if the click was on open
    /// ground. Returns whether the tower took it.
    pub fn tower_right_click(&mut self, body: Option<RegimentHandle>, at: Option<Vec2>) -> bool {
        if !self.conquest_match() || self.watching.is_some() {
            return false;
        }
        let Some(structure) = self.picked_structure() else {
            return false;
        };
        if !(self.world.structures().and_then(|s| s.get(structure)))
            .is_some_and(|s| self.own_tower(&s))
        {
            return false;
        }
        let side = self.sandbox.side;
        let target = body
            .filter(|&h| {
                (self.world.regiments.get(h)).is_some_and(|r| self.world.hostile(side, r.owner))
                    && self.world.sees(side, h)
            })
            .or_else(|| at.and_then(|p| self.enemy_at(p)));
        if target.is_some() {
            self.attack_mark = target.map(|h| (h, std::time::Instant::now()));
            self.ui_sound(sounds::UiSound::Attack);
        }
        self.order(Command::FocusTower { structure, target });
        true
    }

    /// A right-click in Conquest that means more than a move: the rally
    /// point of a selected building that trains, or workers (and only
    /// workers) sent to a seep, a vein or one of your buildings. Returns
    /// whether it was one.
    pub fn conquest_right_click(&mut self, at: Vec2) -> bool {
        if !self.conquest_match() || self.watching.is_some() {
            return false;
        }
        if self.plan_right_click() {
            return true;
        }
        if let Some(structure) = self.picked_structure() {
            let kind = self.world.structures().and_then(|s| s.get(structure));
            if kind.is_some_and(|s| self.trains(s.building).next().is_some()) {
                self.order(Command::Rally {
                    structure,
                    at: fx_vec(at),
                });
            }
            return true;
        }
        if self.send_trading(at) {
            return true;
        }
        // A mixed selection moves together, and an enemy there is attacked.
        let (regiments, units) = (&self.world.regiments, &self.data.conquest_units);
        if !self.selection_all_workers() || self.enemy_at(at).is_some() {
            return false;
        }
        let side = self.sandbox.side;
        // Workers used up by what they build (the Brood's) can only start a
        // foundation nothing has started, and one is enough.
        let spent = (self.selected.iter()).any(|&h| has_flag(regiments, units, h, UNIT_CONSUMED));
        let mend = self.building_under_cursor(|o| o == side).filter(|&h| {
            self.world
                .structures()
                .and_then(|s| s.get(h))
                .is_some_and(|s| {
                    if spent {
                        !s.finished() && !s.growing
                    } else {
                        needs_hands(s)
                    }
                })
        });
        enum Job {
            Mend(StructureHandle),
            Gather(u16),
        }
        // Workers that can't gather there (the Choir's only build; the
        // Concord's have nowhere to take Grub) just walk to the node, with
        // any others, who gather.
        let gathers = |node| (self.selected.iter()).any(|&h| self.world.can_gather(h, node));
        let job = match (mend, self.node_near(at).filter(|&n| gathers(n))) {
            (Some(structure), _) => Job::Mend(structure),
            (None, Some(node)) => Job::Gather(node),
            (None, None) => return false,
        };
        let work = |regiment| match job {
            Job::Mend(structure) => Command::Construct {
                regiment,
                structure,
            },
            Job::Gather(node) if !self.world.can_gather(regiment, node) => Command::MoveTo {
                regiment,
                target: crate::fx_vec(at),
                facing: None,
                files: 0,
                pace: gc_fixed::Fx::ZERO,
            },
            Job::Gather(node) => Command::Gather { regiment, node },
        };
        let crew = match job {
            Job::Mend(_) if spent => 1,
            _ => self.selected.len(),
        };
        self.human_orders.extend(
            self.selected
                .iter()
                .take(crew)
                .map(|&regiment| PlayerCommand {
                    player: side,
                    command: work(regiment),
                }),
        );
        self.ui_sound(sounds::UiSound::Move);
        true
    }

    /// A right-click on a tile your side has planned: with Ctrl (or Cmd)
    /// it drops the whole planned line that tile is on; otherwise, if the
    /// selection is all workers, they go to work the plans. Whether it was
    /// one.
    fn plan_right_click(&mut self) -> bool {
        let Some(line) = self.plan_under_cursor() else {
            return false;
        };
        if self.input.command {
            let [x0, y0, x1, y1] = line;
            self.order(Command::DropPlan {
                from: (x0, y0),
                to: (x1, y1),
            });
            self.say("Dropped the planned wall.".into());
            return true;
        }
        if !self.selection_all_workers() {
            return false;
        }
        self.work_plans();
        true
    }

    /// Caravans among the selection go to trade at the post at `at`: a
    /// building under the mouse that is one for them (their own Waystation,
    /// an ally's main building) or a neutral post by it. Whether it did.
    fn send_trading(&mut self, at: Vec2) -> bool {
        let traders: Vec<RegimentHandle> = (self.selected.iter().copied())
            .filter(|&h| self.world.trades(h))
            .collect();
        if traders.is_empty() || self.enemy_at(at).is_some() {
            return false;
        }
        let side = self.sandbox.side;
        let world = &self.world;
        let building = self
            .building_under_cursor(|_| true)
            .map(TradePost::Structure);
        let hamlet = || {
            world
                .trade_posts()
                .iter()
                .enumerate()
                .map(|(i, p)| (i as u16, from_fx_vec(*p).distance(at)))
                .filter(|&(_, d)| d <= POST_REACH)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| TradePost::Hamlet(i))
        };
        let Some(post) = building
            .filter(|&p| world.is_trade_post(side, p))
            .or_else(hamlet)
            .filter(|&p| world.is_trade_post(side, p))
        else {
            return false;
        };
        self.orders(
            traders
                .into_iter()
                .map(|regiment| Command::Trade { regiment, post }),
        );
        true
    }

    /// The seep or vein nearest `at`, if one is within reach.
    fn node_near(&self, at: Vec2) -> Option<u16> {
        self.world
            .economy()?
            .nodes()
            .map(|n| (n.index, from_fx_vec(n.at).distance(at)))
            .filter(|&(_, d)| d <= NODE_REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Give `command` as the side you command.
    fn order(&mut self, command: Command) {
        self.orders([command]);
    }

    /// Give `commands` as the side you command, all in the same tick.
    pub(crate) fn orders(&mut self, commands: impl IntoIterator<Item = Command>) {
        let player = self.sandbox.side;
        (self.human_orders).extend(
            commands
                .into_iter()
                .map(|command| PlayerCommand { player, command }),
        );
        self.ui_sound(sounds::UiSound::Move);
    }
}

/// Whether regiment `h` is a worker.
fn is_worker(regiments: &Regiments, units: &[ConquestUnitRecord], h: RegimentHandle) -> bool {
    regiments.get(h).is_some_and(|r| {
        units
            .get(usize::from(r.unit))
            .is_some_and(ConquestUnitRecord::worker)
    })
}

/// Whether regiment `h`'s unit has the `UNIT_*` flag `flag`.
fn has_flag(
    regiments: &Regiments,
    units: &[ConquestUnitRecord],
    h: RegimentHandle,
    flag: u16,
) -> bool {
    regiments.get(h).is_some_and(|r| {
        units
            .get(usize::from(r.unit))
            .is_some_and(|u| u.flags & flag != 0)
    })
}

/// A gate mode's name on the card.
pub fn gate_name(mode: GateMode) -> &'static str {
    match mode {
        GateMode::Auto => "Auto",
        GateMode::Locked => "Locked",
        GateMode::Open => "Open",
    }
}

/// What a gate's mode means, for its card and tooltips.
pub fn gate_line(mode: GateMode) -> &'static str {
    match mode {
        GateMode::Auto => "Gate: Auto - opens for your side as they march up",
        GateMode::Locked => "Gate: Locked - shut to everyone",
        GateMode::Open => "Gate: Open - anyone can pass",
    }
}

/// A grid slot's price: Grub, and Marrow after a slash if it costs any.
fn price(detail: &mut String, grub: impl Into<u32>, marrow: impl Into<u32>) {
    let (grub, marrow) = (grub.into(), marrow.into());
    let _ = match marrow {
        0 => write!(detail, "{grub}"),
        m => write!(detail, "{grub}/{m}"),
    };
}

/// A building still going up, or damaged: workers have work there.
fn needs_hands(s: StructureView) -> bool {
    !s.finished() || s.hp < s.max_hp
}

/// A name short enough for a grid slot: its first word.
fn short(name: &str) -> &str {
    name.split(' ').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_names_are_first_words() {
        assert_eq!(short("Yumi Ashigaru"), "Yumi");
        assert_eq!(short("Hands"), "Hands");
        assert_eq!(short(""), "");
    }

    #[test]
    fn only_a_wall_with_a_next_level_can_be_raised() {
        let data = crate::data::load();
        let table = crate::buildings::Table::new(&data);
        let id = |name: &str| {
            data.buildings
                .iter()
                .position(|b| b.name() == name)
                .unwrap_or_else(|| panic!("no {name}")) as BuildingId
        };
        let next = |b: BuildingId| {
            (0..table.len() as BuildingId)
                .find(|&n| table.stats[usize::from(n)].upgrades_from == Some(b))
        };
        // The card offers Raise on a wall that has a next level; the top
        // level and a plain building have none.
        let has_next_level = |name: &str| {
            let b = id(name);
            table.is_wall(b) && next(b).is_some()
        };
        assert!(has_next_level("Palisade") && has_next_level("Stone Wall"));
        assert!(!has_next_level("Castle Wall") && !has_next_level("Clan Hall"));
        assert_eq!(next(id("Palisade")), Some(id("Stone Wall")));
        assert!(!table.is_wall(id("Clan Hall")));
    }
}
