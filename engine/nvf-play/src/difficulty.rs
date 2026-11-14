//! The game's difficulty in play (docs/spec-notes/difficulty.md): a row of the settings
//! page's Gameplay group (kept in `NVForever.ini` as every row is) and the console's
//! `SetGameDifficulty` / `GetGameDifficulty`. The rules are `fnv_game::difficulty`'s;
//! this keeps the chosen level and hands it to the running game each frame (the game is
//! made after the settings are read, and again for a loaded save).

use crate::script::ScriptState;
use crate::settings::{add_setting, choice_index, choice_part, Setting, Slider};
use bevy::prelude::*;
use fnv_game::difficulty::{Difficulty, LEVELS};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The chosen level's code, for the process: the settings page and the console set it.
static LEVEL: AtomicUsize = AtomicUsize::new(2);

/// The chosen level.
pub fn level() -> Difficulty {
    LEVELS[LEVEL.load(Ordering::Relaxed).min(LEVELS.len() - 1)]
}

/// Choose a level; the game takes it on its next frame.
pub fn set_level(d: Difficulty) {
    LEVEL.store(d.code(), Ordering::Relaxed);
}

pub struct DifficultyPlugin;

impl Plugin for DifficultyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync.in_set(crate::PlaySet::Input));
        add_setting(
            app,
            Setting::slider(
                "difficulty",
                crate::pause::GROUP_MENUS,
                "Difficulty",
                "How hard the fights are: from Very Easy (your hits do double damage, you take half) to Very Hard (half and double); Normal as they are.",
                Slider::choice(LEVELS.len(), |_| choice_part(level().code(), LEVELS.len()), |_, v| set_level(LEVELS[choice_index(v, LEVELS.len())]), |v| LEVELS[choice_index(v, LEVELS.len())].name().to_string()),
            ),
        );
    }
}

/// Hand the chosen level to the game when they differ, and log the change.
fn sync(mut script: Option<ResMut<ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let d = level();
    if rt.game.difficulty != d {
        let line = rt.game.set_difficulty(d);
        info!("{line}");
    }
}
