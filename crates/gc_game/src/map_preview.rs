//! The pictures of maps the lobby and the Host a game card show.
//!
//! A Field Battle map takes about a third of a second to make (320 tiles,
//! optimised), far too long for one frame. So the menu says which map it
//! wants ([`Menu::wanted_map_preview`]), a worker thread makes it and boils
//! it down to the minimap's colours, and the latest answer is handed back to
//! the menu. Answers for maps nobody is looking at any more are dropped.

use std::sync::mpsc;

use gc_platform::logln;
use gc_ui::hud::MinimapTerrain;
use gc_ui::menu::MapPreviewKey;
use gc_ui::{Menu, Network};
use glam::Vec2;

use crate::hud::minimap_terrain;
use crate::{MapKey, make_map};

/// One map's picture, as the worker makes it.
struct Made {
    key: MapPreviewKey,
    terrain: MinimapTerrain,
    starts: Vec<[f32; 2]>,
}

/// The start plateaus of a `tiles` map, as fractions of its side.
fn starts_of(tiles: u32) -> Vec<[f32; 2]> {
    gc_map::plateaus(tiles)
        .iter()
        .map(|&(x, z)| [x as f32 / tiles as f32, z as f32 / tiles as f32])
        .collect()
}

/// The worker: make each wanted map, newest request first (a request that
/// has been overtaken is skipped). Ends when the game drops its end.
fn work(jobs: mpsc::Receiver<MapPreviewKey>, made: mpsc::Sender<Made>) {
    while let Ok(mut key) = jobs.recv() {
        while let Ok(newer) = jobs.try_recv() {
            key = newer;
        }
        // The last map made is kept, so the match then started on it costs
        // only a copy.
        let map = make_map(MapKey::from(key));
        let starts = match key {
            // (Measured land moves the starts off the plateaus.)
            MapPreviewKey::Field { tiles, .. } if map.starts().len() == gc_map::START_PLATEAUS => {
                let side = tiles as f32;
                map.starts()
                    .iter()
                    .map(|&(x, z)| [x as f32 / side, z as f32 / side])
                    .collect()
            }
            MapPreviewKey::Field { tiles, .. } => starts_of(tiles),
            MapPreviewKey::Conquest { .. } => {
                let side = map.tiles as f32;
                map.start_positions()
                    .iter()
                    .map(|p| ((*p + Vec2::splat(0.5)) / side).to_array())
                    .collect()
            }
        };
        let answer = Made {
            key,
            terrain: minimap_terrain(&map),
            starts,
        };
        if made.send(answer).is_err() {
            return;
        }
    }
}

/// Keeps the menu's map picture up to date. Call [`MapPreviews::poll`] each
/// frame the menu runs.
pub struct MapPreviews {
    /// Requests to the worker; `None` if it could not start or has gone.
    jobs: Option<mpsc::Sender<MapPreviewKey>>,
    made: mpsc::Receiver<Made>,
    /// The map last asked for.
    asked: Option<MapPreviewKey>,
}

impl MapPreviews {
    pub fn new() -> MapPreviews {
        let (jobs, requests) = mpsc::channel();
        let (answer, made) = mpsc::channel();
        let started = std::thread::Builder::new()
            .name("map-preview".into())
            .spawn(move || work(requests, answer));
        if let Err(e) = &started {
            logln!("grubcraft: could not start the map preview: {e}");
        }
        MapPreviews {
            jobs: started.is_ok().then_some(jobs),
            made,
            asked: None,
        }
    }

    /// Ask for the map the menu wants now (after its frame), and give it
    /// the newest picture that is still the one it wants.
    pub fn poll<N: Network>(&mut self, menu: &mut Menu<N>) {
        let wanted = menu.wanted_map_preview();
        if wanted != self.asked {
            self.asked = wanted;
            if let (Some(key), Some(jobs)) = (wanted, &self.jobs)
                && jobs.send(key).is_err()
            {
                logln!("grubcraft: the map preview stopped");
                self.jobs = None;
            }
        }
        while let Ok(m) = self.made.try_recv() {
            if Some(m.key) == self.asked {
                menu.set_map_preview(m.key, m.terrain, m.starts);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_ui::lobby::DEFAULT_MAP_SEED;
    use gc_ui::{Screen, Ui, UiInput};
    use std::time::{Duration, Instant};

    /// One menu frame, then the previews' poll.
    fn step(menu: &mut Menu, ui: &mut Ui, previews: &mut MapPreviews) {
        ui.begin_frame(UiInput::default(), (1280, 760));
        menu.frame(ui);
        ui.end_frame();
        previews.poll(menu);
    }

    /// Frames until the menu shows the picture of `key`.
    fn wait_for(menu: &mut Menu, ui: &mut Ui, previews: &mut MapPreviews, key: MapPreviewKey) {
        let give_up = Instant::now() + Duration::from_secs(120);
        while menu.shown_map_preview() != Some(key) {
            assert!(Instant::now() < give_up, "no picture for {key:?}");
            std::thread::sleep(Duration::from_millis(5));
            step(menu, ui, previews);
        }
    }

    #[test]
    fn starts_are_inside_the_map() {
        for tiles in [256, 320] {
            let starts = starts_of(tiles);
            assert_eq!(starts.len(), gc_map::START_PLATEAUS);
            assert!(starts.iter().all(|s| s.iter().all(|&v| 0.0 < v && v < 1.0)));
        }
    }

    #[test]
    fn the_menu_gets_a_picture_of_the_map_it_wants_and_follows_it() {
        let mut menu = Menu::new(
            gc_ui::LocalNetwork::new(),
            gc_ui::Settings::default(),
            None,
            "test",
        );
        let mut ui = Ui::new();
        let mut previews = MapPreviews::new();
        menu.show(Screen::Play);
        step(&mut menu, &mut ui, &mut previews);
        let field = |seed| MapPreviewKey::Field {
            choice: gc_map::GROWN,
            tiles: 512,
            seed,
        };
        let first = field(DEFAULT_MAP_SEED);
        assert_eq!(menu.wanted_map_preview(), Some(first));
        // Not made yet, so not shown.
        assert_eq!(menu.shown_map_preview(), None);
        wait_for(&mut menu, &mut ui, &mut previews, first);
        // The host card moves to another map; its picture arrives too, and
        // the old one is not shown for it.
        menu.set_host_map_seed(7);
        step(&mut menu, &mut ui, &mut previews);
        assert_eq!(menu.wanted_map_preview(), Some(field(7)));
        assert_eq!(menu.shown_map_preview(), None);
        wait_for(&mut menu, &mut ui, &mut previews, field(7));
        // Leaving for the main menu wants nothing.
        menu.show(Screen::Main);
        step(&mut menu, &mut ui, &mut previews);
        assert_eq!(menu.wanted_map_preview(), None);
    }
}
