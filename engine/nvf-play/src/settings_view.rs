//! What the settings page shows, as data (#1953; docs/design/settings-page.md): which rows
//! a tab, the search or the Changed list lists, under which headings, which row
//! is chosen, and how the keys move over them. `crate::pause` draws it and turns the keys
//! and the pointer into these steps; nothing here draws or reads input.
//!
//! **Search and the Changed list** [design: #1953 points 5 and 6] list rows of every tab:
//! [`search`] the rows whose words hold every term of the query, [`changed`] the rows set
//! away from their default, each in the page's order ([`catalogue`]) under a heading that
//! says where it lives (its tab and heading, [`listing`]). A row hidden where it lives
//! (a colour's channel, a controller's row, a control or sensitivity of the device not in
//! use) is listed as the row that stands for it there ([`shown_as`]), once, and the page
//! names what it stands for ([`stands_for`]): a result of the other device is the Input
//! Device row, so acting on it switches the device and never edits the other device's
//! binding through the one in use [design: #1953].
//!
//! **A row** is a registered setting by its index in the registry (`crate::settings`,
//! whose order stays the one the values, the INI and the test aids go by), or one of the
//! Controls tab's own rows (`crate::bindings`: the input device, each control's key or
//! button, Invert Y, the sensitivity, Rumble), each by a name of its own ([`RowId`]), never
//! by where it stands. A heading is not a row: it takes one line of the window and the
//! scroll bar counts it, but it cannot be chosen ([`ViewEntry`]) [design].
//!
//! **What is shown** is worked out each frame from the game's state ([`Visibility`]): a
//! colour's red, green and blue only while the colour is Custom
//! (`crate::ui_colour::channel_shown`), the controller's rows of the Controls tab only with
//! the controller as the input device ([`crate::settings_layout::controller_only`]). A
//! hidden row keeps its tab and heading; a choice on it moves to the row that stands for
//! it ([`parent_of`]: the colour's list, the Input Device row) [design].
//!
//! **The keys** [design: #1953]: Up and Down step over the rows, round the ends, passing
//! the headings ([`step_selectable`]); Page Up and Page Down go to the first row under the
//! heading before or after ([`step_heading`]); a tab without headings is one section. The
//! window of `shown` lines keeps the chosen row in view, and its heading too when it is the
//! heading's first row ([`follow`]); a heading is never drawn alone on the window's last
//! line ([`drawn_slots`]). How many lines the window has is the page's layout's
//! (`crate::pause`, from the room between the list's top and the note, #2377: 12 lines); the original's
//! `options_container` shows nine [documented: `_number_of_visible_items` 9], NVForever's page
//! more where the screen has room [design: #2377]. A window of one line shows the chosen row,
//! not its heading.

use crate::bindings::{Bindings, Control, InputDevice, GAME_CONTROLS};
use crate::settings::Setting;
use fnv_util::words::tr;
use crate::settings_layout::{controller_only, resolve_layout, LayoutEntry, Placement, ResolvedLayout, CONTROLS_TAB, TAB_ORDER};
use bevy::prelude::*;

/// One of the Controls tab's own rows (`crate::bindings`), by what it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlRow {
    /// Input Device: keyboard and mouse, or a controller.
    Device,
    /// Saved opt-out beside Input Device [design, #2200].
    AutomaticInput,
    /// A control's key and mouse button (`KeyboardMouse`) or its controller button.
    Binding(Control, InputDevice),
    Invert,
    /// The look sensitivity of a device: Mouse Sensitivity or Stick Sensitivity.
    Sensitivity(InputDevice),
    /// A device's Vertical Sensitivity, a multiple of its horizontal look (#2201).
    Vertical(InputDevice),
    /// With a controller only.
    Rumble,
}

impl ControlRow {
    /// The Controls tab's own rows with `device` in use: the device, automatic switching
    /// [design, #2200], every control, Invert
    /// Y, the device's sensitivity and vertical sensitivity, and with a controller Rumble.
    pub fn rows(device: InputDevice) -> Vec<ControlRow> {
        let mut out = vec![ControlRow::Device, ControlRow::AutomaticInput];
        out.extend(GAME_CONTROLS.iter().map(|(c, _, _)| ControlRow::Binding(*c, device)));
        out.extend([ControlRow::Invert, ControlRow::Sensitivity(device), ControlRow::Vertical(device)]);
        if device == InputDevice::Controller {
            out.push(ControlRow::Rumble);
        }
        out
    }

    /// Every Controls row of both devices, in the order search and the Changed list go by:
    /// the device, each control's key, each control's controller button, Invert Y, the
    /// mouse's sensitivity and vertical sensitivity, the stick's, Rumble [design: #1953].
    pub fn all() -> Vec<ControlRow> {
        let mut out = vec![ControlRow::Device, ControlRow::AutomaticInput];
        for d in [InputDevice::KeyboardMouse, InputDevice::Controller] {
            out.extend(GAME_CONTROLS.iter().map(|(c, _, _)| ControlRow::Binding(*c, d)));
        }
        out.push(ControlRow::Invert);
        for d in [InputDevice::KeyboardMouse, InputDevice::Controller] {
            out.extend([ControlRow::Sensitivity(d), ControlRow::Vertical(d)]);
        }
        out.push(ControlRow::Rumble);
        out
    }

    /// The row's place in [`ControlRow::rows`], the same with either device (the UI sounds
    /// hear a move by it).
    fn ordinal(self) -> usize {
        match self {
            ControlRow::Device => 0,
            ControlRow::AutomaticInput => 1,
            ControlRow::Binding(c, _) => 2 + GAME_CONTROLS.iter().position(|(k, _, _)| *k == c).unwrap_or(0),
            ControlRow::Invert => GAME_CONTROLS.len() + 2,
            ControlRow::Sensitivity(_) => GAME_CONTROLS.len() + 3,
            ControlRow::Vertical(_) => GAME_CONTROLS.len() + 4,
            ControlRow::Rumble => GAME_CONTROLS.len() + 5,
        }
    }

    /// The row as the log and the test aids name it (`device`, `bind:Jump`,
    /// `padbind:Jump`, `invert`, `sensitivity`, `vertical`, `rumble`).
    pub fn name(self) -> String {
        match self {
            ControlRow::Device => "device".into(),
            ControlRow::AutomaticInput => "automatic-input".into(),
            ControlRow::Binding(c, InputDevice::KeyboardMouse) => format!("bind:{}", Bindings::name(c)),
            ControlRow::Binding(c, InputDevice::Controller) => format!("padbind:{}", Bindings::name(c)),
            ControlRow::Invert => "invert".into(),
            ControlRow::Sensitivity(_) => "sensitivity".into(),
            ControlRow::Vertical(_) => "vertical".into(),
            ControlRow::Rumble => "rumble".into(),
        }
    }
}

/// A row of the settings page: a registered setting or one of the Controls tab's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowId {
    /// Its index in `crate::settings::Settings`.
    Setting(usize),
    Control(ControlRow),
}

/// What the UI sounds take for a Controls row: past any setting's index.
const CONTROL_SOUND_BASE: usize = 10_000;
/// What the UI sounds take for no row (an empty tab).
pub const NO_ROW_SOUND: usize = usize::MAX;

