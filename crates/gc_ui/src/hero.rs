//! The hero panel: portrait, level, hit points, experience, the four
//! abilities' ranks and the three item slots. Docs:
//! docs/08-ui-ux.md#screen-layout-1080p-reference.
//!
//! Display only. The abilities themselves are used from the command grid
//! (keys A, S, D, F), which the game fills from [`HeroPanel::abilities`].
//! Drawn above the bar's left end, over the battlefield.

use crate::draw::{CELL_W, DrawList, Rect, fit, text_width};
use crate::hud::{
    GRID_KEYS, HERO_SLOT_FIRST, dim_ink, draw_portrait, health_color, ink_meter, linen_light,
    muted_ink,
};
use crate::ui::theme;

const WIDTH: i32 = 284;
const HEIGHT: i32 = 118;
const MARGIN: i32 = 8;
const PORTRAIT: i32 = 56;
const ROW_H: i32 = 24;
const ABILITY_W: i32 = 40;
const ITEM_W: i32 = 24;
const GAP: i32 = 3;
const XP_COLOR: [u8; 4] = theme::WOAD;

/// One of a hero's four abilities (three basic and the ultimate).
#[derive(Clone, Copy, Debug, Default)]
pub struct HeroAbility<'a> {
    /// At most 6 characters, as on a command grid button.
    pub short: &'a str,
    /// Ranks learned; 0 is not learned yet.
    pub rank: u8,
    pub max_rank: u8,
    pub passive: bool,
    /// Seconds until it can be used again; 0 is ready.
    pub cooldown_secs: u32,
}

impl HeroAbility<'_> {
    /// Whether the command grid button can be pressed.
    pub fn usable(&self) -> bool {
        self.rank > 0 && !self.passive && self.cooldown_secs == 0
    }

    /// The small line under the ability's name on the grid.
    pub fn status(&self) -> String {
        match (self.rank, self.passive, self.cooldown_secs) {
            (0, _, _) => "locked".to_string(),
            (_, true, _) => "auto".to_string(),
            (_, _, 0) => String::new(),
            (_, _, s) => format!("{s}s"),
        }
    }
}

