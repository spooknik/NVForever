//! The settings page's language row (#1443): which language NVForever's own words are
//! shown in (`fnv_util::words`). The game's own text is the install's and is not changed
//! by it. A choice acts at once: the settings page is drawn again in it, every other
//! menu the next time it is drawn. It is kept as `language=<id>` in NVForever.ini like
//! every row (`crate::settings`). English is always there; the others are the languages
//! the start found (`fnv_util::words::load`). With no `language=` kept, the start chooses
//! the language of the game's text when its code page names one (#1948,
//! `fnv_util::words::follow_code_page`).

use crate::settings::{add_choice_list, ChoiceList, Setting};
use bevy::prelude::*;
use fnv_util::words;

pub struct LanguagePlugin;

impl Plugin for LanguagePlugin {
    fn build(&self, app: &mut App) {
        add_choice_list(
            app,
            Setting {
                key: "language",
                group: "Interface",
                label: "Language",
                note: "The language of NVForever's own words (menus, messages). The game's own text is the install's.",
                switches: "",
                get: |_| false,
                set: |_, _| {},
                slider: None,
            },
            ChoiceList { names, chosen, pick, save, load: pick, ask_keep: false },
        );
    }
}

fn names(_: &World) -> Vec<String> {
    words::languages().into_iter().map(|(_, name)| name.to_string()).collect()
}

fn chosen(_: &World) -> String {
    let id = words::language();
    words::languages().into_iter().find(|(i, _)| *i == id).map_or_else(|| id.to_string(), |(_, name)| name.to_string())
}

/// By listed name (from the page) or by id (from the INI).
fn pick(_: &mut World, name: &str) -> bool {
    let ok = words::set_language(name);
    if ok {
        info!("language: {}", words::language());
    }
    ok
}

fn save(_: &World) -> String {
    words::language().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_listed_chosen_and_kept() {
        let mut app = App::new();
        app.add_plugins(LanguagePlugin);
        let world = app.world_mut();
        assert_eq!(names(world)[0], "English");
        assert_eq!(chosen(world), "English");
        assert_eq!(save(world), "en");
        assert!(pick(world, "en") && pick(world, "English"));
        assert!(!pick(world, "no such language"));
        let settings = world.resource::<crate::settings::Settings>();
        assert!(settings.index("language").is_some());
    }
}