impl RowId {
    /// The row as the log names it: a setting's key, a Controls row's [`ControlRow::name`].
    pub fn name(self, list: &[Setting]) -> String {
        match self {
            RowId::Setting(i) => list.get(i).map_or_else(|| format!("row {i}"), |s| s.key.to_string()),
            RowId::Control(c) => c.name(),
        }
    }

    /// The number the UI sounds tell rows apart by (`crate::ui_sounds`).
    pub fn sound_id(self) -> usize {
        match self {
            RowId::Setting(i) => i,
            RowId::Control(c) => CONTROL_SOUND_BASE + c.ordinal(),
        }
    }
}

/// A line of the page's list: a heading over rows, or a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewEntry {
    Heading(Placement),
    Row(RowId),
}

/// What the settings page lists: a tab, the search's results or the Changed list (#1953
/// points 5 and 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsScope {
    Tab(&'static str),
    Search,
    Changed,
}

impl Default for SettingsScope {
    fn default() -> SettingsScope {
        SettingsScope::Tab(TAB_ORDER[0])
    }
}

impl SettingsScope {
    /// Its name in English (the log's; the page translates it).
    pub fn name(self) -> &'static str {
        match self {
            SettingsScope::Tab(t) => t,
            SettingsScope::Search => SEARCH,
            SettingsScope::Changed => CHANGED,
        }
    }
}

/// The rail's first two lines [design: #1953].
pub const SEARCH: &str = "Search";
pub const CHANGED: &str = "Changed";

/// A line of the rail left of the column: Search, Changed, the tabs, then the page's
/// Defaults and Back (drawn at the bottom right, reached by the rail's keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailItem {
    Search,
    Changed,
    Tab(&'static str),
    Defaults,
    Back,
}

impl RailItem {
    /// Its name in English (the log's).
    pub fn name(self) -> &'static str {
        match self {
            RailItem::Search => SEARCH,
            RailItem::Changed => CHANGED,
            RailItem::Tab(t) => t,
            RailItem::Defaults => "Defaults",
            RailItem::Back => "Back",
        }
    }

    /// The rail's line for scope `s`.
    pub fn of(s: SettingsScope) -> RailItem {
        match s {
            SettingsScope::Tab(t) => RailItem::Tab(t),
            SettingsScope::Search => RailItem::Search,
            SettingsScope::Changed => RailItem::Changed,
        }
    }
}

/// Where the keys are on the settings page: on the rows, on the rail (F6, a
/// controller's Y), or in the search's text (typing) [design: #1953].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SettingsFocus {
    #[default]
    Rows,
    Rail(RailItem),
    SearchText,
}

/// The rail's lines: Search, Changed, every tab of `layout`, Defaults, Back.
pub fn rail(layout: &ResolvedLayout) -> Vec<RailItem> {
    let mut out = vec![RailItem::Search, RailItem::Changed];
    out.extend(layout.tab_names().into_iter().map(RailItem::Tab));
    out.extend([RailItem::Defaults, RailItem::Back]);
    out
}

/// What the game's state shows of the rows (module notes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Visibility {
    pub device: InputDevice,
    /// The HUD's and the Pip-Boy's colour are Custom (their sliders shown).
    pub colours: [bool; 2],
}

impl Visibility {
    /// As the world has it now.
    pub fn of_world(world: &World) -> Visibility {
        let device = world.get_resource::<Bindings>().map_or(InputDevice::default(), |b| b.device);
        Visibility::of(device, world.get_resource::<crate::ui_colour::CustomColours>())
    }

    /// With `device` in use and the colours' Custom choices `flags`.
    pub fn of(device: InputDevice, flags: Option<&crate::ui_colour::CustomColours>) -> Visibility {
        Visibility { device, colours: [crate::ui_colour::custom_shown(flags, 0), crate::ui_colour::custom_shown(flags, 1)] }
    }

    /// Whether registered row `s` is shown.
    pub fn shows(&self, s: &Setting) -> bool {
        if let Some((which, _)) = crate::ui_colour::channel_of(s.key) {
            return self.colours[which];
        }
        !controller_only(s.key) || self.device == InputDevice::Controller
    }
}

/// The row that stands for registered row `i` while it is hidden: its colour's list, or
/// the Input Device row for a controller's row; `None` when it is shown or has none.
pub fn parent_of(list: &[Setting], i: usize, vis: Visibility) -> Option<RowId> {
    let s = list.get(i)?;
    if vis.shows(s) {
        return None;
    }
    if let Some((_, colour)) = crate::ui_colour::channel_of(s.key) {
        return list.iter().position(|r| r.key == colour).map(RowId::Setting);
    }
    controller_only(s.key).then_some(RowId::Control(ControlRow::Device))
}

/// The tab a row is on: a setting's by the table, a Controls row's Controls.
pub fn home_tab(layout: &ResolvedLayout, id: RowId) -> &'static str {
    match id {
        RowId::Setting(i) => layout.placement_of(i).map_or(TAB_ORDER[0], |p| p.tab),
        RowId::Control(_) => CONTROLS_TAB,
    }
}

/// The rows the search found and the rows away from their default, as worked out last
/// ([`search`], [`changed`]): each the rows themselves, hidden ones too, in the page's
/// order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Found {
    pub search: Vec<RowId>,
    pub changed: Vec<RowId>,
}

/// The lines `scope` lists: a tab's headings and shown rows (the Controls tab's own rows
/// first, no heading over them); the search's results or the Changed list from `found`,
/// each row as it is shown ([`shown_as`]) under the heading of where it lives
/// ([`listing`]).
pub fn project(layout: &ResolvedLayout, list: &[Setting], scope: SettingsScope, vis: Visibility, found: &Found) -> Vec<ViewEntry> {
    let tab = match scope {
        SettingsScope::Tab(tab) => tab,
        SettingsScope::Search => return listing(layout, &shown_rows(layout, list, &found.search, vis)),
        SettingsScope::Changed => return listing(layout, &shown_rows(layout, list, &found.changed, vis)),
    };
    let mut out = Vec::new();
    if tab == CONTROLS_TAB {
        out.extend(ControlRow::rows(vis.device).into_iter().map(|c| ViewEntry::Row(RowId::Control(c))));
    }
    out.extend(layout.tab_entries(tab, |i| list.get(i).is_some_and(|s| vis.shows(s))).into_iter().map(|e| match e {
        LayoutEntry::Heading(p) => ViewEntry::Heading(p),
        LayoutEntry::Setting(i) => ViewEntry::Row(RowId::Setting(i)),
    }));
    out
}

/// The line `id` is on.
pub fn slot_of(entries: &[ViewEntry], id: RowId) -> Option<usize> {
    entries.iter().position(|e| *e == ViewEntry::Row(id))
}

/// The first row (none: nothing to choose).
pub fn first_row(entries: &[ViewEntry]) -> Option<RowId> {
    entries.iter().find_map(|e| match e {
        ViewEntry::Row(id) => Some(*id),
        ViewEntry::Heading(_) => None,
    })
}

/// The rows, in order.
fn rows(entries: &[ViewEntry]) -> Vec<RowId> {
    entries
        .iter()
        .filter_map(|e| match e {
            ViewEntry::Row(id) => Some(*id),
            ViewEntry::Heading(_) => None,
        })
        .collect()
}

