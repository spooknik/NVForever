//! Where the settings page shows each row: its tab, and the heading it stands under in that
//! tab (#1953; docs/design/settings-page.md). Presentation only: a row keeps the key, the
//! label, the note, the default and the group it registered with (`crate::settings`), and
//! the registry keeps its order, which the values, the INI and the test aids go by. This
//! table only says where the page draws it [design].
//!
//! **Seven tabs, the original's four categories first** [design: #1953; the original's
//! Settings opens Gameplay, Display, Audio and Controls, observed in the original's pause
//! menu, 2026-11-09]: Gameplay, Display, Audio, Controls, then Graphics, Tweaks and Debug
//! ([`TAB_ORDER`]). Each tab lists [`SECTIONS`] in order, a dim heading over each named
//! one.
//!
//! **A row the table does not name** stays under the group it registered with, used as its
//! tab with no heading, after the tab's named rows, in registry order; a group that is
//! none of the seven tabs becomes a tab of its own after Debug, in the order such groups
//! first appear in the registry ([`resolve_layout`]) [design: #1953, so #1892's Tweaks rows
//! show under Tweaks without it naming them here]. No group is renamed for an unnamed row
//! (a `Sound` row the table does not name shows under a `Sound` tab, not under Audio).
//!
//! [`Placement::subgroup`] is kept for the Tweaks tab's own list of groups (#1953 point 4,
//! a later change): every section here has none.
//!
//! What a row's visibility is (the colour sliders only for Custom, the controller rows
//! only with a controller) is not here: the page decides it each frame
//! (`crate::settings_view`), so a hidden row keeps its tab and heading.

use crate::settings::Setting;

/// Where a row is shown: its tab, the later Tweaks list's group, the heading over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Placement {
    pub tab: &'static str,
    /// The Tweaks tab's group (#1953 point 4) [design: reserved; `None` throughout].
    pub subgroup: Option<&'static str>,
    pub heading: Option<&'static str>,
}

impl Placement {
    /// A tab of its own, with no heading.
    pub const fn tab(tab: &'static str) -> Placement {
        Placement { tab, subgroup: None, heading: None }
    }

    /// Its tab, group and heading in English joined by " > " (`Gameplay > Dialogue`), as
    /// the log names it.
    pub fn path(&self) -> String {
        [Some(self.tab), self.subgroup, self.heading].into_iter().flatten().collect::<Vec<_>>().join(" > ")
    }
}

/// A run of rows under one heading.
pub struct Section {
    pub placement: Placement,
    pub keys: &'static [&'static str],
}

/// The tabs, in the order the page lists them [design: #1953].
pub const TAB_ORDER: [&str; 7] = ["Gameplay", "Display", "Audio", "Controls", "Graphics", "Tweaks", "Debug"];
/// The tab whose first rows are the controls' keys and buttons (`crate::bindings`), drawn
/// by the page itself before this table's rows.
pub const CONTROLS_TAB: &str = "Controls";
/// The tab #1892's tweaks register under.
pub const TWEAKS_TAB: &str = "Tweaks";

const fn section(tab: &'static str, heading: Option<&'static str>, keys: &'static [&'static str]) -> Section {
    Section { placement: Placement { tab, subgroup: None, heading }, keys }
}

