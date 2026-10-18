//! Menus and lobby screens. Docs: docs/08-ui-ux.md#menus-and-lobby.
//!
//! An immediate-mode UI that knows nothing about the GPU: each frame the
//! game feeds it the mouse and keyboard ([`UiInput`]), a [`Menu`] lays out
//! and draws its current screen into a list of [`Quad`]s, and the renderer
//! draws those. Everything here runs headless, so the screens are unit
//! tested by clicking through them.
//!
//! The network is behind the [`lobby::Network`] trait. The game plugs in
//! `gc_net::Lan` (LAN play); [`lobby::LocalNetwork`] is a practice lobby on
//! one machine that the menu tests run on.

pub mod alerts;
pub mod conquest;
pub mod draw;
pub mod guide;
pub mod hero;
pub mod hud;
pub mod icons;
pub mod lobby;
pub mod menu;
pub mod placer;
pub mod powers;
pub mod settings;
pub mod ui;
pub mod viewer;

pub use draw::{Color, DrawList, Quad, Rect};
pub use icons::Icon;
pub use lobby::{LocalNetwork, MatchStart, Network};
pub use menu::{Menu, MenuAction, ReplayEntry, Screen};
pub use settings::Settings;
pub use ui::{Ui, UiInput};