/// The row `step` rows after (1) or before (-1) `selected`, round the ends, headings passed;
/// none chosen: the first (or last) row.
pub fn step_selectable(entries: &[ViewEntry], selected: Option<RowId>, step: i32) -> Option<RowId> {
    let rows = rows(entries);
    if rows.is_empty() {
        return None;
    }
    let n = rows.len() as i32;
    match selected.and_then(|s| rows.iter().position(|r| *r == s)) {
        Some(at) => Some(rows[(at as i32 + step).rem_euclid(n) as usize]),
        None if step < 0 => rows.last().copied(),
        None => rows.first().copied(),
    }
}

/// The sections: each heading (or the tab's start, without one) and its rows' lines.
fn sections(entries: &[ViewEntry]) -> Vec<(Option<Placement>, Vec<usize>)> {
    let mut out: Vec<(Option<Placement>, Vec<usize>)> = Vec::new();
    for (k, e) in entries.iter().enumerate() {
        match e {
            ViewEntry::Heading(p) => out.push((Some(*p), Vec::new())),
            ViewEntry::Row(_) => match out.last_mut() {
                Some((_, rows)) => rows.push(k),
                None => out.push((None, vec![k])),
            },
        }
    }
    out.retain(|(_, rows)| !rows.is_empty());
    out
}

/// Page Down (`step` 1) or Page Up (-1): the first row under the heading after or before
/// `selected`'s, round the ends, and that heading (`None`: a section without one). A tab
/// without headings is one section: its first row. Nothing to choose: `None`.
pub fn step_heading(entries: &[ViewEntry], selected: Option<RowId>, step: i32) -> Option<(RowId, Option<Placement>)> {
    let secs = sections(entries);
    if secs.is_empty() {
        return None;
    }
    let n = secs.len() as i32;
    let slot = selected.and_then(|s| slot_of(entries, s));
    let to = match slot.and_then(|k| secs.iter().position(|(_, rows)| rows.contains(&k))) {
        Some(at) => (at as i32 + step).rem_euclid(n) as usize,
        None if step < 0 => secs.len() - 1,
        None => 0,
    };
    let (heading, rows) = &secs[to];
    match entries[rows[0]] {
        ViewEntry::Row(id) => Some((id, *heading)),
        ViewEntry::Heading(_) => None,
    }
}

/// The heading over `id` (`None`: a section without one).
pub fn heading_of(entries: &[ViewEntry], id: RowId) -> Option<Placement> {
    let k = slot_of(entries, id)?;
    entries[..k].iter().rev().find_map(|e| match e {
        ViewEntry::Heading(p) => Some(*p),
        ViewEntry::Row(_) => None,
    })
}

/// The first line shown of `entries` from `first`, `shown` (at least 1) at once, moved as
/// little as keeps the chosen line `selected` in view, and its heading too when it is the
/// heading's first row and the window has room for both.
pub fn follow(entries: &[ViewEntry], first: usize, selected: Option<usize>, shown: usize) -> usize {
    let shown = shown.max(1);
    let n = entries.len();
    let last = n.saturating_sub(shown);
    let Some(s) = selected.filter(|s| *s < n) else { return first.min(last) };
    let mut first = crate::menu::model::keep_in_view(first, s, n, shown);
    if shown > 1 && s > 0 && matches!(entries[s - 1], ViewEntry::Heading(_)) {
        first = first.min(s - 1);
    }
    first.min(last)
}

/// The window's first line this frame, from `first`: when the choice is new since the
/// window last followed it (`followed`), as [`follow`] puts it (its heading too); else
/// only as far as keeps the chosen line in view, so a window the wheel or the scroll bar
/// moved stays where they put it and is not pulled back to the heading over the chosen
/// row [design: #1953].
pub fn window_first(entries: &[ViewEntry], first: usize, selected: Option<RowId>, followed: Option<RowId>, shown: usize) -> usize {
    let shown = shown.max(1);
    let at = selected.and_then(|id| slot_of(entries, id));
    if selected != followed {
        return follow(entries, first, at, shown);
    }
    let n = entries.len();
    let last = n.saturating_sub(shown);
    match at.filter(|s| *s < n) {
        Some(s) => crate::menu::model::keep_in_view(first, s, n, shown).min(last),
        None => first.min(last),
    }
}

/// The lines drawn of a window of `shown` (at least 1) from `first`, less a heading on the
/// last line (it is drawn with its first row in the next window) unless it is the window's
/// only line.
pub fn drawn_slots(entries: &[ViewEntry], first: usize, shown: usize) -> std::ops::Range<usize> {
    let first = first.min(entries.len());
    let mut end = (first + shown.max(1)).min(entries.len());
    if end > first + 1 && matches!(entries[end - 1], ViewEntry::Heading(_)) {
        end -= 1;
    }
    first..end
}

/// The first line of a window of `shown` lines the wheel or the scroll bar moved to `first`
/// (`step`: the way it moved, down for 0): a one-line window that would show only a heading
/// goes on past it to the row beyond (else back to the row before), so the window always
/// draws a row the choice can move to and the wheel crosses headings [design: #2377].
pub fn land(entries: &[ViewEntry], first: usize, step: i64, shown: usize) -> usize {
    let is_heading = |k: usize| matches!(entries.get(k), Some(ViewEntry::Heading(_)));
    if shown > 1 || !is_heading(first) {
        return first;
    }
    let ahead = if step < 0 { first.checked_sub(1) } else { Some(first + 1) };
    let back = if step < 0 { Some(first + 1) } else { first.checked_sub(1) };
    [ahead, back].into_iter().flatten().find(|k| *k < entries.len() && !is_heading(*k)).unwrap_or(first)
}

/// The chosen row once the window moved to `first` (the wheel, the scroll bar): itself when
/// still drawn, else the drawn row nearest to it.
/// `shown`: the window's lines.
pub fn clamp_into_window(entries: &[ViewEntry], first: usize, selected: Option<RowId>, shown: usize) -> Option<RowId> {
    let drawn = drawn_slots(entries, first, shown);
    let at = selected.and_then(|s| slot_of(entries, s));
    if at.is_some_and(|k| drawn.contains(&k)) {
        return selected;
    }
    let shown: Vec<(usize, RowId)> = drawn
        .filter_map(|k| match entries[k] {
            ViewEntry::Row(id) => Some((k, id)),
            ViewEntry::Heading(_) => None,
        })
        .collect();
    if shown.is_empty() {
        // A window of one line on a heading: no row to move the choice to.
        return selected;
    }
    match at {
        // Above the window: its top row; below it: its bottom row.
        Some(k) if shown.first().is_some_and(|(f, _)| k < *f) => shown.first().map(|r| r.1),
        Some(_) => shown.last().map(|r| r.1),
        None => shown.first().map(|r| r.1),
    }
}