/// Every named row's tab and heading, in the order the page shows them [design: #1953's
/// table; the colours' red, green and blue each after their colour].
pub const SECTIONS: &[Section] = &[
    section("Gameplay", Some("Game"), &["difficulty", "hardcore", "kill-camera", "vats-style", "vats-slowdown", "reload-slows", "UsePipBoy"]),
    section("Gameplay", Some("Saving"), &["autosave-travel", "autosave-fast-travel", "autosave-wait", "autosave-sleep", "autosave-scripts"]),
    section("Gameplay", Some("Dialogue"), &["menu-pause", "dialogue-goodbye-hint", "dialogue-look", "dialogue-recentre", "dialogue-zoom", "lip-sync"]),
    section("Gameplay", Some("Reports"), &["send-reports", "reports-delete"]),
    section("Gameplay", Some("Camera"), &["detached-camera", "true-iron-sights", "first-person-body", "weapon-sway", "head-bob", "weapon-lower", "field-of-view"]),
    section("Display", Some("Screen"), &["resolution", "borderless", "monitor", "display-sync", "frame-cap", "ui-scale"]),
    section(
        "Display",
        Some("Interface"),
        &["hud-colour", "hud-red", "hud-green", "hud-blue", "pipboy-colour", "pipboy-red", "pipboy-green", "pipboy-blue", "language", "dynamic-crosshair", "crosshair-style", "sneak-gauges"],
    ),
    section(
        "Display",
        Some("Performance"),
        &["fps-counter", "fps-counter-fps", "fps-counter-low", "fps-counter-graph", "fps-counter-split", "fps-counter-cpu", "fps-counter-gpu", "fps-counter-vram", "fps-counter-ram", "fps-counter-loaded", "fps-counter-upscaling"],
    ),
    section("Audio", None, &["volume-master", "volume-music", "volume-footstep", "volume-voice", "volume-effects", "volume-radio", "music", "voice", "reverb", "voice-reverb"]),
    section("Controls", None, &CONTROLLER_KEYS),
    // The quality presets over the rows they set, under a heading of their own; the
    // enhanced look as a whole first under Picture [design: #1966].
    section("Graphics", Some("Quality"), &["quality"]),
    section(
        "Graphics",
        Some("Picture"),
        &[
            "enhanced",
            "grade",
            "sun-shadows",
            "upscaler",
            "upscaler-quality",
            "dlss-model",
            "fsr-library",
            "frame-generation",
            "frame-generation-frames",
            "anti-aliasing",
            "latency-reduction",
            "shadow-filter",
            "ambient-occlusion",
            "fine-shadows",
            "bloom",
            "glow",
            "eye-adaptation",
            "neon-glow",
            "sharpen",
            "anisotropic",
            "light-shafts",
            "water-reflections",
            "water-mirror",
        ],
    ),
    section("Graphics", Some("Distance"), &["distant-lod", "grass", "object-distance", "actor-distance", "item-distance", "shadow-distance", "grass-distance"]),
    section("Graphics", Some("World"), &["head-tracking", "foot-ik", "sky-clock"]),
    section("Debug", None, &["overlay", "hit-markers", "god-mode", "navmesh", "scope-3d"]),
];

/// The Controls tab's rows that are shown only while the input device is a controller
/// (`crate::bindings::InputDevice`), after the controls' own [design: #1953].
pub const CONTROLLER_KEYS: [&str; 4] = ["button-style", "aim-assist", "aim-slowdown", "aim-lock"];

/// Whether row `key` is shown only with a controller ([`CONTROLLER_KEYS`]).
pub fn controller_only(key: &str) -> bool {
    CONTROLLER_KEYS.contains(&key)
}

/// The table's faults: a key named twice, a tab not in [`TAB_ORDER`], a reserved group set.
pub fn check(sections: &[Section]) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::new();
    for s in sections {
        if !TAB_ORDER.contains(&s.placement.tab) {
            return Err(format!("{}: not one of the tabs", s.placement.tab));
        }
        if s.placement.subgroup.is_some() {
            return Err(format!("{}: a group is for the Tweaks list, not yet", s.placement.path()));
        }
        for k in s.keys {
            if seen.contains(k) {
                return Err(format!("{k}: placed twice"));
            }
            seen.push(k);
        }
    }
    Ok(())
}

/// A section of the registered rows: its place and its rows (indices in the registry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSection {
    pub placement: Placement,
    pub rows: Vec<usize>,
}

/// A tab and its sections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTab {
    pub name: &'static str,
    pub sections: Vec<ResolvedSection>,
}

/// One line of a tab as the table gives it: a heading or a registered row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutEntry {
    Heading(Placement),
    Setting(usize),
}

/// The table applied to the registered rows ([`resolve_layout`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResolvedLayout {
    pub tabs: Vec<ResolvedTab>,
    /// Each registered row's place, by its index.
    placements: Vec<Placement>,
    /// The rows the table does not name.
    unnamed: usize,
}