/// What to show for the selected hero.
#[derive(Clone, Copy, Debug)]
pub struct HeroPanel<'a> {
    pub name: &'a str,
    /// e.g. "Duelist".
    pub role: &'a str,
    /// Team colour of the portrait.
    pub color: [u8; 3],
    pub level: u8,
    pub hp: u32,
    pub max_hp: u32,
    /// Progress to the next level, 0 to 1.
    pub xp: f32,
    pub abilities: [HeroAbility<'a>; 4],
    pub items: [Option<&'a str>; 3],
    /// Seconds until the hero can be revived, when dead.
    pub revive_secs: Option<u32>,
}

/// Where the panel sits on a screen whose bar starts at `bar_top`.
fn panel_rect(bar_top: i32) -> Rect {
    Rect::new(MARGIN, bar_top - MARGIN - HEIGHT, WIDTH, HEIGHT)
}

/// Draw the panel above a bar that starts at `bar_top`, and note its
/// rectangle in `over` so a click on it is not the battlefield's.
pub fn draw(draw: &mut DrawList, bar_top: i32, hero: &HeroPanel, over: &mut Vec<Rect>) {
    let r = panel_rect(bar_top);
    if r.y < 0 {
        return;
    }
    over.push(r);
    draw.shadow(r, 3);
    draw.cloth(r, theme::LINEN);

    let portrait = Rect::new(r.x + 10, r.y + 10, PORTRAIT, PORTRAIT);
    if let Some(secs) = hero.revive_secs {
        draw.rect(portrait, theme::LINEN_DK);
        draw.text_centered(portrait, 1, theme::MADDER, "DEAD");
        let t = format!("{}:{:02}", secs / 60, secs % 60);
        let below = Rect::new(portrait.x, portrait.bottom() - 18, portrait.w, 16);
        draw.text_centered(below, 1, muted_ink(), &t);
    } else {
        let name = hero.name.trim_start_matches("The ");
        draw_portrait(draw, portrait, hero.color, name, 3);
    }
    draw.border(portrait, 2, theme::INK);

    let x = portrait.right() + 6;
    let w = r.right() - 10 - x;
    let chars = (w / CELL_W).max(1) as usize;
    draw.text_bold(x, r.y + 9, 1, theme::INK, &fit(hero.name, chars));
    let sub = format!("Lv {} {}", hero.level, hero.role);
    draw.text(x, r.y + 25, 1, muted_ink(), &fit(&sub, chars));
    let hp_frac = hero.hp as f32 / hero.max_hp.max(1) as f32;
    ink_meter(
        draw,
        Rect::new(x, r.y + 42, w, 10),
        hp_frac,
        health_color(hp_frac),
    );
    let hp = format!("{}/{}", hero.hp, hero.max_hp);
    draw.meter(
        Rect::new(x, r.y + 54, w, 4),
        hero.xp,
        XP_COLOR,
        theme::LINEN_DK,
    );
    draw.text(x, r.y + 58, 1, muted_ink(), &fit(&hp, chars));

    // Ranks and cooldowns, then the three item slots.
    let row_y = r.bottom() - 10 - ROW_H;
    let mut cell_x = r.x + 10;
    for (a, key) in hero
        .abilities
        .iter()
        .zip(&GRID_KEYS[HERO_SLOT_FIRST..HERO_SLOT_FIRST + 4])
    {
        let cell = Rect::new(cell_x, row_y, ABILITY_W, ROW_H);
        let (back, ink) = match (a.rank, a.cooldown_secs) {
            (0, _) => (theme::LINEN, dim_ink()),
            (_, 0) => (theme::LINEN_DK, theme::INK),
            _ => (theme::LINEN_DK, muted_ink()),
        };
        draw.boxed(cell, back, 2);
        let mut k = [0u8; 4];
        draw.text(
            cell.x + 3,
            cell.y + 1,
            1,
            theme::MADDER,
            key.encode_utf8(&mut k),
        );
        // Short enough to clear the key letter: minutes from 100 seconds.
        let line = if a.rank == 0 {
            "-".to_string()
        } else if a.cooldown_secs >= 100 {
            format!("{}m", a.cooldown_secs.div_ceil(60))
        } else if a.cooldown_secs > 0 {
            format!("{}s", a.cooldown_secs)
        } else {
            format!("{}/{}", a.rank, a.max_rank)
        };
        let tw = text_width(&line, 1);
        draw.text(cell.right() - 3 - tw, cell.y + 1, 1, ink, &line);
        cell_x += ABILITY_W + GAP;
    }
    cell_x += GAP * 2;
    for item in hero.items {
        let cell = Rect::new(cell_x, row_y, ITEM_W, ROW_H);
        draw.boxed(cell, linen_light(), 2);
        if let Some(initial) = item.and_then(|n| n.chars().next()) {
            draw.text_centered(cell, 1, theme::MADDER, &initial.to_string());
        }
        cell_x += ITEM_W + GAP;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hero() -> HeroPanel<'static> {
        let ability = |short, rank, passive, cooldown_secs| HeroAbility {
            short,
            rank,
            max_rank: 3,
            passive,
            cooldown_secs,
        };
        HeroPanel {
            name: "The Daimyo",
            role: "Commander",
            color: [200, 60, 50],
            level: 3,
            hp: 212,
            max_hp: 340,
            xp: 0.4,
            abilities: [
                ability("Rally", 2, false, 0),
                ability("Aura", 1, true, 0),
                ability("Chall", 0, false, 0),
                ability("Banner", 0, false, 0),
            ],
            items: [Some("Jade Charm"), None, None],
            revive_secs: None,
        }
    }

    #[test]
    fn the_panel_sits_above_the_bar_and_takes_its_clicks() {
        let (mut d, mut over) = (DrawList::default(), Vec::new());
        draw(&mut d, 592, &hero(), &mut over);
        assert_eq!(over.len(), 1);
        let r = over[0];
        assert!(r.bottom() <= 592 && r.x >= 0 && r.right() <= 300);
        assert!(!d.quads.is_empty());
        // Too short a screen for it: nothing is drawn or claimed.
        let (mut d, mut over) = (DrawList::default(), Vec::new());
        draw(&mut d, 60, &hero(), &mut over);
        assert!(d.quads.is_empty() && over.is_empty());
    }

    #[test]
    fn only_a_learned_active_ability_off_cooldown_is_usable() {
        let a = hero().abilities;
        assert!(a[0].usable());
        assert!(!a[1].usable(), "passive");
        assert!(!a[2].usable(), "not learned");
        let cooling = HeroAbility {
            cooldown_secs: 12,
            ..a[0]
        };
        assert!(!cooling.usable());
        assert_eq!(cooling.status(), "12s");
        assert_eq!(a[1].status(), "auto");
        assert_eq!(a[2].status(), "locked");
        assert_eq!(a[0].status(), "");
    }

    #[test]
    fn a_dead_hero_shows_the_revive_time() {
        let mut h = hero();
        h.hp = 0;
        h.revive_secs = Some(84);
        let (mut d, mut over) = (DrawList::default(), Vec::new());
        draw(&mut d, 592, &h, &mut over);
        // "1:24" is four glyphs, drawn.
        let glyph = |c: char| u32::from(c as u8 - b' ');
        let t: Vec<u32> = "1:24".chars().map(glyph).collect();
        assert!(
            d.quads
                .windows(4)
                .any(|w| w.iter().map(|q| q.glyph).eq(t.iter().copied()))
        );
    }

    #[test]
    fn a_long_cooldown_stays_clear_of_the_key_letter() {
        let mut h = hero();
        h.abilities[0].cooldown_secs = 130;
        let (mut d, mut over) = (DrawList::default(), Vec::new());
        draw(&mut d, 592, &h, &mut over);
        // "3m", not "130s": two glyphs of the text are at most 16px wide.
        let glyph = |c: char| u32::from(c as u8 - b' ');
        let t = [glyph('3'), glyph('m')];
        assert!(
            d.quads
                .windows(2)
                .any(|w| w[0].glyph == t[0] && w[1].glyph == t[1])
        );
    }
}