/// The settings page's state made whole for `layout`: on a tab, the tab of the chosen row
/// (a test aid or a search may choose one on another tab); the chosen row itself when
/// listed, else the row that stands for it (hidden, [`parent_of`]; a Controls row of the
/// other device, that row of the device in use), else the first row; and the lines listed.
pub fn normalize(layout: &ResolvedLayout, list: &[Setting], scope: SettingsScope, selected: Option<RowId>, vis: Visibility, found: &Found) -> (SettingsScope, Option<RowId>, Vec<ViewEntry>) {
    let scope = match (scope, selected) {
        (SettingsScope::Tab(_), Some(id)) => SettingsScope::Tab(home_tab(layout, id)),
        (SettingsScope::Tab(t), None) if layout.tab(t).is_none() => SettingsScope::default(),
        (s, _) => s,
    };
    let entries = project(layout, list, scope, vis, found);
    let stand_in = selected.and_then(|id| match id {
        RowId::Setting(i) => parent_of(list, i, vis),
        RowId::Control(ControlRow::Binding(c, _)) => Some(RowId::Control(ControlRow::Binding(c, vis.device))),
        RowId::Control(ControlRow::Sensitivity(_)) => Some(RowId::Control(ControlRow::Sensitivity(vis.device))),
        RowId::Control(ControlRow::Vertical(_)) => Some(RowId::Control(ControlRow::Vertical(vis.device))),
        RowId::Control(ControlRow::Rumble) => Some(RowId::Control(ControlRow::Device)),
        RowId::Control(_) => None,
    });
    let listed = |id: &RowId| slot_of(&entries, *id).is_some();
    let selected = selected.filter(listed).or(stand_in.filter(listed)).or_else(|| first_row(&entries));
    (scope, selected, entries)
}

/// The row that shows `id` where every row is listed with `vis` (the search, the Changed
/// list): itself when shown, else the row that stands for it ([`parent_of`]; a control's
/// binding or the sensitivity of the device not in use, and Rumble without a controller:
/// the Input Device row, which switches the device) [design: #1953]; `None`: neither.
pub fn shown_as(list: &[Setting], id: RowId, vis: Visibility) -> Option<RowId> {
    match id {
        RowId::Setting(i) if list.get(i).is_some_and(|s| vis.shows(s)) => Some(id),
        RowId::Setting(i) => parent_of(list, i, vis),
        RowId::Control(ControlRow::Binding(_, d) | ControlRow::Sensitivity(d) | ControlRow::Vertical(d)) if d != vis.device => Some(RowId::Control(ControlRow::Device)),
        RowId::Control(ControlRow::Rumble) if vis.device != InputDevice::Controller => Some(RowId::Control(ControlRow::Device)),
        RowId::Control(_) => Some(id),
    }
}

