//! Test aid `NVF_PLAY_TEST_MOD_LOG=1`: the log says how the load order's mods loaded and
//! what their scripts ran into, in lines that start with `mods:`, so one run of a mod
//! list answers "did everything load" without a second tool [design].
//!
//! - At the start: the load order, and for each mod its scripts (how many use the script
//!   extender and run from their source, which do not compile, the functions and names
//!   NVForever does not have).
//! - While playing, each once: what a mod's scripts ask about plugins and read
//!   (`fnv_game::plugin_questions`: a "not there" is why a mod switches itself off), and
//!   the first of each failure in a mod's script (a function not built, a call that
//!   failed, a statement skipped, a script stopped early).
//! - Under the pause menu's Mods page: where MCM's mod list stands, each time it changes
//!   ([`mcm_list_line`]).
//! - At exit: the counts per mod, and what was asked for and not there.
//!
//! A mod is a plugin that is not one of the game's own files ([`GAME_PLUGINS`]). The
//! lines name plugins, scripts and functions; they quote no script text.

use bevy::log::{info, warn};
use fnv_game::plugin_questions::Question;
use fnv_game::source_compile::SourceUnit;
use fnv_game::trace::{Event, EventKind};
use std::collections::{BTreeMap, BTreeSet};

pub const ENV: &str = "NVF_PLAY_TEST_MOD_LOG";

/// The base game's plugin and its add-ons' [documented: the file names of the game's
/// Data folder, as the examples' `MASTERS` lists have them].
const GAME_PLUGINS: [&str; 10] = [
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

/// How many scripts that do not compile one mod names.
const FAILED_SHOWN: usize = 10;

/// Whether the aid is on (`0` and the empty value leave it off).
pub fn on() -> bool {
    std::env::var(ENV).is_ok_and(|v| !matches!(v.trim(), "" | "0"))
}

fn is_mod(plugin: &str) -> bool {
    !plugin.is_empty() && !GAME_PLUGINS.iter().any(|g| g.eq_ignore_ascii_case(plugin))
}

/// A unit key's plugin and owner label (`<plugin>|<owner label>`).
fn split(unit: &str) -> (&str, &str) {
    unit.split_once('|').unwrap_or(("", unit))
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The lines at the start. `units`: every script unit's key and whether it holds bytes
/// the decoder could not read; `source`: what the source compile made of the units it
/// picked (`GameData::source_units`).
pub fn load_lines<'a>(load_order: &[String], units: impl Iterator<Item = (&'a str, bool)>, source: &[SourceUnit]) -> Vec<String> {
    let mods: Vec<&String> = load_order.iter().filter(|p| is_mod(p)).collect();
    let listed: Vec<String> = load_order.iter().enumerate().map(|(i, p)| format!("{i:02X} {p}")).collect();
    let mut out = vec![format!("load order, {} ({}): {}", plural(load_order.len(), "plugin", "plugins"), plural(mods.len(), "mod", "mods"), listed.join(", "))];
    let from_source: BTreeSet<&str> = source.iter().map(|u| u.key.as_str()).collect();
    // Per plugin, lower case: (units, units with unread bytes the source compile did not pick).
    let mut totals: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (key, unread) in units {
        let t = totals.entry(split(key).0.to_ascii_lowercase()).or_default();
        t.0 += 1;
        t.1 += usize::from(unread && !from_source.contains(key));
    }
    for plugin in mods {
        let (total, no_source) = totals.get(&plugin.to_ascii_lowercase()).copied().unwrap_or_default();
        let picked: Vec<&SourceUnit> = source.iter().filter(|u| u.plugin.eq_ignore_ascii_case(plugin)).collect();
        if total == 0 {
            out.push(format!("{plugin}: no scripts"));
            continue;
        }
        let failed: Vec<&&SourceUnit> = picked.iter().filter(|u| u.error.is_some()).collect();
        out.push(format!(
            "{plugin}: {}, {} the script extender: {} from source, {} not compile and cannot run",
            plural(total, "script", "scripts"),
            plural(picked.len(), "uses", "use"),
            plural(picked.len() - failed.len(), "runs", "run"),
            plural(failed.len(), "does", "do")
        ));
        for u in failed.iter().take(FAILED_SHOWN) {
            let e = u.error.as_ref().expect("filtered on it");
            out.push(format!("{plugin}: does not compile: {} (line {}: {})", split(&u.key).1, e.line, e.kind));
        }
        if failed.len() > FAILED_SHOWN {
            out.push(format!("{plugin}: does not compile: {} more (fnvtool ext report lists them)", failed.len() - FAILED_SHOWN));
        }
        if no_source > 0 {
            out.push(format!("{plugin}: {} extender bytes and no source text to compile: what the decoder cannot read of them does not run", plural(no_source, "script has", "scripts have")));
        }
        let each_once = |names: &dyn Fn(&SourceUnit) -> &Vec<String>| -> Vec<String> {
            let mut seen = BTreeSet::new();
            picked.iter().flat_map(|u| names(u).iter()).filter(|n| seen.insert(n.to_ascii_lowercase())).cloned().collect()
        };
        let functions = each_once(&|u| &u.unknown_functions);
        if !functions.is_empty() {
            out.push(format!("{plugin}: calls {} NVForever does not have (the calls do nothing): {}", plural(functions.len(), "function", "functions"), functions.join(", ")));
        }
        let enums = each_once(&|u| &u.unknown_enums);
        if !enums.is_empty() {
            out.push(format!("{plugin}: uses {} NVForever has no code for: {}", plural(enums.len(), "name", "names"), enums.join(", ")));
        }
    }
    out
}

