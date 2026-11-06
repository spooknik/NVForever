//! What `GameData::build` makes from the game's own masters, as one digest a field (#1732):
//! a change meant only to make the build faster must leave every line as it was, so a
//! run before the change and one after are compared with `diff`. It needs the game data,
//! which is never in the repository: set `FNVLAB_DATA_DIR` (the parity lab's `data_dir`,
//! parity-lab/lab.json) to the game's `Data` folder. Without it, or without the ten masters
//! there, it prints why it was skipped and passes.
//!
//! ```text
//! FNVLAB_DATA_DIR=<Data> cargo test -p fnv-game --release --test build_identity -- --nocapture
//! ```
//!
//! Only digests are printed, no game text. `NVF_COMPILE_SOURCE=always` also takes the build
//! through `source_compile`. The `script build:` lines are the build's step times.

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_game::GameData;
use fnv_script::FunctionTable;
use fnv_sim::RefTable;
use serde_json::Value as Json;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

const MASTERS: [&str; 10] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "CaravanPack.esm",
];

/// The game's Data folder, or `None` (with the reason printed) when the check cannot run.
fn data_dir() -> Option<PathBuf> {
    let Some(dir) = std::env::var_os("FNVLAB_DATA_DIR").map(PathBuf::from) else {
        eprintln!("skipped: FNVLAB_DATA_DIR is not set (no game data)");
        return None;
    };
    if let Some(missing) = MASTERS.iter().find(|m| !dir.join(m).is_file()) {
        eprintln!("skipped: {missing} is not in {}", dir.display());
        return None;
    }
    Some(dir)
}

/// One text's digest [design: SipHash with `DefaultHasher::new()`'s fixed keys, so the same
/// text gives the same number in every run].
fn text_digest(lines: &[String]) -> u64 {
    let mut h = DefaultHasher::new();
    lines.hash(&mut h);
    h.finish()
}

/// The digest of a value with no hash map in it.
fn ordered<T: Debug>(v: &T) -> u64 {
    text_digest(&[format!("{v:?}")])
}

/// The digest of a collection in no order of its own (a hash map or set): its items, sorted
/// by their text.
fn unordered<T: Debug>(items: impl IntoIterator<Item = T>) -> (usize, u64) {
    let mut lines: Vec<String> = items.into_iter().map(|i| format!("{i:?}")).collect();
    lines.sort_unstable();
    (lines.len(), text_digest(&lines))
}

macro_rules! map_fields {
    ($d:ident, $($f:ident),* $(,)?) => {
        $(
            let (n, h) = unordered($d.$f.iter());
            println!("identity {}: {n} {h:016x}", stringify!($f));
        )*
    };
}

macro_rules! value_fields {
    ($d:ident, $($f:ident),* $(,)?) => {
        $(println!("identity {}: {:016x}", stringify!($f), ordered(&$d.$f));)*
    };
}

fn print_digests(d: &GameData) {
    map_fields!(
        d, scripts, infos, topics, shared_lines, linked_from, follow_up_targets, topic_defs, packages, patrol_scripts, package_defs, messages, terminals, notes,
        form_lists, repair_lists, menu_icons, actors, combat_styles, leveled_actors, form_script, edids, edid_of, form_flags, item_order, object_bounds, full_names,
        sigs, reputations, reputation_names, challenges, challenge_thresholds, challenge_defs, regions, magic_effects, effect_items, ingestibles, spell_types,
        spell_flags, race_spells, voices_without_default, armor_enchantments, enchantment_types, child_races, race_older, race_younger, faction_reactions,
        faction_flags, faction_reputations, cells, owner_ranks, destructibles, weapon_skills, weapon_anim_types, furniture, idle_markers, idles, idle_children,
        image_modifiers, station_flags, perks, actor_value_infos, recipes, camera_paths, camera_shots, recipe_categories, casinos, caravan_cards, caravan_decks,
        need_stages, water_effects, drink_waters, water_damage, radiation_markers, class_services, unit_by_key,
    );
    value_fields!(d, load_order, plugin_versions, quests, lines_moved_by_previous, top_level, greeting, goodbye, ini_texts, scripted_by_cell, default_objects, issues);
    // In order, with each unit's whole text: the units' ids are their places in this list.
    println!("identity units: {} {:016x}", d.units.len(), text_digest(&d.units.iter().map(|c| format!("{:?} {:?} {} {}", c.unit, c.program, c.raw, c.key())).collect::<Vec<_>>()));
    println!("identity read_issues: {:016x} {:016x}", ordered(&d.read_issues.counts), ordered(&d.read_issues.samples));
    println!("identity param_types: {:?}", unordered(d.param_types.0.iter()));
    // Each kept record's fields are decoded here, on first asking, as a script's getter would.
    let (n, h) = unordered(d.sigs.keys().filter_map(|f| d.record_fields.get(f).map(|x| (f, x))));
    println!("identity record_fields: {} {n} {h:016x}", d.record_fields.len());
}

#[test]
fn every_field_digest() {
    let Some(dir) = data_dir() else { return };
    let _ = tracing_subscriber::fmt().with_writer(std::io::stderr).with_max_level(tracing::Level::INFO).try_init();
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(dir.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schema = Schema::load_dir(root.join("schemas/fnv")).unwrap();
    let fp = root.join("schemas/script/functions.json");
    let j: Json = serde_json::from_str(&std::fs::read_to_string(&fp).unwrap()).unwrap();
    let table = FunctionTable::load(&fp).unwrap();
    let refs = RefTable::build(&lo, &schema);
    let data = GameData::build(&lo, &schema, table, &j, &refs);
    print_digests(&data);
}