/// `ids` each as it is shown ([`shown_as`]), each once, in the page's order of the rows
/// shown ([`catalogue`]).
fn shown_rows(layout: &ResolvedLayout, list: &[Setting], ids: &[RowId], vis: Visibility) -> Vec<RowId> {
    let mut out: Vec<RowId> = Vec::new();
    for id in ids.iter().filter_map(|id| shown_as(list, *id, vis)) {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    let order = catalogue(layout);
    out.sort_by_key(|id| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
    out
}

/// Of `ids` (the search's or the Changed list's rows), those listed as another row
/// ([`shown_as`]), by the row that stands for them, in their order: what the page names
/// under that row so it is plain what a choice there is about [design: #1953].
pub fn stands_for(list: &[Setting], ids: &[RowId], vis: Visibility) -> Vec<(RowId, Vec<RowId>)> {
    let mut out: Vec<(RowId, Vec<RowId>)> = Vec::new();
    for &id in ids {
        let Some(by) = shown_as(list, id, vis).filter(|by| *by != id) else { continue };
        match out.iter_mut().find(|(b, _)| *b == by) {
            Some((_, v)) => v.push(id),
            None => out.push((by, vec![id])),
        }
    }
    out
}

/// Where row `id` lives: a setting's place by the table, a Controls row's the Controls tab.
pub fn placement(layout: &ResolvedLayout, id: RowId) -> Placement {
    match id {
        RowId::Setting(i) => layout.placement_of(i).unwrap_or(Placement::tab(TAB_ORDER[0])),
        RowId::Control(_) => Placement::tab(CONTROLS_TAB),
    }
}

/// Every row in the page's order: tab by tab, section by section, the Controls tab's own
/// rows (of both devices, [`ControlRow::all`]) first on it [design: #1953].
pub fn catalogue(layout: &ResolvedLayout) -> Vec<RowId> {
    let mut out = Vec::new();
    for t in &layout.tabs {
        if t.name == CONTROLS_TAB {
            out.extend(ControlRow::all().into_iter().map(RowId::Control));
        }
        out.extend(t.sections.iter().flat_map(|s| s.rows.iter().map(|&i| RowId::Setting(i))));
    }
    out
}

/// Rows of several tabs as lines: before each run of rows that live in one place, a
/// heading saying where (its tab and heading: `Gameplay > Dialogue`, `Controls`)
/// [design: #1953].
pub fn listing(layout: &ResolvedLayout, ids: &[RowId]) -> Vec<ViewEntry> {
    let mut out = Vec::new();
    let mut at: Option<Placement> = None;
    for &id in ids {
        let p = placement(layout, id);
        if at != Some(p) {
            out.push(ViewEntry::Heading(p));
            at = Some(p);
        }
        out.push(ViewEntry::Row(id));
    }
    out
}

/// The query's terms: split at whitespace, lowercased (Unicode) [design: #1953].
pub fn search_terms(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// A row's words to search, lowercased, a line each so no term spans two.
pub fn haystack(parts: &[&str]) -> String {
    parts.join("\n").to_lowercase()
}

/// A registered row's words: its label and note in the language shown and in English, its
/// key as the INI keeps it (a tweak's note names its Stewie's Tweaks key) [design: #1953]
/// and its command-line switches (`--msaa` finds Anti-aliasing, #2377).
pub fn setting_haystack(s: &Setting) -> String {
    haystack(&[tr(s.label), s.label, tr(s.note), s.note, s.key, s.switches])
}

/// Whether `hay` holds every term somewhere (terms may match different words; no
/// stemming, no ranking) [design: #1953].
pub fn matches(hay: &str, terms: &[String]) -> bool {
    terms.iter().all(|t| hay.contains(t.as_str()))
}

/// The rows the search finds for `query`, in the page's order ([`catalogue`]): every row
/// whose words ([`setting_haystack`]; a Controls row's from `control_words`) hold each
/// term; an empty query, every row. Hidden rows are found too; [`project`] lists them as
/// the row that stands for them.
pub fn search(layout: &ResolvedLayout, list: &[Setting], query: &str, control_words: impl Fn(ControlRow) -> String) -> Vec<RowId> {
    let terms = search_terms(query);
    catalogue(layout)
        .into_iter()
        .filter(|id| {
            terms.is_empty()
                || match id {
                    RowId::Setting(i) => list.get(*i).is_some_and(|s| matches(&setting_haystack(s), &terms)),
                    RowId::Control(c) => matches(&control_words(*c).to_lowercase(), &terms),
                }
        })
        .collect()
}

/// The rows away from their default, in the page's order: the registered rows `settings`
/// names (indices, `crate::settings::changed_rows`) and the Controls rows `control_changed`
/// says so of (`crate::bindings::is_default`) [design: #1953].
pub fn changed(layout: &ResolvedLayout, settings: &[usize], control_changed: impl Fn(ControlRow) -> bool) -> Vec<RowId> {
    catalogue(layout)
        .into_iter()
        .filter(|id| match id {
            RowId::Setting(i) => settings.contains(i),
            RowId::Control(c) => control_changed(*c),
        })
        .collect()
}

/// The choice once a list lost the chosen row (a Changed row put back): the first row after
/// it in `before` still listed `now`, else the nearest before it; else as chosen.
pub fn reselect(before: &[ViewEntry], now: &[ViewEntry], selected: Option<RowId>) -> Option<RowId> {
    let id = selected?;
    if slot_of(now, id).is_some() {
        return selected;
    }
    let old = rows(before);
    let Some(k) = old.iter().position(|r| *r == id) else { return selected };
    let new = rows(now);
    old[k + 1..].iter().find(|r| new.contains(r)).or_else(|| old[..k].iter().rev().find(|r| new.contains(r))).copied().or(selected)
}

/// The table applied to the registry, kept until a row is added ([`resolve_layout`]).
#[derive(Resource, Default)]
pub struct SettingsLayout {
    rows: Option<usize>,
    pub layout: ResolvedLayout,
}

impl SettingsLayout {
    /// The layout of `list`, worked out again when the registry grew.
    pub fn refresh(&mut self, list: &[Setting]) -> &ResolvedLayout {
        if self.rows != Some(list.len()) {
            self.layout = resolve_layout(list);
            self.rows = Some(list.len());
        }
        &self.layout
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    const NONE: Found = Found { search: Vec::new(), changed: Vec::new() };

    fn registry(rows: &[(&'static str, &'static str)]) -> Vec<Setting> {
        let mut s = Settings::default();
        for (key, group) in rows {
            s.add(Setting { key, group, label: key, note: "", switches: "", get: |_| false, set: |_, _| {}, slider: None });
        }
        s.list().to_vec()
    }

    const ROWS: [(&str, &str); 12] = [
        ("difficulty", "Gameplay"),
        ("hardcore", "Gameplay"),
        ("menu-pause", "Gameplay"),
        ("head-bob", "Display"),
        ("autosave-travel", "Saving"),
        ("autosave-wait", "Saving"),
        ("hud-colour", "Interface"),
        ("hud-red", "Interface"),
        ("hud-green", "Interface"),
        ("hud-blue", "Interface"),
        ("button-style", "Controller"),
        ("god-mode", "Debug"),
    ];

    fn key(list: &[Setting], k: &str) -> RowId {
        RowId::Setting(list.iter().position(|s| s.key == k).unwrap())
    }

    fn names(list: &[Setting], entries: &[ViewEntry]) -> Vec<String> {
        entries
            .iter()
            .map(|e| match e {
                ViewEntry::Heading(p) => format!("[{}]", p.heading.unwrap_or(p.tab)),
                ViewEntry::Row(id) => id.name(list),
            })
            .collect()
    }

    #[test]
    fn a_tab_lists_its_headings_over_its_shown_rows() {
        let list = registry(&ROWS);
        let l = resolve_layout(&list);
        let vis = Visibility::default();
        let g = project(&l, &list, SettingsScope::Tab("Gameplay"), vis, &NONE);
        assert_eq!(names(&list, &g), ["[Game]", "difficulty", "hardcore", "[Saving]", "autosave-travel", "autosave-wait", "[Dialogue]", "menu-pause", "[Camera]", "head-bob"]);
        // The colour's sliders only for Custom, the HUD's alone.
        let d = |colours| names(&list, &project(&l, &list, SettingsScope::Tab("Display"), Visibility { colours, ..vis }, &NONE));
        assert_eq!(d([false, false]), ["[Interface]", "hud-colour"]);
        assert_eq!(d([true, false]), ["[Interface]", "hud-colour", "hud-red", "hud-green", "hud-blue"]);
        assert_eq!(d([false, true]), ["[Interface]", "hud-colour"]);
        // The Controls tab: its own rows first, the controller's rows with a controller only.
        let c = |device| names(&list, &project(&l, &list, SettingsScope::Tab(CONTROLS_TAB), Visibility { device, ..vis }, &NONE));
        let kb = c(InputDevice::KeyboardMouse);
        assert_eq!(kb.len(), GAME_CONTROLS.len() + 5);
        assert_eq!((kb[0].as_str(), kb[1].as_str(), kb[2].as_str(), kb.last().unwrap().as_str()), ("device", "automatic-input", "bind:Forward", "vertical"));
        let pad = c(InputDevice::Controller);
        assert_eq!(pad[GAME_CONTROLS.len() + 2..], ["invert", "sensitivity", "vertical", "rumble", "button-style"]);
        assert!(pad[2].starts_with("padbind:"));
        // An empty tab, and a search that found nothing: nothing.
        assert!(project(&l, &list, SettingsScope::Tab("Tweaks"), vis, &NONE).is_empty());
        assert!(project(&l, &list, SettingsScope::Search, vis, &NONE).is_empty());
    }

    #[test]
    fn up_and_down_pass_the_headings_round_the_ends() {
        let list = registry(&ROWS);
        let l = resolve_layout(&list);
        let g = project(&l, &list, SettingsScope::Tab("Gameplay"), Visibility::default(), &NONE);
        let k = |n| key(&list, n);
        assert_eq!(step_selectable(&g, Some(k("hardcore")), 1), Some(k("autosave-travel")));
        assert_eq!(step_selectable(&g, Some(k("autosave-travel")), -1), Some(k("hardcore")));
        assert_eq!(step_selectable(&g, Some(k("head-bob")), 1), Some(k("difficulty")), "round the end");
        assert_eq!(step_selectable(&g, Some(k("difficulty")), -1), Some(k("head-bob")));
        assert_eq!(step_selectable(&g, None, 1), Some(k("difficulty")));
        assert_eq!(step_selectable(&[], None, 1), None, "an empty tab");
        let one = [ViewEntry::Heading(Placement::tab("x")), ViewEntry::Row(k("god-mode"))];
        assert_eq!(step_selectable(&one, Some(k("god-mode")), 1), Some(k("god-mode")));
    }

    #[test]
    fn page_up_and_down_go_from_heading_to_heading() {
        let list = registry(&ROWS);
        let l = resolve_layout(&list);
        let g = project(&l, &list, SettingsScope::Tab("Gameplay"), Visibility::default(), &NONE);
        let k = |n| key(&list, n);
        let to = |from: &str, step| step_heading(&g, Some(key(&list, from)), step).map(|(id, p)| (id.name(&list), p.and_then(|p| p.heading)));
        assert_eq!(to("difficulty", 1), Some(("autosave-travel".into(), Some("Saving"))));
        assert_eq!(to("autosave-wait", 1), Some(("menu-pause".into(), Some("Dialogue"))));
        assert_eq!(to("menu-pause", -1), Some(("autosave-travel".into(), Some("Saving"))));
        assert_eq!(to("head-bob", 1), Some(("difficulty".into(), Some("Game"))), "round the end");
        assert_eq!(to("difficulty", -1), Some(("head-bob".into(), Some("Camera"))));
        // A tab without headings is one section: its first row; an empty one, nothing.
        let debug = project(&l, &list, SettingsScope::Tab("Debug"), Visibility::default(), &NONE);
        assert_eq!(step_heading(&debug, Some(k("god-mode")), 1), Some((k("god-mode"), None)));
        assert_eq!(step_heading(&[], None, 1), None);
        // A section whose rows are all hidden has no heading to stop at.
        let d = project(&l, &list, SettingsScope::Tab("Display"), Visibility::default(), &NONE);
        assert_eq!(heading_of(&d, k("hud-colour")).and_then(|p| p.heading), Some("Interface"));
        assert_eq!(step_heading(&d, Some(k("hud-colour")), 1), Some((k("hud-colour"), Some(heading_of(&d, k("hud-colour")).unwrap()))));
    }

    #[test]
    fn the_window_keeps_the_choice_and_its_heading_and_never_ends_on_a_heading() {
        let h = |t| ViewEntry::Heading(Placement { tab: "T", subgroup: None, heading: Some(t) });
        let r = |i| ViewEntry::Row(RowId::Setting(i));
        // [A] 0..7, [B] 7..12: the heading B is on line 9, the window's 10th.
        let mut e = vec![h("A")];
        e.extend((0..8).map(r));
        e.push(h("B"));
        e.extend((8..12).map(r));
        assert_eq!(e.len(), 14);
        // From the top: line 9 (B) would be the last of a window from 1; it is left out.
        assert_eq!(drawn_slots(&e, 1, 9), 1..9);
        assert_eq!(drawn_slots(&e, 0, 9), 0..9);
        // Down onto B's first row (line 10): the window shows B too.
        let first = follow(&e, 0, Some(10), 9);
        assert!(first <= 9 && first + 9 > 10, "{first}");
        // Up to A's first row (line 1): A's heading comes back into view.
        assert_eq!(follow(&e, 5, Some(1), 9), 0);
        // Never past the last full window.
        assert_eq!(follow(&e, 20, None, 9), e.len() - 9);
        // The wheel moved the window off the choice: the nearest drawn row is chosen.
        assert_eq!(clamp_into_window(&e, 3, Some(RowId::Setting(0)), 9), Some(RowId::Setting(2)));
        assert_eq!(clamp_into_window(&e, 0, Some(RowId::Setting(11)), 9), Some(RowId::Setting(7)));
        assert_eq!(clamp_into_window(&e, 0, Some(RowId::Setting(3)), 9), Some(RowId::Setting(3)));
    }

    /// #2377: the window works with any number of lines, the layout's: every row can be
    /// followed into view (with its heading where there is room for both), the window never
    /// passes the last full one, never ends on a heading unless that is its only line, and
    /// the wheel's window always has the choice drawn.
    #[test]
    fn the_window_works_with_any_number_of_lines() {
        let h = |t| ViewEntry::Heading(Placement { tab: "T", subgroup: None, heading: Some(t) });
        let r = |i| ViewEntry::Row(RowId::Setting(i));
        // [A] 0..5, [B] 5..10, [C] 10..15: 18 lines.
        let mut e = Vec::new();
        for (k, name) in ["A", "B", "C"].into_iter().enumerate() {
            e.push(h(name));
            e.extend((k * 5..k * 5 + 5).map(r));
        }
        let short = vec![h("A"), r(0), r(1)];
        let rows_only: Vec<ViewEntry> = (0..4).map(r).collect();
        for shown in [1, 2, 9, 13, 30] {
            for list in [&e, &short, &rows_only] {
                let n = list.len();
                let last = n.saturating_sub(shown);
                for (s, entry) in list.iter().enumerate() {
                    let ViewEntry::Row(id) = *entry else { continue };
                    for first in 0..n + 3 {
                        let f = follow(list, first, Some(s), shown);
                        let drawn = drawn_slots(list, f, shown);
                        assert!(f <= last, "{shown} lines: {f} past the last full window {last}");
                        assert!(drawn.contains(&s), "{shown} lines from {first}: row {s} not drawn in {drawn:?}");
                        if shown > 1 && s > 0 && matches!(list[s - 1], ViewEntry::Heading(_)) {
                            assert!(drawn.contains(&(s - 1)), "{shown} lines: the heading over the heading's first row {s}");
                        }
                        // A new choice is followed as `follow` does; the same one only kept in view.
                        assert_eq!(window_first(list, first, Some(id), None, shown), f);
                        let kept = window_first(list, first, Some(id), Some(id), shown);
                        assert!(drawn_slots(list, kept, shown).contains(&s) && kept <= last);
                    }
                }
                for first in 0..=last {
                    let drawn = drawn_slots(list, first, shown);
                    assert!(drawn.len() <= shown && drawn.start == first.min(n));
                    if drawn.len() > 1 {
                        assert!(matches!(list[drawn.end - 1], ViewEntry::Row(_)), "{shown} lines from {first}: ends on a heading");
                    }
                    // The wheel put the window here (landing past a heading a one-line
                    // window would show alone): the choice is a drawn row.
                    for step in [-1, 1] {
                        let landed = land(list, first, step, shown);
                        let picked = clamp_into_window(list, landed, Some(RowId::Setting(0)), shown);
                        let k = picked.and_then(|id| slot_of(list, id)).expect("a row");
                        assert!(drawn_slots(list, landed, shown).contains(&k), "{shown} lines from {first} ({step}): {picked:?}");
                    }
                }
            }
        }
        // One line: the row, not its heading; two: both.
        assert_eq!(follow(&e, 0, Some(7), 1), 7);
        assert_eq!(follow(&e, 0, Some(7), 2), 6);
        assert_eq!(drawn_slots(&e, 6, 1), 6..7, "a heading alone on a one-line window");
        // The wheel crosses a heading in a one-line window: [A], a0, [B], b0 (a list of
        // [row, heading, row] after the first heading).
        let ab = vec![h("A"), r(0), h("B"), r(5)];
        let down = land(&ab, 2, 1, 1);
        assert_eq!((down, clamp_into_window(&ab, down, Some(RowId::Setting(0)), 1)), (3, Some(RowId::Setting(5))), "down from a0 past [B] onto b0");
        let up = land(&ab, 2, -1, 1);
        assert_eq!((up, clamp_into_window(&ab, up, Some(RowId::Setting(5)), 1)), (1, Some(RowId::Setting(0))), "up from b0 past [B] onto a0");
        assert_eq!(land(&ab, 0, -1, 1), 1, "the top heading: on to the row after it");
        assert_eq!(land(&ab, 2, 1, 2), 2, "two lines show the heading with its row");
        assert_eq!(land(&[h("A")], 0, 1, 1), 0, "a heading alone: nowhere to go");
        // Thirteen lines: from the top, line 12 (C) would be last; it is left out.
        assert_eq!(drawn_slots(&e, 0, 13), 0..12);
        assert_eq!(follow(&e, 0, Some(13), 13), 1, "C's first row brings C into view: the window from 1, C on its 12th line");
        // More lines than the list: all of it from the top.
        assert_eq!(follow(&e, 4, Some(17), 30), 0);
        assert_eq!(drawn_slots(&e, 0, 30), 0..18);
        // An empty list.
        assert_eq!(follow(&[], 3, None, 13), 0);
        assert_eq!(drawn_slots(&[], 0, 13), 0..0);
        assert_eq!(clamp_into_window(&[], 0, None, 13), None);
        assert_eq!(window_first(&[], 2, None, None, 13), 0);
        // A zero is taken as one line, never a window of none.
        assert_eq!(drawn_slots(&e, 1, 0), 1..2);
        assert_eq!(follow(&e, 0, Some(7), 0), 7);
    }

    #[test]
    fn the_wheel_moves_a_window_with_headings_and_the_choice_into_it() {
        let h = |t| ViewEntry::Heading(Placement { tab: "T", subgroup: None, heading: Some(t) });
        let r = |i| ViewEntry::Row(RowId::Setting(i));
        // [A] 0..5, [B] 5..10, [C] 10..15: 18 lines, as a tab with headings.
        let mut e = Vec::new();
        for (k, name) in ["A", "B", "C"].into_iter().enumerate() {
            e.push(h(name));
            e.extend((k * 5..k * 5 + 5).map(r));
        }
        let first_row = Some(RowId::Setting(0));
        // A notch down: the window from line 1, the first row (line 1) still in it.
        assert_eq!(clamp_into_window(&e, 1, first_row, 9), first_row);
        // The next frame keeps it there: the choice did not change, so its heading is not
        // brought back (the window followed this choice before).
        assert_eq!(window_first(&e, 1, first_row, first_row, 9), 1);
        // A new choice is followed with its heading.
        assert_eq!(window_first(&e, 1, first_row, None, 9), 0);
        // Two notches: the first row leaves the window, the choice is its top row.
        assert_eq!(clamp_into_window(&e, 2, first_row, 9), Some(RowId::Setting(1)));
        assert_eq!(window_first(&e, 2, Some(RowId::Setting(1)), Some(RowId::Setting(1)), 9), 2);
        // The window's top on a heading: the choice is the row under it, not the heading.
        assert_eq!(clamp_into_window(&e, 6, Some(RowId::Setting(1)), 9), Some(RowId::Setting(5)));
        assert_eq!(window_first(&e, 6, Some(RowId::Setting(5)), Some(RowId::Setting(5)), 9), 6);
        // Never past the last full window; a choice below the window is still kept in view.
        assert_eq!(window_first(&e, 30, None, None, 9), e.len() - 9);
        assert_eq!(window_first(&e, 0, Some(RowId::Setting(14)), Some(RowId::Setting(14)), 9), e.len() - 9);
    }

    #[test]
    fn a_hidden_or_missing_choice_moves_to_the_row_that_stands_for_it() {
        let list = registry(&ROWS);
        let l = resolve_layout(&list);
        let k = |n| key(&list, n);
        let vis = Visibility::default();
        // A test aid chooses a hidden slider on another tab: its colour, on its tab.
        let (scope, sel, _) = normalize(&l, &list, SettingsScope::Tab("Gameplay"), Some(k("hud-red")), vis, &NONE);
        assert_eq!((scope, sel), (SettingsScope::Tab("Display"), Some(k("hud-colour"))));
        // Shown: itself.
        let (_, sel, _) = normalize(&l, &list, SettingsScope::Tab("Display"), Some(k("hud-red")), Visibility { colours: [true, false], ..vis }, &NONE);
        assert_eq!(sel, Some(k("hud-red")));
        // A controller's row with keyboard and mouse: the Input Device row.
        let (scope, sel, _) = normalize(&l, &list, SettingsScope::Tab("Gameplay"), Some(k("button-style")), vis, &NONE);
        assert_eq!((scope, sel), (SettingsScope::Tab(CONTROLS_TAB), Some(RowId::Control(ControlRow::Device))));
        // A control's controller button once the keyboard is chosen: its key.
        let jump = RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::Controller));
        let (_, sel, _) = normalize(&l, &list, SettingsScope::Tab(CONTROLS_TAB), Some(jump), vis, &NONE);
        assert_eq!(sel, Some(RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::KeyboardMouse))));
        // Nothing chosen: the first row; an empty tab keeps none.
        let (_, sel, _) = normalize(&l, &list, SettingsScope::Tab("Gameplay"), None, vis, &NONE);
        assert_eq!(sel, Some(k("difficulty")));
        let (scope, sel, entries) = normalize(&l, &list, SettingsScope::Tab("Tweaks"), None, vis, &NONE);
        assert_eq!((scope, sel, entries.len()), (SettingsScope::Tab("Tweaks"), None, 0));
        assert_eq!(parent_of(&list, list.iter().position(|s| s.key == "god-mode").unwrap(), vis), None, "shown: none");
    }

    #[test]
    fn each_row_has_a_name_and_a_sound_of_its_own() {
        let list = registry(&ROWS);
        let all: Vec<RowId> = ControlRow::rows(InputDevice::Controller).into_iter().map(RowId::Control).chain((0..list.len()).map(RowId::Setting)).collect();
        let mut sounds: Vec<usize> = all.iter().map(|r| r.sound_id()).collect();
        sounds.sort_unstable();
        sounds.dedup();
        assert_eq!(sounds.len(), all.len());
        assert!(!sounds.contains(&NO_ROW_SOUND));
        assert_eq!(RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::KeyboardMouse)).name(&list), "bind:Jump");
        assert_eq!(RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::Controller)).name(&list), "padbind:Jump");
        assert_eq!(key(&list, "god-mode").name(&list), "god-mode");
        let l = resolve_layout(&list);
        assert_eq!(rail(&l).len(), 2 + TAB_ORDER.len() + 2);
    }

    /// A registry with the rows' own words: key, group, label, note, switches.
    fn worded(rows: &[(&'static str, &'static str, &'static str, &'static str, &'static str)]) -> Vec<Setting> {
        let mut s = Settings::default();
        for (key, group, label, note, switches) in rows {
            s.add(Setting { key, group, label, note, switches, get: |_| false, set: |_, _| {}, slider: None });
        }
        s.list().to_vec()
    }

    const WORDED: [(&str, &str, &str, &str, &str); 6] = [
        ("difficulty", "Gameplay", "Difficulty", "How hard the fights are.", ""),
        ("menu-pause", "Gameplay", "Freeze the picture in dialogue and menus", "While the inventory is open, animations stand still; the world lives on otherwise.", "--menu-pause, --still-world"),
        ("hud-colour", "Interface", "HUD colour", "The colour of the HUD.", ""),
        ("hud-red", "Interface", "HUD red", "Its red.", ""),
        ("button-style", "Controller", "Button style", "The controller's buttons.", ""),
        ("god-mode", "Debug", "God mode", "Nothing hurts the player.", ""),
    ];

    /// What the search lists for `query` with `vis`, as names (a Controls row's words: its name).
    fn searched(list: &[Setting], query: &str, vis: Visibility) -> Vec<String> {
        let l = resolve_layout(list);
        let found = Found { search: search(&l, list, query, |c| c.name()), changed: Vec::new() };
        names(list, &project(&l, list, SettingsScope::Search, vis, &found))
    }

    #[test]
    fn the_search_finds_rows_by_label_note_or_key_in_every_tab() {
        let list = worded(&WORDED);
        let vis = Visibility::default();
        // The INI key, a label's word, a note's words (each anywhere), any case and spacing.
        assert_eq!(searched(&list, "menu-pause", vis), ["[Dialogue]", "menu-pause"]);
        assert_eq!(searched(&list, "FREEZE", vis), ["[Dialogue]", "menu-pause"]);
        assert_eq!(searched(&list, "  inventory   animations ", vis), ["[Dialogue]", "menu-pause"]);
        assert_eq!(searched(&list, "freeze lives", vis), ["[Dialogue]", "menu-pause"], "terms in the label and the note");
        // A row's command-line switches, though the page does not show them (#2377).
        assert_eq!(searched(&list, "--Still-World", vis), ["[Dialogue]", "menu-pause"]);
        assert_eq!(searched(&list, "--menu-pause", vis), ["[Dialogue]", "menu-pause"]);
        assert!(searched(&list, "freeze nothing", vis).is_empty(), "every term must be found");
        assert!(searched(&list, "zzz", vis).is_empty());
        // Rows of several tabs, in the page's order, each under where it lives.
        let l = resolve_layout(&list);
        let found = Found { search: search(&l, &list, "the", |_| String::new()), changed: Vec::new() };
        let e = project(&l, &list, SettingsScope::Search, vis, &found);
        let heads: Vec<String> = e.iter().filter_map(|e| if let ViewEntry::Heading(p) = e { Some(p.path()) } else { None }).collect();
        assert_eq!(heads, ["Gameplay > Game", "Gameplay > Dialogue", "Display > Interface", "Controls", "Debug"]);
        // A hidden colour channel is found as its colour, once; a controller's row as the Input Device row.
        assert_eq!(searched(&list, "hud", vis), ["[Interface]", "hud-colour"]);
        assert_eq!(searched(&list, "hud", Visibility { colours: [true, false], ..vis }), ["[Interface]", "hud-colour", "hud-red"]);
        assert_eq!(searched(&list, "button-style", vis), ["[Controls]", "device"]);
        // A Controls row is found by its words; a control of the device not in use as the
        // Input Device row, first on the tab.
        assert_eq!(searched(&list, "bind:jump", vis), ["[Controls]", "device", "bind:Jump"]);
        assert_eq!(searched(&list, "padbind:jump", vis), ["[Controls]", "device"]);
        assert_eq!(searched(&list, "jump", Visibility { device: InputDevice::Controller, ..vis }), ["[Controls]", "device", "padbind:Jump"]);
        // An empty query: every row.
        let all = search(&l, &list, " ", |_| String::new());
        assert_eq!(all.len(), list.len() + ControlRow::all().len());
        assert_eq!(all, catalogue(&l));
        assert_eq!(search_terms(" Ab  CD "), ["ab", "cd"]);
        assert!(matches(&haystack(&["One", "two"]), &search_terms("TWO one")));
        assert!(!matches(&haystack(&["one", "two"]), &search_terms("etw")), "no term spans two words' lines");
    }

    #[test]
    fn the_changed_list_names_where_each_row_lives() {
        let list = worded(&WORDED);
        let l = resolve_layout(&list);
        let k = |n| key(&list, n);
        let RowId::Setting(god) = k("god-mode") else { unreachable!() };
        let RowId::Setting(red) = k("hud-red") else { unreachable!() };
        let RowId::Setting(diff) = k("difficulty") else { unreachable!() };
        let rows = changed(&l, &[god, red, diff], |c| c == ControlRow::Rumble);
        // In the page's order, hidden rows too.
        assert_eq!(rows, [k("difficulty"), k("hud-red"), RowId::Control(ControlRow::Rumble), k("god-mode")]);
        let found = Found { search: Vec::new(), changed: rows };
        let e = project(&l, &list, SettingsScope::Changed, Visibility::default(), &found);
        assert_eq!(names(&list, &e), ["[Game]", "difficulty", "[Interface]", "hud-colour", "[Controls]", "device", "[Debug]", "god-mode"]);
        assert_eq!(heading_of(&e, k("hud-colour")).map(|p| p.path()).as_deref(), Some("Display > Interface"));
        // LB/RB go from place to place.
        assert_eq!(step_heading(&e, Some(k("difficulty")), 1).map(|(id, _)| id), Some(k("hud-colour")));
        assert!(project(&l, &list, SettingsScope::Changed, Visibility::default(), &NONE).is_empty());
    }

    #[test]
    fn a_change_of_the_device_not_in_use_is_listed_as_the_input_device_row() {
        // Keyboard and mouse in use; only the controller's Jump button and the stick's
        // sensitivity changed: the Changed list shows the Input Device row (acting on it
        // switches the device), never the keyboard's Jump or the mouse's sensitivity.
        let list = worded(&WORDED);
        let l = resolve_layout(&list);
        let pad_jump = RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::Controller));
        let stick = RowId::Control(ControlRow::Sensitivity(InputDevice::Controller));
        let rows = changed(&l, &[], |c| RowId::Control(c) == pad_jump || RowId::Control(c) == stick);
        assert_eq!(rows, [pad_jump, stick], "the rows themselves, for the log and Defaults");
        let vis = Visibility::default();
        let found = Found { search: Vec::new(), changed: rows.clone() };
        let e = project(&l, &list, SettingsScope::Changed, vis, &found);
        assert_eq!(names(&list, &e), ["[Controls]", "device"]);
        assert!(!e.contains(&ViewEntry::Row(RowId::Control(ControlRow::Binding(Control::Jump, InputDevice::KeyboardMouse)))));
        assert!(!e.contains(&ViewEntry::Row(RowId::Control(ControlRow::Sensitivity(InputDevice::KeyboardMouse)))));
        // What the Input Device row stands for, named under it.
        assert_eq!(stands_for(&list, &rows, vis), [(RowId::Control(ControlRow::Device), vec![pad_jump, stick])]);
        // With the controller in use they are their own rows.
        let pad = Visibility { device: InputDevice::Controller, ..vis };
        assert_eq!(names(&list, &project(&l, &list, SettingsScope::Changed, pad, &found)), ["[Controls]", "padbind:Jump", "sensitivity"]);
        assert!(stands_for(&list, &rows, pad).is_empty());
    }

    #[test]
    fn a_row_put_back_gives_the_choice_to_the_row_after_it() {
        let r = |i| ViewEntry::Row(RowId::Setting(i));
        let h = ViewEntry::Heading(Placement::tab("Debug"));
        let before = [h, r(1), r(2), r(3)];
        assert_eq!(reselect(&before, &[h, r(1), r(3)], Some(RowId::Setting(2))), Some(RowId::Setting(3)));
        assert_eq!(reselect(&before, &[h, r(1), r(2)], Some(RowId::Setting(3))), Some(RowId::Setting(2)), "the last: the one before");
        assert_eq!(reselect(&before, &[h, r(1), r(2), r(3)], Some(RowId::Setting(2))), Some(RowId::Setting(2)));
        assert_eq!(reselect(&before, &[], Some(RowId::Setting(2))), Some(RowId::Setting(2)), "none left: as chosen (the caller finds none listed)");
        assert_eq!(reselect(&before, &[h, r(1)], None), None);
    }
}