/// The table applied to `list`, the registry in its order (module notes): the seven tabs
/// (an empty one too), each named section with the rows registered (a missing optional row
/// left out, a section left with none dropped), then each tab's unnamed rows, then the
/// unnamed groups' tabs. Every registered row is in it once.
pub fn resolve_layout(list: &[Setting]) -> ResolvedLayout {
    let index: std::collections::HashMap<&str, usize> = list.iter().enumerate().map(|(i, s)| (s.key, i)).collect();
    let mut placements: Vec<Option<Placement>> = vec![None; list.len()];
    let mut tabs: Vec<ResolvedTab> = TAB_ORDER.iter().map(|&name| ResolvedTab { name, sections: Vec::new() }).collect();
    for s in SECTIONS {
        let rows: Vec<usize> = s.keys.iter().filter_map(|k| index.get(k).copied()).filter(|&i| placements[i].is_none()).collect();
        if rows.is_empty() {
            continue;
        }
        for &i in &rows {
            placements[i] = Some(s.placement);
        }
        if let Some(t) = tabs.iter_mut().find(|t| t.name == s.placement.tab) {
            t.sections.push(ResolvedSection { placement: s.placement, rows });
        }
    }
    // The unnamed rows, by tab, in registry order: one section after each tab's named ones.
    let mut unnamed: Vec<(usize, Vec<usize>)> = Vec::new();
    for (i, s) in list.iter().enumerate() {
        if placements[i].is_some() {
            continue;
        }
        placements[i] = Some(Placement::tab(s.group));
        let t = match tabs.iter().position(|t| t.name == s.group) {
            Some(t) => t,
            None => {
                tabs.push(ResolvedTab { name: s.group, sections: Vec::new() });
                tabs.len() - 1
            }
        };
        match unnamed.iter_mut().find(|(at, _)| *at == t) {
            Some((_, rows)) => rows.push(i),
            None => unnamed.push((t, vec![i])),
        }
    }
    let count = unnamed.iter().map(|(_, r)| r.len()).sum();
    for (t, rows) in unnamed {
        let placement = Placement::tab(tabs[t].name);
        tabs[t].sections.push(ResolvedSection { placement, rows });
    }
    ResolvedLayout { tabs, placements: placements.into_iter().map(|p| p.unwrap_or(Placement::tab(""))).collect(), unnamed: count }
}

