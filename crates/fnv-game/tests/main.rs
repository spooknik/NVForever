//! fnv-game's integration tests, built as one test binary (`autotests = false` in Cargo.toml).
//! One binary instead of one per file: the crate and its dependencies are linked and
//! optimised once, not once per file. Each file stays a module of its own; run one with
//! `cargo test -p fnv-game --release --test integration <file>::`.
//! The files that change process-wide state (mod_files' roots, nvse_jg7's combat-music
//! flag, source_compile's mode) are test binaries of their own, listed in Cargo.toml: in
//! one process they would change each other's, or the setting other tests read.
//! A new file goes in the list below or, if it sets such state, in Cargo.toml as a
//! `[[test]]`; `every_test_file_is_built` fails until it is in one of them.

mod common;
#[path = "common/digest.rs"]
mod digest;

mod abilities;
mod actor_values;
mod aid;
mod appearance;
mod area_effects;
mod ash_piles;
mod build_identity;
mod camera_paths;
mod caravan_data;
mod clone_calling_reference;
mod combat_blocks;
mod combat_lines_data;
mod combat_transitions;
mod companion_wheel;
mod container_sound_getters;
mod crime;
mod crippled_limbs_data;
mod dead_on_spawn;
mod death_items;
mod dialogue_data;
mod dialogue;
mod difficulty;
mod effect_updates;
mod experience;
mod extender_conditions;
mod factions;
mod functions;
mod game_clock;
mod hit_effects_data;
mod hit_effects;
mod idles_holstered;
mod idles;
mod item_ref_health;
mod jg_nif;
mod jg10;
mod jg3;
mod jg4;
mod jg5;
mod jg6;
mod jg8;
mod jip10;
mod jip11;
mod jip12;
mod jip13;
mod jip14;
mod jip6;
mod jip7;
mod jip8;
mod jip9;
mod knockdowns;
mod lutana;
mod lutana2;
mod lutana3;
mod magic_archetypes;
mod npc_doors;
mod nvse_itr2;
mod nvsescript;
mod one111;
mod package_choice;
mod packages;
mod perk_combat;
mod perk_talk;
mod pp;
mod radiation;
mod radio_data;
mod respawn;
mod save_pairs;
mod script_events;
mod script_stack;
mod showoff3;
mod set_class;
mod showoff4;
mod showoff5;
mod step_budget;
mod sup1;
mod sup4;
mod sup6;
mod sup7;
mod sweep;
mod temporary_scene_bases;
mod traps;
mod ui_functions;
mod ui_hud;
mod ui_loading;
mod ui_mcm;
mod ui_more;
mod unconscious;
mod unlisted;
mod vendor_services;
mod voice_gate;
mod zones;

/// With `autotests = false` Cargo builds only the files named, so a new file in `tests/`
/// would never run: every one is a `mod` here or a `[[test]]` in Cargo.toml, not both.
mod world_model_fields;

#[test]
fn every_test_file_is_built() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let main = std::fs::read_to_string(dir.join("tests/main.rs")).unwrap();
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    let mods: Vec<&str> = main.lines().filter_map(|l| l.strip_prefix("mod ")?.strip_suffix(';')).collect();
    let own: Vec<&str> =
        manifest.lines().filter_map(|l| l.trim().strip_prefix("path = \"tests/")?.strip_suffix(".rs\"")).collect();
    let mut problems = Vec::new();
    for entry in std::fs::read_dir(dir.join("tests")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        if stem == "main" {
            continue;
        }
        match (mods.contains(&stem.as_str()), own.contains(&stem.as_str())) {
            (false, false) => problems.push(format!("tests/{stem}.rs is not built: add `mod {stem};` to tests/main.rs")),
            (true, true) => problems.push(format!("tests/{stem}.rs is built twice: in tests/main.rs and as a [[test]]")),
            _ => {}
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
