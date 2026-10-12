//! Player settings, saved as RON in the user's config folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::lobby::Faction;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphicsPreset {
    Battery,
    Balanced,
    High,
}

impl GraphicsPreset {
    pub const ALL: [GraphicsPreset; 3] = [
        GraphicsPreset::Battery,
        GraphicsPreset::Balanced,
        GraphicsPreset::High,
    ];

    pub fn name(self) -> &'static str {
        match self {
            GraphicsPreset::Battery => "Battery",
            GraphicsPreset::Balanced => "Balanced",
            GraphicsPreset::High => "High",
        }
    }
}

/// UI scale steps in percent, as docs/08-ui-ux.md#settings asks (75 to 150).
pub const UI_SCALES: [u16; 6] = [75, 90, 100, 115, 125, 150];
/// Frame caps; 0 means uncapped.
pub const FRAME_CAPS: [u16; 4] = [30, 60, 120, 0];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub player_name: String,
    /// Index into the lobby's colour list.
    pub player_color: u8,
    pub faction: Faction,
    pub graphics: GraphicsPreset,
    pub ui_scale: u16,
    pub vsync: bool,
    pub frame_cap: u16,
    pub volume_master: u8,
    pub volume_music: u8,
    pub volume_effects: u8,
    pub volume_voices: u8,
    pub volume_ui: u8,
    pub colorblind_palette: bool,
    pub high_contrast_banners: bool,
    pub larger_text: bool,
    pub reduced_shake: bool,
    pub hints: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            player_name: "Player".into(),
            player_color: 0,
            faction: Faction::EmberClans,
            graphics: GraphicsPreset::Balanced,
            ui_scale: 100,
            vsync: true,
            frame_cap: 60,
            volume_master: 80,
            volume_music: 60,
            volume_effects: 80,
            volume_voices: 80,
            volume_ui: 60,
            colorblind_palette: false,
            high_contrast_banners: false,
            larger_text: false,
            reduced_shake: false,
            hints: true,
        }
    }
}

impl Settings {
    /// The UI scale to draw at, with Larger text counted in.
    pub fn effective_ui_scale(&self) -> f32 {
        let base = self.ui_scale.clamp(75, 150) as f32 / 100.0;
        if self.larger_text { base * 1.25 } else { base }
    }

    /// Fix anything out of range (hand-edited files).
    pub fn sanitize(&mut self) {
        self.player_name = self
            .player_name
            .chars()
            .filter(|c| (' '..='~').contains(c))
            .take(16)
            .collect();
        if self.player_name.trim().is_empty() {
            self.player_name = "Player".into();
        }
        self.player_color %= 4;
        self.ui_scale = self.ui_scale.clamp(75, 150);
        for v in [
            &mut self.volume_master,
            &mut self.volume_music,
            &mut self.volume_effects,
            &mut self.volume_voices,
            &mut self.volume_ui,
        ] {
            *v = (*v).min(100);
        }
    }

    /// Read settings; a missing or broken file gives the defaults.
    pub fn load(path: &Path) -> Settings {
        let mut s = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| ron::from_str::<Settings>(&text).ok())
            .unwrap_or_default();
        s.sanitize();
        s
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

/// Where settings live: `GRUBCRAFT_SETTINGS` if set, else the platform's
/// per-user config folder.
pub fn settings_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GRUBCRAFT_SETTINGS") {
        return Some(PathBuf::from(p));
    }
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        var("HOME").map(|h| h.join("Library/Application Support/Grubcraft/settings.ron"))
    } else if cfg!(windows) {
        var("APPDATA").map(|h| h.join("Grubcraft").join("settings.ron"))
    } else {
        var("XDG_CONFIG_HOME")
            .or_else(|| var("HOME").map(|h| h.join(".config")))
            .map(|h| h.join("grubcraft").join("settings.ron"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("gc_ui_{name}_{}", std::process::id()))
    }

    #[test]
    fn round_trips_through_a_file() {
        let path = temp("rt").join("settings.ron");
        let s = Settings {
            player_name: "Mario".into(),
            ui_scale: 125,
            faction: Faction::Brood,
            ..Default::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_or_broken_files_give_defaults() {
        assert_eq!(Settings::load(&temp("missing")), Settings::default());
        let path = temp("broken");
        std::fs::write(&path, "this is not ron").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn older_files_missing_new_fields_still_load() {
        let path = temp("old");
        std::fs::write(&path, "(player_name: \"Alex\")").unwrap();
        let s = Settings::load(&path);
        assert_eq!(s.player_name, "Alex");
        assert_eq!(s.ui_scale, 100);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn sanitize_fixes_hand_edits() {
        let mut s = Settings {
            player_name: "   ".into(),
            ui_scale: 900,
            volume_master: 250,
            player_color: 9,
            ..Default::default()
        };
        s.sanitize();
        assert_eq!(s.player_name, "Player");
        assert_eq!(s.ui_scale, 150);
        assert_eq!(s.volume_master, 100);
        assert_eq!(s.player_color, 1);
    }

    #[test]
    fn larger_text_scales_the_ui() {
        let s = Settings {
            larger_text: true,
            ..Default::default()
        };
        assert!((s.effective_ui_scale() - 1.25).abs() < 1e-6);
    }
}
