//! T11: the pause menu's Mods page, a mod's part of `StartMenu` drawn from its tiles
//! (issue #976) [design: ui-tile-tree.md §7, owner-review.md #34: NVForever's column
//! stays; a "Mods" entry joins it only when a mod added one, and opens the mod's page
//! from tiles; docs/handoff/ui-tile-tree-tasks.md, T11].
//!
//! - **The switch**: `[UI] TileMenus=StartMenu` (`super::vanilla::TileMenus::start`), on
//!   by default (`-StartMenu` in the list turns it off; `super::vanilla`). With it on and a
//!   mod's tiles in `StartMenu` (MCM's include, menu-xml.md §10;
//!   `fnv_ui::Ui::has_mod_added_tiles`), the column has a Mods entry
//!   (`crate::pause::ModsEntry`, [`available`]). `StartMenu` itself is built from the files
//!   whenever the pause menu is open (T3b, `fnv_game::ui_loading::sync`).
//! - **The page** (`crate::pause::Page::Mods`): the mod's tiles of `StartMenu` are drawn
//!   over the pause menu (`super::draw`, [`MODS_Z`]; only the tiles mods added, as T5 draws
//!   the HUD's: a loose replacement of the start menu's file draws its own tiles, not the
//!   archived file's), in interface units; T8's pointer and keys go to `StartMenu`
//!   (`super::input`); and the start menu's MenuMode blocks run each frame
//!   (`Begin MenuMode 1013`, [`tick`]), as they do under the original's start menu, so a
//!   mod's script fills in its page. Esc or Back leave it.
//! - **The original's pause column, for MCM** ([`prepare_column`]): MCM shows its trigger
//!   only while `StartMenu/NOGLOW_BRANCH/main_container` reports `childcount` 8 and places
//!   it from that column. The original's code fills the column with six rows of the file's
//!   `lb_item_template` and writes `childcount` 8, `_alpha` 255 and `_enabled` 1 [observed:
//!   menu-xml.md §10.1]; NVForever's column is hand-built, so the page writes those traits
//!   and makes one code row (the one MCM reads) when it opens. `_enabled` is MCM's after
//!   that (it turns the column off while its window is open).
//! - **MCM opens with the page** ([`tick`]): a click on MCM's trigger (its `_clicked`, one
//!   frame) once the trigger has been placed, as the player's click on the original's
//!   "Mod Configuration" row; MCM's window closing itself (its Back, its trigger: `_Menu`
//!   back to 0) closes the page ([`ModsPage::closed`]) [design].
use bevy::prelude::info;
use fnv_game::game::menus;
use fnv_ui::Ui;
use std::collections::HashSet;

/// The start menu's name [documented: start_menu.xml].
pub const START_MENU: &str = "StartMenu";

/// The Mods page's stacking level: over the pause menu's (110 to 113, `crate::pause`)
/// [design].
pub const MODS_Z: i32 = 115;

/// The archived start menu file's tile names, kept per file path (`super::draw`).
pub type KeptNames = Option<(String, Option<HashSet<String>>)>;

/// Whether the column has a Mods entry: the switch on and a tile in `StartMenu` that a mod
/// adds, as the HUD's mod tiles are told apart (`Ui::mod_tile_drawn`): injected, a
/// template's, a loose UIO include's, or a tile of a loose replacement that the archived
/// file does not have. A loose `start_menu.xml` that only reskins the menu adds none
/// [design]. `files` reads the archived file's names, once per path into `kept`.
pub fn available(switch: super::vanilla::TileMenus, ui: &Ui, files: Option<&dyn fnv_ui::Source>, kept: &mut KeptNames) -> bool {
    if !switch.start {
        return false;
    }
    let Some(menu) = ui.menu(START_MENU).filter(|&m| ui.has_mod_added_tiles(m)) else { return false };
    let vanilla = files.and_then(|f| super::draw::archived_names(ui, menu, f, kept));
    let mut stack = ui.mod_added_tiles(menu);
    while let Some(t) = stack.pop() {
        if !ui.tile(t).live {
            continue;
        }
        if ui.mod_tile_drawn(t, vanilla) {
            return true;
        }
        stack.extend(ui.tile(t).children.iter().copied());
    }
    false
}

