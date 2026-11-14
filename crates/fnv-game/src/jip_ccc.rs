//! What JIP LN writes for JIP Companions Command & Control (CCC) before its scripts run
//! (issue #2298, docs/spec-notes/mcm.md §4).
//!
//! CCC's MCM script (`JIPCCCMCMScript`, a `MenuMode 1013` quest script) acts only while
//! `GetMCMFloat 0 0 "_ActiveMod"` equals the global `iJIPCCCModIndex`, and on its first such
//! frame switches on three of MCM's sub-menu rows (`SubMenu1`-`SubMenu3`) and names them
//! [observed: the plugin's script source, CCC 2.60]. MCM's `_ActiveMod` is the chosen
//! mod's load index (its label form's source mod, `SetMCMModList`, menu-xml.md §10.2).
//! No script in the plugin writes `iJIPCCCModIndex` [observed: its 20 scripts], and its
//! file value is 0, MCM's "no mod chosen": so with nobody writing it CCC took every frame
//! of MCM's window as its own page and its sub-menu rows stood over the mod names
//! [observed: the owner's Blue Moon list, h9x000's run, issue #2270]. The only value the
//! comparison works with is CCC's own load index, which JIP LN evidently writes
//! [hypothesis: no JIP source read; the plugin's use of it is the evidence]. It is written
//! when a game is made, new or loaded, over whatever a save kept.
//!
//! `iJIPCCCStartup` (the start-up state CCC's main script reports on: 0 "no NVSE" to 4
//! "UI extension missing", 5 ready) is left as the data has it: at 5 the script calls
//! JIP's own `CCC…` functions, which NVForever does not have (issue #2271).

use fnv_sim::World;

/// CCC's plugin [observed: the Blue Moon list's file name].
pub const PLUGIN: &str = "JIP Companions Command & Control.esp";
/// The global CCC's MCM script compares MCM's `_ActiveMod` with [observed: its source].
pub const MOD_INDEX_GLOBAL: &str = "iJIPCCCModIndex";

/// Writes `iJIPCCCModIndex` as CCC's load index when CCC is in `load_order` and has the
/// global; the index written, if any.
pub fn write_globals(load_order: &[String], world: &mut World<'_>) -> Option<u32> {
    let index = load_order.iter().position(|p| p.eq_ignore_ascii_case(PLUGIN))?;
    let (id, _) = world.forms.global_by_edid(MOD_INDEX_GLOBAL)?;
    world.set_global(id, index as f32).ok()?;
    Some(index as u32)
}