impl ResolvedLayout {
    /// The tabs' names, in order.
    pub fn tab_names(&self) -> Vec<&'static str> {
        self.tabs.iter().map(|t| t.name).collect()
    }

    pub fn tab(&self, name: &str) -> Option<&ResolvedTab> {
        self.tabs.iter().find(|t| t.name == name)
    }

    /// Registered row `i`'s place.
    pub fn placement_of(&self, i: usize) -> Option<Placement> {
        self.placements.get(i).copied()
    }

    /// How many registered rows the table does not name.
    pub fn unnamed(&self) -> usize {
        self.unnamed
    }

    /// Every registered row in the page's order: tab by tab, section by section (what
    /// search and the Changed list go by).
    pub fn ordered_settings(&self) -> impl Iterator<Item = usize> + '_ {
        self.tabs.iter().flat_map(|t| t.sections.iter().flat_map(|s| s.rows.iter().copied()))
    }

    /// Tab `name`'s lines: each section's heading (when it has one and a row `visible`
    /// says is shown) over those rows. A hidden row keeps its place for when it shows.
    pub fn tab_entries(&self, name: &str, visible: impl Fn(usize) -> bool) -> Vec<LayoutEntry> {
        let mut out = Vec::new();
        let Some(t) = self.tab(name) else { return out };
        for s in &t.sections {
            let rows: Vec<usize> = s.rows.iter().copied().filter(|&i| visible(i)).collect();
            if rows.is_empty() {
                continue;
            }
            if s.placement.heading.is_some() {
                out.push(LayoutEntry::Heading(s.placement));
            }
            out.extend(rows.into_iter().map(LayoutEntry::Setting));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn row(key: &'static str, group: &'static str) -> Setting {
        Setting { key, group, label: key, note: "", switches: "", get: |_| false, set: |_, _| {}, slider: None }
    }

    /// The registry as `Settings::add` orders it.
    fn registry(rows: &[(&'static str, &'static str)]) -> Vec<Setting> {
        let mut s = Settings::default();
        for (k, g) in rows {
            s.add(row(k, g));
        }
        s.list().to_vec()
    }

    fn keys(list: &[Setting], l: &ResolvedLayout, tab: &str) -> Vec<String> {
        l.tab_entries(tab, |_| true)
            .into_iter()
            .map(|e| match e {
                LayoutEntry::Heading(p) => format!("[{}]", p.heading.unwrap_or("")),
                LayoutEntry::Setting(i) => list[i].key.to_string(),
            })
            .collect()
    }

    /// Every row the registry can hold today (upstream main 9af0a4552 and f4015fae8, with #1966's
    /// quality and enhanced rows, #2014's counter rows, the two VATS rows and #2198's Upscaler
    /// and Upscaling mode rows in place of the one upscaling row, its DLSS model row and #2169's
    /// FSR library row, #2242's upscaling counter row and #2317's Generated frames row), by the group it
    /// registers with.
    const KNOWN: [(&str, &str); 109] = [
        ("detached-camera", "Gameplay"),
        ("menu-pause", "Gameplay"),
        ("dialogue-goodbye-hint", "Gameplay"),
        ("reload-slows", "Gameplay"),
        ("music", "Gameplay"),
        ("voice", "Gameplay"),
        ("dialogue-look", "Gameplay"),
        ("dialogue-recentre", "Gameplay"),
        ("dialogue-zoom", "Gameplay"),
        ("dynamic-crosshair", "Gameplay"),
        ("crosshair-style", "Gameplay"),
        ("sneak-gauges", "Gameplay"),
        ("UsePipBoy", "Gameplay"),
        ("kill-camera", "Gameplay"),
        ("vats-style", "Gameplay"),
        ("vats-slowdown", "Gameplay"),
        ("difficulty", "Gameplay"),
        ("hardcore", "Gameplay"),
        ("lip-sync", "Gameplay"),
        ("true-iron-sights", "Gameplay"),
        ("scope-3d", "Gameplay"),
        ("button-style", "Controller"),
        ("aim-assist", "Controller"),
        ("aim-slowdown", "Controller"),
        ("aim-lock", "Controller"),
        ("autosave-travel", "Saving"),
        ("autosave-fast-travel", "Saving"),
        ("autosave-wait", "Saving"),
        ("autosave-sleep", "Saving"),
        ("autosave-scripts", "Saving"),
        ("hud-colour", "Interface"),
        ("hud-red", "Interface"),
        ("hud-green", "Interface"),
        ("hud-blue", "Interface"),
        ("pipboy-colour", "Interface"),
        ("pipboy-red", "Interface"),
        ("pipboy-green", "Interface"),
        ("pipboy-blue", "Interface"),
        ("language", "Interface"),
        ("head-tracking", "World"),
        ("foot-ik", "World"),
        ("sky-clock", "World"),
        ("resolution", "Display"),
        ("borderless", "Display"),
        ("monitor", "Display"),
        ("first-person-body", "Display"),
        ("weapon-sway", "Display"),
        ("head-bob", "Display"),
        ("weapon-lower", "Display"),
        ("sun-shadows", "Display"),
        ("grass", "Display"),
        ("distant-lod", "Display"),
        ("grade", "Display"),
        ("field-of-view", "Display"),
        ("ui-scale", "Display"),
        ("fps-counter", "Display"),
        ("fps-counter-fps", "Display"),
        ("fps-counter-low", "Display"),
        ("fps-counter-graph", "Display"),
        ("fps-counter-split", "Display"),
        ("fps-counter-cpu", "Display"),
        ("fps-counter-gpu", "Display"),
        ("fps-counter-vram", "Display"),
        ("fps-counter-ram", "Display"),
        ("fps-counter-loaded", "Display"),
        ("fps-counter-upscaling", "Display"),
        ("display-sync", "Display"),
        ("frame-cap", "Display"),
        ("object-distance", "Display"),
        ("actor-distance", "Display"),
        ("item-distance", "Display"),
        ("shadow-distance", "Display"),
        ("grass-distance", "Display"),
        ("upscaler", "Graphics"),
        ("upscaler-quality", "Graphics"),
        ("dlss-model", "Graphics"),
        ("fsr-library", "Graphics"),
        ("frame-generation", "Graphics"),
        ("frame-generation-frames", "Graphics"),
        ("anti-aliasing", "Graphics"),
        ("latency-reduction", "Graphics"),
        ("shadow-filter", "Graphics"),
        ("ambient-occlusion", "Graphics"),
        ("fine-shadows", "Graphics"),
        ("bloom", "Graphics"),
        ("glow", "Graphics"),
        ("eye-adaptation", "Graphics"),
        ("neon-glow", "Graphics"),
        ("sharpen", "Graphics"),
        ("anisotropic", "Graphics"),
        ("light-shafts", "Graphics"),
        ("water-reflections", "Graphics"),
        ("water-mirror", "Graphics"),
        ("enhanced", "Graphics"),
        ("quality", "Graphics"),
        ("volume-master", "Sound"),
        ("volume-music", "Sound"),
        ("volume-footstep", "Sound"),
        ("volume-voice", "Sound"),
        ("volume-effects", "Sound"),
        ("volume-radio", "Sound"),
        ("reverb", "Sound"),
        ("voice-reverb", "Sound"),
        ("overlay", "Debug"),
        ("hit-markers", "Debug"),
        ("god-mode", "Debug"),
        ("navmesh", "Debug"),
        ("send-reports", "Reports"),
        ("reports-delete", "Reports"),
    ];

    #[test]
    fn the_table_names_each_key_once_in_one_of_the_tabs() {
        assert_eq!(check(SECTIONS), Ok(()));
        let twice = [section("Gameplay", None, &["a"]), section("Debug", None, &["a"])];
        assert!(check(&twice).unwrap_err().contains("placed twice"));
        let odd = [section("Sound", None, &["a"])];
        assert!(check(&odd).is_err());
        // Every row there is has its place in the table, and the table names no other.
        let named: Vec<&str> = SECTIONS.iter().flat_map(|s| s.keys.iter().copied()).collect();
        for (k, _) in KNOWN {
            assert!(named.contains(&k), "{k} has no place");
        }
        assert_eq!(named.len(), KNOWN.len());
    }

    #[test]
    fn the_seven_tabs_come_first_with_every_row_once_in_the_tables_order() {
        let list = registry(&KNOWN);
        let l = resolve_layout(&list);
        assert_eq!(l.tab_names(), TAB_ORDER, "no group is a tab of its own: every row is named");
        assert_eq!(l.unnamed(), 0);
        let mut all: Vec<usize> = l.ordered_settings().collect();
        assert_eq!(all.len(), list.len());
        all.sort_unstable();
        assert_eq!(all, (0..list.len()).collect::<Vec<_>>(), "each registered row once");
        assert_eq!(keys(&list, &l, "Gameplay")[..9], ["[Game]", "difficulty", "hardcore", "kill-camera", "vats-style", "vats-slowdown", "reload-slows", "UsePipBoy", "[Saving]"]);
        assert_eq!(keys(&list, &l, "Audio"), ["volume-master", "volume-music", "volume-footstep", "volume-voice", "volume-effects", "volume-radio", "music", "voice", "reverb", "voice-reverb"]);
        assert_eq!(keys(&list, &l, "Controls"), CONTROLLER_KEYS);
        assert_eq!(keys(&list, &l, "Debug"), ["overlay", "hit-markers", "god-mode", "navmesh", "scope-3d"]);
        assert!(keys(&list, &l, "Tweaks").is_empty(), "an empty tab is still a tab");
        let at = |k: &str| l.placement_of(list.iter().position(|s| s.key == k).unwrap()).unwrap().path();
        assert_eq!(at("menu-pause"), "Gameplay > Dialogue");
        assert_eq!(at("head-bob"), "Gameplay > Camera");
        assert_eq!(at("hud-red"), "Display > Interface");
        assert_eq!(at("sky-clock"), "Graphics > World");
        assert_eq!(at("quality"), "Graphics > Quality");
        assert_eq!(at("enhanced"), "Graphics > Picture");
        assert_eq!(keys(&list, &l, "Graphics")[..4], ["[Quality]", "quality", "[Picture]", "enhanced"], "the presets first, under their own heading (#1966)");
        assert_eq!(at("button-style"), "Controls");
        // The registry itself is left as it was: its order and each row's group.
        let again = registry(&KNOWN);
        assert!(list.iter().zip(&again).all(|(a, b)| a.key == b.key && a.group == b.group && a.label == b.label && a.note == b.note));
    }

    #[test]
    fn a_missing_row_and_an_empty_heading_are_left_out() {
        // No lip sync, no player body; no head tracking, foot IK or sky clock at all.
        let list = registry(&[("difficulty", "Gameplay"), ("menu-pause", "Gameplay"), ("head-bob", "Display"), ("god-mode", "Debug")]);
        let l = resolve_layout(&list);
        assert_eq!(keys(&list, &l, "Gameplay"), ["[Game]", "difficulty", "[Dialogue]", "menu-pause", "[Camera]", "head-bob"]);
        assert!(keys(&list, &l, "Graphics").is_empty());
        // A hidden row hides its heading when it is the section's only one, and keeps its place.
        let hidden = list.iter().position(|s| s.key == "menu-pause").unwrap();
        let shown: Vec<_> = l.tab_entries("Gameplay", |i| i != hidden);
        assert!(!shown.iter().any(|e| matches!(e, LayoutEntry::Heading(p) if p.heading == Some("Dialogue"))));
        assert_eq!(l.placement_of(hidden).unwrap().path(), "Gameplay > Dialogue");
    }

    #[test]
    fn an_unnamed_row_stays_under_its_own_group() {
        let list = registry(&[("difficulty", "Gameplay"), ("new-switch", "Gameplay"), ("odd-sound", "Sound"), ("x1", "Mods"), ("volume-master", "Sound"), ("x2", "Extras"), ("x3", "Mods")]);
        let l = resolve_layout(&list);
        // After Debug, in the order the groups first appear in the registry; Sound is not Audio.
        assert_eq!(l.tab_names()[7..], ["Sound", "Mods", "Extras"]);
        assert_eq!(keys(&list, &l, "Gameplay"), ["[Game]", "difficulty", "new-switch"]);
        assert_eq!(keys(&list, &l, "Sound"), ["odd-sound"]);
        assert_eq!(keys(&list, &l, "Audio"), ["volume-master"]);
        assert_eq!(keys(&list, &l, "Mods"), ["x1", "x3"]);
        let new = list.iter().position(|s| s.key == "new-switch").unwrap();
        assert_eq!(l.placement_of(new), Some(Placement::tab("Gameplay")));
        assert_eq!(l.unnamed(), 5);
    }

    #[test]
    fn hundreds_of_unnamed_tweaks_keep_the_registrys_order() {
        // #1892 registers its tweaks under Tweaks without naming them here.
        let names: Vec<&'static str> = (0..650)
            .map(|i| {
                let n: &'static str = Box::leak(format!("tweak-{i:03}").into_boxed_str());
                n
            })
            .collect();
        let mut rows: Vec<(&'static str, &'static str)> = vec![("difficulty", "Gameplay")];
        rows.extend(names.iter().map(|n| (*n, "Tweaks")));
        rows.push(("god-mode", "Debug"));
        let list = registry(&rows);
        let l = resolve_layout(&list);
        assert_eq!(l.tab_names(), TAB_ORDER);
        let shown = keys(&list, &l, "Tweaks");
        assert_eq!(shown.len(), 650, "no heading: all in one run");
        assert!(shown.iter().zip(&names).all(|(a, b)| a == b), "in the order registered");
        assert_eq!(l.ordered_settings().count(), list.len());
    }
}