/// The start menu's column of the original, the one MCM reads [observed: menu-xml.md §10.1].
pub const COLUMN: &str = "StartMenu/NOGLOW_BRANCH/main_container";
/// MCM's own rect and its trigger [observed: MCM's `MCM.xml`, menu-xml.md §10].
pub const MCM: &str = "StartMenu/MCM";
pub const MCM_TRIGGER: &str = "StartMenu/MCM/MCM_Trigger";
/// The frame of the page MCM's trigger is clicked on: after MCM's first frames have placed
/// it (its install checks run on the first) [design].
const CLICK_AT: u64 = 3;
/// The most frames the click is held for [design].
const CLICK_FOR: u64 = 60;

/// The Mods page's state over its frames (`crate::script` keeps it; `crate::pause` reads
/// [`ModsPage::closed`]).
#[derive(bevy::prelude::Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct ModsPage {
    /// Frames of the page so far (0 before its first).
    pub frames: u64,
    /// MCM's window was seen open on this page.
    pub mcm_open: bool,
    /// MCM's window closed itself after being open: the page goes back to the column.
    pub closed: bool,
    /// MCM's trigger is held clicked, until its window opens.
    pub clicking: bool,
}

/// The original's column as MCM reads it, written into `StartMenu` when it was (re)built
/// (with the menu's `alpha` 255, its fade-in done): `childcount` 8, `_alpha` 255 and `_enabled` 1 on [`COLUMN`], and one code row of the
/// file's `lb_item_template` (`lb_item_hotrect`, `target` 1) under it [observed: menu-xml.md
/// §10.1, the pause menu's column; design: one row stands for the six, MCM reads only the
/// first's `target`]. Whether it wrote anything (a fresh tree).
pub fn prepare_column(ui: &mut Ui, files: Option<&dyn fnv_ui::Source>) -> bool {
    use fnv_ui::value::Value;
    let Some(column) = ui.find_tile(COLUMN) else { return false };
    if ui.tile(column).get("childcount").is_some() {
        return false;
    }
    // The menu faded in, as the code does for a menu it shows (its file starts it at 0;
    // `super::vats_menu::write` likewise) [design].
    if let Some(menu) = ui.menu(START_MENU) {
        ui.set_trait(menu, "alpha", Value::Num(255.0));
    }
    ui.set_trait(column, "childcount", Value::Num(8.0));
    ui.set_trait(column, "_alpha", Value::Num(255.0));
    ui.set_trait(column, "_enabled", Value::Num(1.0));
    if ui.find_tile(&format!("{COLUMN}/lb_item_hotrect")).is_none() {
        let empty = fnv_ui::MemSource::default();
        if let Some(row) = ui.add_code_tile_from_template(files.unwrap_or(&empty), COLUMN, "lb_item_template", None) {
            ui.set_trait(row, "target", Value::Num(1.0));
        }
    }
    true
}