/// A plugin question's function as scripts spell it.
fn spelled(function: &str) -> &str {
    match function {
        "isplugininstalled" => "IsPluginInstalled",
        "getpluginversion" => "GetPluginVersion",
        "isdllloaded" => "IsDLLLoaded",
        "ismodloaded" => "IsModLoaded",
        "getmodindex" => "GetModIndex",
        other => other,
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    errors: u64,
    failed: u64,
    skipped: u64,
    unbuilt: u64,
}

/// What the run met so far.
#[derive(Debug, Default)]
pub struct ModLog {
    /// (unit, what) said already.
    said: BTreeSet<(String, String)>,
    /// Per mod, as its unit keys spell it.
    counts: BTreeMap<String, Counts>,
    /// The questions answered "not there", for the line at exit.
    absent: Vec<String>,
    asked: usize,
    /// MCM's list as last said ([`mcm_list_line`]).
    mcm_list: String,
    /// What was skipped since the last statement the VM named (`SkippedAt`).
    pending: Vec<String>,
}

impl ModLog {
    /// The line for a question a script asked (each comes once, `Questions::record`).
    pub fn question(&mut self, q: &Question) -> String {
        self.asked += 1;
        let who = if q.plugin.is_empty() { "the console" } else { q.plugin.as_str() };
        let asks = format!("{} \"{}\"", spelled(&q.function), q.name);
        if q.absent() {
            self.absent.push(format!("{asks} ({who})"));
        }
        format!("{who} asks {asks}: {}{} (first in {})", q.answer, if q.absent() { ", not there" } else { "" }, split(&q.unit).1)
    }

    /// The line for a trace event, when it is the first of its kind in a mod's script;
    /// every one is counted.
    pub fn event(&mut self, e: &Event) -> Option<String> {
        // Each statement that skipped something is said once, with what it skipped, so
        // the lines behind a reason can be found.
        if let EventKind::SkippedAt { unit, line } = &e.kind {
            let mut whats = std::mem::take(&mut self.pending);
            whats.dedup();
            let (plugin, label) = split(unit);
            if !is_mod(plugin) {
                return None;
            }
            return self.said.insert((unit.clone(), format!("at {line}"))).then(|| format!("{plugin}: {label}: skipped at: {line} ({})", whats.join("; ")));
        }
        if let EventKind::Skipped { what, .. } = &e.kind {
            self.pending.push(what.clone());
        }
        let (unit, what, line) = match &e.kind {
            EventKind::Unhandled { name, unit, .. } => (unit, format!("unbuilt {name}"), format!("{name} is not built: its calls do nothing")),
            EventKind::CallFailed { name, unit, error, .. } => (unit, format!("failed {name}"), format!("{name} failed: {error}")),
            EventKind::Skipped { unit, what } => (unit, format!("skipped {what}"), format!("skipped: {what}")),
            EventKind::ScriptError { unit, error } => (unit, format!("error {error}"), format!("stopped early: {error}")),
            _ => return None,
        };
        let (plugin, label) = split(unit);
        if !is_mod(plugin) {
            return None;
        }
        let c = self.counts.entry(plugin.to_string()).or_default();
        match &e.kind {
            EventKind::Unhandled { .. } => c.unbuilt += 1,
            EventKind::CallFailed { .. } => c.failed += 1,
            EventKind::Skipped { .. } => c.skipped += 1,
            _ => c.errors += 1,
        }
        self.said.insert((unit.clone(), what)).then(|| format!("{plugin}: {label}: {line}"))
    }

    /// The lines at exit.
    pub fn final_lines(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .counts
            .iter()
            .map(|(plugin, c)| format!("at exit: {plugin}: {} script errors, {} failed calls, {} skipped statements, {} functions not built", c.errors, c.failed, c.skipped, c.unbuilt))
            .collect();
        if out.is_empty() {
            out.push("at exit: no script error, failed call, skipped statement or function not built in the mods' scripts".to_string());
        }
        out.push(if self.absent.is_empty() {
            format!("at exit: {} about plugins, none answered \"not there\"", plural(self.asked, "question", "questions"))
        } else {
            format!("at exit: {} about plugins, asked for and not there: {}", plural(self.asked, "question", "questions"), self.absent.join(", "))
        });
        out
    }
}

/// The game keeps what scripts ask about plugins from here on.
pub fn keep_questions(game: &mut fnv_game::Game<'_>) {
    game.plugin_questions.get_or_insert_with(Default::default);
}

/// The run's start, once the scripts are ready: the start lines go to the log.
pub fn start(game: &mut fnv_game::Game<'_>) -> ModLog {
    keep_questions(game);
    let data = game.data;
    let keys: Vec<(String, bool)> = data.units.iter().map(|c| (c.key(), c.raw > 0)).collect();
    info!("mods: test aid {ENV}: how the load order's mods loaded and what their scripts met");
    for line in load_lines(&data.load_order, keys.iter().map(|(k, unread)| (k.as_str(), *unread)), &data.source_units) {
        info!("mods: {line}");
    }
    ModLog::default()
}

/// A frame's events and the questions asked since the last call.
pub fn frame(log: &mut ModLog, game: &mut fnv_game::Game<'_>, events: &[Event]) {
    for q in game.plugin_questions.as_mut().map(|q| q.take()).unwrap_or_default() {
        let absent = q.absent();
        let line = log.question(&q);
        if absent {
            warn!("mods: {line}");
        } else {
            info!("mods: {line}");
        }
    }
    for line in events.iter().filter_map(|e| log.event(e)) {
        warn!("mods: {line}");
    }
}

/// MCM's mod list in the start menu's tree (menu-xml.md §10).
const MCM_LIST: &str = "StartMenu/MCM/MCM_ModList";

/// Where MCM's mod list stands: what places its rows (the selected mod, the count of its
/// sub menus, the arrows, the row step) and each row that is switched on, a mod's (`Mod1`
/// to `Mod23`) or a sub menu's (`SubMenu1` to `SubMenu10`), with the name it shows.
/// `None` without MCM's list in the tree.
pub fn mcm_list_line(ui: &fnv_ui::Ui) -> Option<String> {
    ui.find_tile(MCM_LIST)?;
    let read = |p: &str| ui.get_float(p).map_or("none".to_string(), |v| ((v * 100.0).round() / 100.0).to_string());
    let list = |p: &str| read(&format!("{MCM_LIST}/{p}"));
    let names = (1..=23).map(|k| format!("Mod{k}")).chain((1..=10).map(|k| format!("SubMenu{k}")));
    let rows: Vec<String> = names
        .filter(|n| ui.get_float(&format!("{MCM_LIST}/{n}/_enable")).unwrap_or(0.0) != 0.0)
        .map(|n| {
            let text = ui.get_string(&format!("{MCM_LIST}/{n}/text/string")).unwrap_or_default();
            format!(
                "{n} {:?} y {} (text {}) height {} visible {} _modID {} _subID {}",
                text.trim(),
                list(&format!("{n}/y")),
                list(&format!("{n}/text/y")),
                list(&format!("{n}/height")),
                list(&format!("{n}/visible")),
                list(&format!("{n}/_modID")),
                list(&format!("{n}/_subID"))
            )
        })
        .collect();
    // The option panel: where it and its two columns stand, and the rows that are on.
    const OPTIONS: &str = "StartMenu/MCM/MCM_Options";
    let option = |p: &str| read(&format!("{OPTIONS}/{p}"));
    let on: Vec<String> = (1..=50).map(|k| format!("Option{k}")).filter(|n| ui.get_float(&format!("{OPTIONS}/{n}/_enable")).unwrap_or(0.0) != 0.0).collect();
    let shown: Vec<String> = on
        .iter()
        .take(3)
        .map(|n| {
            let title = ui.get_string(&format!("{OPTIONS}/{n}/title/string")).unwrap_or_default();
            let value = ui.get_string(&format!("{OPTIONS}/{n}/value/text/string")).unwrap_or_default();
            format!(
                "{n} {:?} x {} y {} visible {} title y {} _type {} _value {} shown {:?}",
                title.trim(),
                option(&format!("{n}/x")),
                option(&format!("{n}/y")),
                option(&format!("{n}/visible")),
                option(&format!("{n}/title/y")),
                option(&format!("{n}/_type")),
                option(&format!("{n}/_value")),
                value.trim()
            )
        })
        .collect();
    // The rows with a suffix: its text and measured width, and where the value's text
    // stands left of it (OptionRect.xml: `_xSuffix` is minus the suffix's width).
    let suffixes: Vec<String> = on
        .iter()
        .filter(|n| ui.get_float(&format!("{OPTIONS}/{n}/_suffix")).unwrap_or(0.0) != 0.0)
        .map(|n| {
            let text = ui.get_string(&format!("{OPTIONS}/{n}/value/suffix/string")).unwrap_or_default();
            format!("{n} suffix {text:?} width {} value x {}", option(&format!("{n}/value/suffix/width")), option(&format!("{n}/value/text/x")))
        })
        .collect();
    let shown = if suffixes.is_empty() { shown } else { [shown, vec![format!("suffixes: {}", suffixes.join(", "))]].concat() };
    let options = format!(
        "options: panel x {} y {} width {} visible {}, _x1 {}, _x2 {}, _columns {}; MCM x {} y {} width {}; {} rows on{}{}",
        option("x"),
        option("y"),
        option("width"),
        option("visible"),
        option("_x1"),
        option("_x2"),
        option("_columns"),
        read("StartMenu/MCM/x"),
        read("StartMenu/MCM/y"),
        read("StartMenu/MCM/width"),
        on.len(),
        if shown.is_empty() { "" } else { ": " },
        shown.join("; ")
    );
    let values = format!(
        "MCM _ActiveOption {}, _Value {}, _NewValue {}, _Reset {}, _ShowList {}, _ShowScale {}",
        read("StartMenu/MCM/_ActiveOption"),
        read("StartMenu/MCM/_Value"),
        read("StartMenu/MCM/_NewValue"),
        read("StartMenu/MCM/_Reset"),
        read("StartMenu/MCM/_ShowList"),
        read("StartMenu/MCM/_ShowScale")
    );
    Some(format!(
        "MCM's list: _selected {}, _subCount {}, clicked row _index {} _modID {}, ArrowUp y {}, ArrowDown y {}, row height {}, _spacing {}, _listY {}; MCM _ActiveMod {}, _ActiveSubMenu {}, _UnClick {}, _AltInput {}; rows on: {}; {options}; {values}",
        list("_selected"),
        list("_subCount"),
        list("_index"),
        list("_modID"),
        list("ArrowUp/y"),
        list("ArrowDown/y"),
        list("MCM_ModList_Size/height"),
        list("_spacing"),
        list("_listY"),
        read("StartMenu/MCM/_ActiveMod"),
        read("StartMenu/MCM/_ActiveSubMenu"),
        read("StartMenu/MCM/_UnClick"),
        read("StartMenu/MCM/_AltInput"),
        if rows.is_empty() { "none".to_string() } else { rows.join("; ") }
    ))
}

/// A frame under the pause menu's Mods page: the questions asked there, and MCM's list
/// when it changed.
pub fn mods_page(log: &mut ModLog, game: &mut fnv_game::Game<'_>) {
    frame(log, game, &[]);
    if let Some(line) = mcm_list_line(&game.ui_tree).filter(|l| *l != log.mcm_list) {
        info!("mods: {line}");
        log.mcm_list = line;
    }
}

/// At exit: what is left to say, then the counts.
pub fn finish(log: &mut ModLog, game: &mut fnv_game::Game<'_>) {
    frame(log, game, &[]);
    for line in log.final_lines() {
        info!("mods: {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_game::source_compile::SourceError;

    fn unit(key: &str, error: Option<(usize, &'static str)>, functions: &[&str], enums: &[&str]) -> SourceUnit {
        SourceUnit {
            key: key.to_string(),
            plugin: split(key).0.to_string(),
            error: error.map(|(line, kind)| SourceError { line, kind, detail: "set x to Secret".to_string() }),
            unknown_functions: functions.iter().map(|s| s.to_string()).collect(),
            unknown_enums: enums.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn the_start_lines_say_what_each_mod_loaded() {
        let order: Vec<String> = ["FalloutNV.esm", "DeadMoney.esm", "A.esp", "B.esp", "C.esp"].iter().map(|s| s.to_string()).collect();
        let units = [("FalloutNV.esm|script 1", false), ("A.esp|script 1", false), ("A.esp|script 2", true), ("A.esp|script 3", false), ("A.esp|script 4", true), ("B.esp|quest 5 stage 10", false)];
        let source = [unit("A.esp|script 1", None, &["NoSuch", "Other"], &[]), unit("A.esp|script 2", Some((12, "syntax")), &[], &[]), unit("A.esp|script 3", None, &["nosuch"], &["anim_group: Fly"])];
        let lines = load_lines(&order, units.iter().copied(), &source);
        assert_eq!(
            lines,
            [
                "load order, 5 plugins (3 mods): 00 FalloutNV.esm, 01 DeadMoney.esm, 02 A.esp, 03 B.esp, 04 C.esp",
                "A.esp: 4 scripts, 3 use the script extender: 2 run from source, 1 does not compile and cannot run",
                "A.esp: does not compile: script 2 (line 12: syntax)",
                "A.esp: 1 script has extender bytes and no source text to compile: what the decoder cannot read of them does not run",
                "A.esp: calls 2 functions NVForever does not have (the calls do nothing): NoSuch, Other",
                "A.esp: uses 1 name NVForever has no code for: anim_group: Fly",
                "B.esp: 1 script, 0 use the script extender: 0 run from source, 0 do not compile and cannot run",
                "C.esp: no scripts",
            ]
        );
        // No script text: the compiler's detail, which may quote a line, stays out.
        assert!(lines.iter().all(|l| !l.contains("Secret")));
    }

    #[test]
    fn a_mods_failures_are_said_once_and_counted() {
        let mut log = ModLog::default();
        let failed = |unit: &str| Event { frame: 0, kind: EventKind::CallFailed { opcode: "0x2000".into(), name: "NoSuch".into(), unit: unit.into(), error: "not built".into() } };
        assert_eq!(log.event(&failed("A.esp|script 1")).as_deref(), Some("A.esp: script 1: NoSuch failed: not built"));
        assert_eq!(log.event(&failed("A.esp|script 1")), None);
        assert_eq!(log.event(&failed("A.esp|script 2")).as_deref(), Some("A.esp: script 2: NoSuch failed: not built"));
        // The game's own scripts are not the aid's.
        assert_eq!(log.event(&failed("FalloutNV.esm|script 1")), None);
        let skipped = Event { frame: 0, kind: EventKind::Skipped { unit: "A.esp|script 1".into(), what: "division by zero".into() } };
        assert_eq!(log.event(&skipped).as_deref(), Some("A.esp: script 1: skipped: division by zero"));
        let at = |line: &str| Event { frame: 0, kind: EventKind::SkippedAt { unit: "A.esp|script 1".into(), line: line.into() } };
        assert_eq!(log.event(&at("let x := 1 / 0")).as_deref(), Some("A.esp: script 1: skipped at: let x := 1 / 0 (division by zero)"));
        assert_eq!(log.event(&skipped), None);
        assert_eq!(log.event(&at("let x := 1 / 0")), None, "a statement is said once");
        assert_eq!(log.event(&skipped), None);
        assert_eq!(log.event(&at("let y := 2 / 0")).as_deref(), Some("A.esp: script 1: skipped at: let y := 2 / 0 (division by zero)"));
        let mut q = fnv_game::plugin_questions::Questions::default();
        q.record("isplugininstalled", "yUI", 0.0, "A.esp|script 1".into(), "a.esp".into());
        q.record("getpluginversion", "JIP NVSE Plugin", 57.3, "A.esp|script 1".into(), "a.esp".into());
        let asked: Vec<String> = q.take().iter().map(|x| log.question(x)).collect();
        assert_eq!(asked, ["a.esp asks IsPluginInstalled \"yUI\": 0, not there (first in script 1)", "a.esp asks GetPluginVersion \"JIP NVSE Plugin\": 57.3 (first in script 1)"]);
        assert_eq!(
            log.final_lines(),
            ["at exit: A.esp: 0 script errors, 3 failed calls, 3 skipped statements, 0 functions not built", "at exit: 2 questions about plugins, asked for and not there: IsPluginInstalled \"yUI\" (a.esp)"]
        );
    }

    #[test]
    fn a_clean_run_says_so() {
        assert_eq!(
            ModLog::default().final_lines(),
            ["at exit: no script error, failed call, skipped statement or function not built in the mods' scripts", "at exit: 0 questions about plugins, none answered \"not there\""]
        );
    }
}
