//! Menus and lobby screens. Docs: docs/08-ui-ux.md#menus-and-lobby.
//!
//! An immediate-mode UI that knows nothing about the GPU: each frame the
//! game feeds it the mouse and keyboard ([`UiInput`]), a [`Menu`] lays out
//! and draws its current screen into a list of [`Quad`]s, and the renderer
//! draws those. Everything here runs headless, so the screens are unit
//! tested by clicking through them.
//!
//! The network is behind the [`lobby::Network`] trait. Until `gc_net` lands
//! (M2) the game plugs in [`lobby::LocalNetwork`], which lets you host a
//! practice lobby with AI players and says plainly that joining friends is
//! not built yet.

pub mod draw;
pub mod lobby;
pub mod menu;
pub mod settings;
pub mod ui;

pub use draw::{Color, DrawList, Quad, Rect};
pub use lobby::{LocalNetwork, MatchStart, Network};
pub use menu::{Menu, MenuAction, Screen};
pub use settings::Settings;
pub use ui::{Ui, UiInput};