/// One frame of the start menu's scripts under the Mods page: the column made ready
/// ([`prepare_column`]), MCM's trigger clicked at [`CLICK_AT`] if its window is not open,
/// then only the `MenuMode 1013` blocks run (`Game::page_tick`; no GameMode block, no game
/// time) and the open menu is left alone, so a box a block shows stays open [design: the
/// original runs them under its start menu, as MCM's mods expect, menu-xml.md §10;
/// NVForever's pause menu otherwise holds every script, `crate::script`]. The caller puts
/// the played world in the game first (`crate::script::with_world`).
pub fn tick(game: &mut fnv_game::Game<'_>, page: &mut ModsPage) {
    if page.frames == 0 {
        *page = ModsPage::default();
        // JIP's menu-open handlers for the start menu, as the original's pause menu opens
        // (`Game::page_menu_event`); MCM Extender adds its menus to MCM's list there.
        let ran = game.page_menu_event(menus::START, true);
        let listed = game.data.by_edid("MCMMods").map(|l| fnv_game::functions::list_members(game, l).len());
        info!("tiles: the start menu's opening sent to {ran} JIP handlers; MCM's list of mods (MCMMods) holds {listed:?}");
    }
    let files = game.ui_files.as_deref().map(|f| f as &dyn fnv_ui::Source);
    if prepare_column(&mut game.ui_tree, files) {
        info!("tiles: {COLUMN}: the original's column traits written for MCM");
    }
    let ui = &mut game.ui_tree;
    let open = |ui: &Ui| ui.get_float(&format!("{MCM}/_Menu")) == Some(1.0);
    // The click is held until MCM's window is open, at most [`CLICK_FOR`] frames: MCM skips
    // a click in the frame after it reset itself (its `_UnClick`) [observed: MCM's script
    // source; the original opened with `_clicked` left at 1, menu-xml.md §10.1].
    let clicked = format!("{MCM_TRIGGER}/_clicked");
    if page.frames == CLICK_AT && ui.find_tile(MCM_TRIGGER).is_some() && !open(ui) {
        ui.set_float(&clicked, 1.0);
        page.clicking = true;
        info!("tiles: MCM's trigger clicked: its window opens with the Mods page");
    } else if page.clicking && (open(ui) || page.frames >= CLICK_AT + CLICK_FOR) {
        if !open(ui) {
            let read = |p: &str| ui.get_float(p).map_or("none".to_string(), |v| v.to_string());
            bevy::prelude::warn!(
                "tiles: MCM's window did not open: MCM visible {}, its trigger visible {}, {COLUMN} childcount {}",
                read(&format!("{MCM}/visible")),
                read(&format!("{MCM_TRIGGER}/visible")),
                read(&format!("{COLUMN}/childcount"))
            );
        }
        ui.set_float(&clicked, 0.0);
        page.clicking = false;
    }
    game.page_tick(menus::START);
    // Let go as soon as the window is open: a click held into the next frame is a second
    // click on the trigger, which closes MCM again (its script: `iMenu` 1 and the trigger
    // closes; the close resets its mod list's quest) [observed: MCM's script source].
    if page.clicking && open(&game.ui_tree) {
        game.ui_tree.set_float(&clicked, 0.0);
        page.clicking = false;
    }
    if open(&game.ui_tree) {
        if !page.mcm_open {
            info!("tiles: MCM's window is open on the Mods page");
        }
        page.mcm_open = true;
    } else if page.mcm_open && !page.closed {
        page.closed = true;
        info!("tiles: MCM's window closed: the Mods page closes");
    }
    page.frames += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    #[test]
    fn the_entry_needs_the_switch_and_a_mods_tiles() {
        let on = super::super::vanilla::TileMenus { start: true, ..Default::default() };
        let off = super::super::vanilla::TileMenus::default();
        let path = r"menus\options\start_menu.xml";
        let vanilla = r#"<menu name="StartMenu"> <rect name="Panel"/> </menu>"#;
        let mut kept = None;
        let mut ui = Ui::new();
        assert!(!available(on, &ui, None, &mut kept), "no start menu");
        // The vanilla file from the archive: no mod's tiles.
        let src = MemSource::default().with(path, vanilla);
        ui.load_menu(&src, path).unwrap();
        assert!(!available(on, &ui, Some(&src), &mut kept));
        // A mod's tile injected into it.
        assert!(ui.inject_text(&src, "StartMenu", r#"<rect name="MCM"/>"#));
        assert!(available(on, &ui, Some(&src), &mut kept));
        assert!(!available(off, &ui, Some(&src), &mut kept), "the switch off");
    }

    #[test]
    fn a_loose_reskin_of_the_start_menu_is_no_mods_page() {
        let on = super::super::vanilla::TileMenus { start: true, ..Default::default() };
        let path = r"menus\options\start_menu.xml";
        let vanilla = r#"<menu name="StartMenu"> <rect name="Panel"/> </menu>"#;
        // A UI overhaul's loose copy: the same tiles, other looks.
        let reskin = MemSource::default().with_loose_over(path, r#"<menu name="StartMenu"> <rect name="Panel"> <width>300</width> </rect> </menu>"#, vanilla);
        let mut ui = Ui::new();
        ui.load_menu(&reskin, path).unwrap();
        assert!(ui.has_mod_added_tiles(ui.menu(START_MENU).unwrap()), "the loose file's tiles count as loose");
        assert!(!available(on, &ui, Some(&reskin), &mut None));
        // A loose copy that adds a tile of its own.
        let added = MemSource::default().with_loose_over(path, r#"<menu name="StartMenu"> <rect name="Panel"/> <rect name="ModPage"/> </menu>"#, vanilla);
        let mut ui = Ui::new();
        ui.load_menu(&added, path).unwrap();
        assert!(available(on, &ui, Some(&added), &mut None));
    }
}
