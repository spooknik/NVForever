//! The tile tree: menus read from their files into tiles with traits, found by path,
//! read and changed by the engine and by scripts, and worked out again when what a
//! trait reads from changes (docs/design/ui-tile-tree.md §2–§4,
//! docs/spec-notes/menu-xml.md).

use crate::expr::{self, Arg, Op, OpKind, Src};
use crate::value::Value;
use crate::xml::{self, Element, Node};
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::num::NonZeroU64;
use std::panic::Location;
use std::sync::atomic::{AtomicU32, Ordering};

/// The tile kinds, the tags that make a tile [observed: the tags with a `name`
/// attribute in the vanilla files; `window` is in no vanilla file but in older Gamebryo
/// menu files, kept as a tile in case a mod uses it (hypothesis)].
pub const TILE_KINDS: &[&str] = &["menu", "rect", "image", "text", "nif", "radial", "hotrect", "window"];

/// Includes nested deeper than this are not followed [design: a guard against a file
/// including itself].
const MAX_INCLUDE_DEPTH: usize = 16;

/// Passes of [`Ui::update`] before it gives up on traits that keep changing (a loop of
/// traits reading each other) [design].
const MAX_PASSES: usize = 32;

/// Where the menu files come from: the game's archives and loose files under `Data`
/// (the engine passes its VFS), or files in memory for tests.
pub trait Source {
    /// [design] Files recursively below a Data-relative directory, as full Data-relative
    /// paths; comparisons ignore case and slash style. Sources may omit discovery.
    fn list(&self, _dir: &str) -> Vec<String> {
        Vec::new()
    }
    /// A file by its path under `Data`, e.g. `menus\prefabs\meter.xml`; any case, `\`
    /// or `/`.
    fn read(&self, path: &str) -> Option<Vec<u8>>;
    /// [design] Whether the winning file is loose; sources without provenance return false.
    fn is_loose(&self, _path: &str) -> bool {
        false
    }
    /// [design] The archived copy of a file, also when a loose file replaces it (the
    /// file a loose replacement is compared against, [`Ui::archived_name_paths`]); `None`
    /// when no archive has it. By default a path that is not loose reads as it wins.
    fn read_archived(&self, path: &str) -> Option<Vec<u8>> {
        if self.is_loose(path) {
            None
        } else {
            self.read(path)
        }
    }
    /// A game setting's string, for `&-sName;` (the GMST `sName`): the engine answers
    /// from a [`crate::GameStrings`] (the load order's GMSTs, then the built-in table).
    fn game_string(&self, _name: &str) -> Option<String> {
        None
    }
    /// [design] Whether [`Source::game_string`] answers `name` from the built-in table, no
    /// plugin defining it. A source that cannot tell says no: its text then counts as chosen.
    fn game_string_is_built_in(&self, _name: &str) -> bool {
        false
    }
}

/// Files in memory, by path in lower case with `\`.
#[derive(Default)]
pub struct MemSource {
    pub files: HashMap<String, Vec<u8>>,
    pub strings: HashMap<String, String>,
    /// [design] The names in `strings` answering as the built-in table does, in lower case.
    pub built_in_strings: HashSet<String>,
    /// [design] Loose-file provenance for synthetic sources, using normalized paths.
    pub loose_files: HashSet<String>,
    /// [design] The archived copies under synthetic loose files, by normalized path.
    pub archived: HashMap<String, Vec<u8>>,
}

impl MemSource {
    pub fn with(mut self, path: &str, text: &str) -> MemSource {
        self.files.insert(norm(path), text.as_bytes().to_vec());
        self
    }

    /// [design] Add a synthetic loose file.
    pub fn with_loose(mut self, path: &str, text: &str) -> MemSource {
        self.loose_files.insert(norm(path));
        self.files.insert(norm(path), text.as_bytes().to_vec());
        self
    }

    /// [design] Add a synthetic loose file over an archived copy of the same path.
    pub fn with_loose_over(mut self, path: &str, loose: &str, archived: &str) -> MemSource {
        self.archived.insert(norm(path), archived.as_bytes().to_vec());
        self.with_loose(path, loose)
    }
}

impl Source for MemSource {
    fn list(&self, dir: &str) -> Vec<String> {
        let prefix = format!("{}\\", norm(dir).trim_end_matches('\\'));
        let mut paths: Vec<_> = self.files.keys().filter(|p| p.starts_with(&prefix)).cloned().collect();
        paths.sort();
        paths
    }
    fn is_loose(&self, path: &str) -> bool {
        self.loose_files.contains(&norm(path))
    }

    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.files.get(&norm(path)).cloned()
    }

    fn read_archived(&self, path: &str) -> Option<Vec<u8>> {
        let path = norm(path);
        if self.loose_files.contains(&path) {
            self.archived.get(&path).cloned()
        } else {
            self.files.get(&path).cloned()
        }
    }

    /// Without case, as the original [observed: menu-xml.md §2.4].
    fn game_string(&self, name: &str) -> Option<String> {
        self.strings.get(name).or_else(|| self.strings.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v)).cloned()
    }

    fn game_string_is_built_in(&self, name: &str) -> bool {
        self.built_in_strings.contains(&name.to_ascii_lowercase())
    }
}

/// [design] A source's archived copies alone ([`Source::read_archived`]): the menu files
/// as the archives hold them, under any loose replacement. No discovery, no loose files.
pub struct Archived<'a>(pub &'a dyn Source);

impl Source for Archived<'_> {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0.read_archived(path)
    }
    fn game_string(&self, name: &str) -> Option<String> {
        self.0.game_string(name)
    }
    fn game_string_is_built_in(&self, name: &str) -> bool {
        self.0.game_string_is_built_in(name)
    }
}

fn norm(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

/// A tile: its tree, its slot in the tree and the slot's generation when the tile was
/// made. An unloaded tile's slot is used again for a later tile with the next generation,
/// so an id kept past its tile's unloading names no tile ([`Ui::tile`] answers a dead,
/// empty tile) and never the slot's new tile; nor does an id kept from a tree since
/// replaced (a load builds a new one) name a tile of the new tree [design: #1039, #1061].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileId {
    tree: u32,
    index: u32,
    gen: u32,
}

/// The number the next [`Ui::new`] takes, so each tree's ids are its own.
static NEXT_TREE: AtomicU32 = AtomicU32::new(1);

/// Updates in a row that each followed a change of the tree's shape (so worked every
/// expression out) before [`Ui::take_reshape_warning`] answers [design: #1061, about two
/// seconds of frames; loading a menu takes a few].
const RESHAPED_RUN_WARNING: u32 = 120;

impl TileId {
    /// The tile's slot: the same for a later tile made there, so not an identity.
    pub fn index(self) -> u32 {
        self.index
    }
}

/// [design] Creator of a tile; descendants retain that creator across includes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    /// [design] Path passed to Source, and its loose-file status when loaded.
    File {
        path: String,
        loose: bool,
    },
    Injected,
    Template,
    Code,
}

impl Origin {
    fn file(src: &dyn Source, path: &str) -> Self {
        Self::File { path: path.to_string(), loose: src.is_loose(path) }
    }

    fn mod_added(&self) -> bool {
        matches!(self, Self::Injected | Self::Template | Self::File { loose: true, .. })
    }
}

/// An external write's opaque identity within its Ui, not writable authority.
/// Pair with the full TileId and trait name, never a value or vector index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraitWriteSerial(NonZeroU64);

/// One trait of a tile.
#[derive(Clone, Debug)]
pub struct Trait {
    /// As the file writes it.
    pub name: String,
    pub value: Value,
    /// The operators that work the value out, when it is not a plain value.
    pub expr: Option<Vec<Op>>,
    /// The tree's change count when the value last changed.
    pub changed: u64,
    /// Every external setter/XML replacement, even equal values; never expression work.
    write_serial: Option<TraitWriteSerial>,
    /// The change count when the expression was last worked out (0: never).
    evaluated: u64,
    /// [design] The lower-case setting whose built-in label this trait holds, as the file
    /// wrote it (`&-sName;` alone, no plugin defining the name); none once anything writes it.
    /// [`Ui::built_in_label`].
    pub built_in_label: Option<String>,
    /// The traits (tile, index) the expression read when last worked out, valid while the
    /// tree's shape is still `seen` (0: not known): with none of them changed since
    /// `evaluated`, [`Ui::update`] skips it without working it out [design: #1039].
    sources: Vec<(TileId, u32)>,
    seen: u64,
}

impl Trait {
    fn new(name: String, value: Value, expr: Option<Vec<Op>>, changed: u64) -> Trait {
        Trait { name, value, expr, changed, write_serial: None, evaluated: 0, built_in_label: None, sources: Vec::new(), seen: 0 }
    }
}

/// A `<template>`: content kept to make tiles from later (`AddTileFromTemplate`, the
/// game's code filling lists).
#[derive(Clone, Debug)]
pub struct Template {
    pub name: String,
    pub body: Vec<Node>,
}

#[derive(Clone, Debug)]
pub struct Tile {
    /// [design] Provenance of this tile, independent of later trait writes.
    pub origin: Origin,
    /// The tag in lower case: one of [`TILE_KINDS`].
    pub kind: String,
    pub name: String,
    pub parent: Option<TileId>,
    pub children: Vec<TileId>,
    pub traits: Vec<Trait>,
    pub templates: Vec<Template>,
    /// False once unloaded: [`Ui::tile`] answers such a tile for an id kept past its
    /// tile's unloading.
    pub live: bool,
}

/// What [`Ui::tile`] answers for an unloaded tile's id.
static DEAD: Tile = Tile::dead();

impl Tile {
    const fn dead() -> Tile {
        Tile {
            origin: Origin::Code,
            kind: String::new(),
            name: String::new(),
            parent: None,
            children: Vec::new(),
            traits: Vec::new(),
            templates: Vec::new(),
            live: false,
        }
    }

    pub fn trait_index(&self, name: &str) -> Option<usize> {
        self.traits.iter().position(|t| t.name.eq_ignore_ascii_case(name))
    }

    /// A trait's value by name (any case).
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.trait_index(name).map(|k| &self.traits[k].value)
    }
}

/// The menus' tile tree.
#[derive(Default)]
pub struct Ui {
    /// [design] Evaluated list entries and cumulative outcomes for this tree's lifetime.
    pub uio: crate::uio::Report,
    uio_tiles: HashSet<TileId>,
    /// The tiles by slot; an unloaded tile's slot holds a dead tile until it is used again.
    tiles: Vec<Tile>,
    /// Each slot's generation, one up each time its tile is unloaded.
    slot_gens: Vec<u32>,
    /// The slots of unloaded tiles, to use again [design: #1039, so opening and closing a
    /// menu leaves the tree its size], the lowest first: a menu's tiles then take slots in
    /// the order they are made, parents before children, as [`Ui::update`] walks the
    /// slots, so a reopened menu settles in as few passes as when first opened.
    free: BinaryHeap<Reverse<u32>>,
    /// One up whenever tiles are made, unloaded or changed other than in a value (renamed,
    /// given a trait or a template): what a source names may have changed, so the sources
    /// a trait read last are no longer known.
    shape: u64,
    /// This tree's number, in every [`TileId`] it makes ([`Ui::new`]).
    tree: u32,
    /// The shape when [`Ui::update`] last ran, the updates in a row that found it changed,
    /// whether that was told ([`Ui::take_reshape_warning`]), and the last change of shape
    /// made from outside the tree: what did it and from where.
    shape_at_update: u64,
    reshaped_run: u32,
    reshape_told: bool,
    reshaped_by: Option<(&'static str, &'static Location<'static>)>,
    /// The public call under way and where it was called from, for `reshaped_by`.
    op: Option<(&'static str, &'static Location<'static>)>,
    /// The loaded menus (and `globals()`, `screen()`, `strings()`), in load order.
    roots: Vec<TileId>,
    globals: Option<TileId>,
    screen: Option<TileId>,
    strings: Option<TileId>,
    /// Counts value changes: a trait whose sources changed after it was last worked out
    /// is worked out again.
    gen: u64,
    /// A tile, a file trait or a value changed since [`Ui::update`] last finished: with
    /// none, it has nothing to work out [design: writes of the same value, as the
    /// engine's every frame, leave the tree clean].
    dirty: bool,
    /// Trait writes from outside so far, and those that changed the value
    /// ([`Ui::trait_write_counts`]). Not behind the test aid: an integer added to in
    /// `set_at`, as `gen` is beside them, with no lock, allocation or output; a gate would
    /// read a flag in the same place.
    trait_writes: u64,
    trait_changes: u64,
    /// Ui-local external-write clock; exhausted writes are unownable, never wrapped.
    last_write_serial: u64,
    /// How many live mod-added tiles (an `Injected`, `Template` or loose file origin, or
    /// a loose UIO include's) each root tile holds, itself included, by root: kept where
    /// tiles are made, unloaded or change origin, so [`Ui::has_mod_added_tiles`] walks
    /// nothing [design: maintainer's review of #902, #938].
    mod_tiles: HashMap<TileId, usize>,
    /// What loading mended or left out, one line each.
    pub warnings: Vec<String>,
    /// The game strings (`&-sName;`) read into traits so far, by lower-case name, and
    /// whether the source had text for it (menu-xml.md §2.4) [design].
    pub game_strings_read: BTreeMap<String, bool>,
    /// What the game's own code last wrote to a tile's trait, for the writes it makes only
    /// when its value changes, so a script's value in between lasts (`Ui::write_changed`).
    code_written: HashMap<(TileId, String), Value>,
    /// Expressions [`Ui::update`] worked out (not skipped), for the tests.
    #[cfg(test)]
    pub(crate) evaluations: usize,
    /// Work every expression out at every pass as before #1039, for the tests to compare.
    #[cfg(test)]
    pub(crate) never_skip: bool,
    /// Passes the last [`Ui::update`] made, for the tests.
    #[cfg(test)]
    pub(crate) passes: usize,
}

impl Ui {
    /// An empty tree with the three pseudo-tiles `screen()`, `strings()` and (until
    /// [`Ui::load_globals`]) an empty `globals()`.
    pub fn new() -> Ui {
        let mut ui = Ui { tree: NEXT_TREE.fetch_add(1, Ordering::Relaxed), ..Ui::default() };
        let screen = ui.new_tile("rect", "screen()", None, Origin::Code);
        let strings = ui.new_tile("rect", "strings()", None, Origin::Code);
        let globals = ui.new_tile("rect", "globals()", None, Origin::Code);
        ui.roots.extend([screen, strings, globals]);
        ui.screen = Some(screen);
        ui.strings = Some(strings);
        ui.globals = Some(globals);
        ui
    }

    /// Sets a trait as the game's code does for a value it writes only when the value
    /// changes: nothing when `v` is what the code wrote last, whatever a script set since.
    #[track_caller]
    pub fn write_changed(&mut self, tile: TileId, name: &str, v: Value) {
        self.entered("write_changed");
        if !self.is_live(tile) {
            return;
        }
        let key = (tile, name.to_string());
        if self.code_written.get(&key) == Some(&v) {
            return;
        }
        self.set_trait(tile, name, v.clone());
        self.code_written.insert(key, v);
    }

    /// The tile; for an id kept past its tile's unloading a dead tile (not `live`, no
    /// name, children or traits).
    pub fn tile(&self, id: TileId) -> &Tile {
        match self.slot(id) {
            Some(i) => &self.tiles[i],
            None => &DEAD,
        }
    }

    /// Whether `id` names a loaded tile of this tree: false for one unloaded (even when
    /// a later tile has its slot), and for an id kept from a tree since replaced (a load
    /// builds a new one) [design].
    pub fn is_live(&self, id: TileId) -> bool {
        self.slot(id).is_some()
    }

    /// The id of the tile in slot `i` now.
    fn id_at(&self, i: usize) -> TileId {
        TileId { tree: self.tree, index: i as u32, gen: self.slot_gens[i] }
    }

    /// The id's slot, when its tile is loaded.
    fn slot(&self, id: TileId) -> Option<usize> {
        let i = id.index as usize;
        (id.tree == self.tree && self.slot_gens.get(i) == Some(&id.gen) && self.tiles[i].live).then_some(i)
    }

    /// How many slots the tree holds: the most tiles loaded at once, the pseudo-tiles
    /// included.
    pub fn tile_slots(&self) -> usize {
        self.tiles.len()
    }

    /// How many tiles are loaded, the pseudo-tiles included.
    pub fn live_tiles(&self) -> usize {
        self.tiles.len() - self.free.len()
    }

    /// A count that goes up whenever tiles are made, unloaded or changed other than in a value:
    /// what follows from the tree's shape alone stands while this does.
    pub fn shape(&self) -> u64 {
        self.shape
    }

    /// A loaded tile to change (made, unloaded, renamed, its file traits or templates):
    /// the tree is worked out again at the next [`Ui::update`], every expression in it
    /// (the shape changed). Value writes go through `set_at`.
    fn tile_mut(&mut self, id: TileId) -> &mut Tile {
        self.dirty = true;
        self.reshape();
        let i = self.slot(id).expect("a loaded tile");
        &mut self.tiles[i]
    }

    /// A public call that may change the shape starts: what it is and where it was called
    /// from, kept for [`Ui::take_reshape_warning`].
    #[track_caller]
    fn entered(&mut self, what: &'static str) {
        self.op = Some((what, Location::caller()));
    }

    /// The shape changes: what a source names may have changed, so every expression is
    /// worked out at the next update.
    fn reshape(&mut self) {
        self.shape += 1;
        self.reshaped_by = self.op;
    }

    /// [design: #1061] Once, when the shape changed before each of the last
    /// [`RESHAPED_RUN_WARNING`] updates, so that every update worked every expression out
    /// (the skip of #1039 lost): a line naming the last call that changed it and where
    /// from. The engine asks after each update and logs it.
    pub fn take_reshape_warning(&mut self) -> Option<String> {
        if self.reshape_told || self.reshaped_run < RESHAPED_RUN_WARNING {
            return None;
        }
        self.reshape_told = true;
        let by = match self.reshaped_by {
            Some((what, at)) => format!("{what} called at {}:{}", at.file(), at.line()),
            None => "unknown".into(),
        };
        Some(format!("ui tree: its shape changed before each of {} updates in a row, so each worked every expression out; last change: {by}", self.reshaped_run))
    }

    pub fn screen(&self) -> TileId {
        self.screen.expect("Ui::new makes screen()")
    }

    pub fn strings(&self) -> TileId {
        self.strings.expect("Ui::new makes strings()")
    }

    pub fn globals(&self) -> TileId {
        self.globals.expect("Ui::new makes globals()")
    }

    /// The live menus' root tiles (not the pseudo-tiles).
    pub fn menus(&self) -> impl Iterator<Item = TileId> + '_ {
        self.roots.iter().copied().filter(|&r| self.tile(r).kind == "menu")
    }

    /// The loaded menu of that name (any case).
    pub fn menu(&self, name: &str) -> Option<TileId> {
        self.menus().find(|&r| self.tile(r).name.eq_ignore_ascii_case(name))
    }

    fn new_tile(&mut self, kind: &str, name: &str, parent: Option<TileId>, origin: Origin) -> TileId {
        self.dirty = true;
        self.reshape();
        let tile = Tile { origin, kind: kind.to_ascii_lowercase(), name: name.to_string(), parent, live: true, ..Tile::dead() };
        let id = match self.free.pop() {
            Some(Reverse(index)) => {
                self.tiles[index as usize] = tile;
                self.id_at(index as usize)
            }
            None => {
                self.tiles.push(tile);
                self.slot_gens.push(0);
                self.id_at(self.tiles.len() - 1)
            }
        };
        if let Some(p) = parent {
            self.tile_mut(p).children.push(id);
        }
        if self.tile(id).origin.mod_added() {
            self.count_mod_tile(id, true);
        }
        id
    }

    /// Whether a tile counts in [`Ui::mod_added_tiles`]' walk as the mod's: its origin, or
    /// a loose UIO include's.
    fn is_mod_added(&self, id: TileId) -> bool {
        self.tile(id).origin.mod_added() || self.uio_tiles.contains(&id)
    }

    /// One mod-added tile more (`up`) or less under its root ([`Ui::mod_tiles`]).
    fn count_mod_tile(&mut self, id: TileId, up: bool) {
        let root = self.root_of(id);
        let n = self.mod_tiles.entry(root).or_default();
        if up {
            *n += 1;
        } else {
            *n = n.saturating_sub(1);
            if *n == 0 {
                self.mod_tiles.remove(&root);
            }
        }
    }

    /// Counts the live mod-added tiles again, after the UIO tiles were forgotten.
    fn recount_mod_tiles(&mut self) {
        self.mod_tiles.clear();
        for i in 0..self.tiles.len() {
            let id = self.id_at(i);
            if self.tile(id).live && self.is_mod_added(id) {
                self.count_mod_tile(id, true);
            }
        }
    }

    // ---- loading --------------------------------------------------------------------

    /// [design] Discover and evaluate UIO once before loading menus; the caller supplies
    /// active plugins. Rebuilt menus reuse these entries (menu-xml.md §7 item 3).
    pub fn configure_uio(&mut self, src: &dyn Source, plugin: impl Fn(&str) -> bool) {
        self.uio = crate::uio::Report::read(src, &plugin);
        self.uio_tiles.clear();
        self.recount_mod_tiles();
    }

    /// Reads `menus\globals.xml` into `globals()`.
    #[track_caller]
    pub fn load_globals(&mut self, src: &dyn Source) -> bool {
        self.entered("load_globals");
        let Some(doc) = self.read_doc(src, "menus\\globals.xml") else {
            return false;
        };
        let g = self.globals();
        let origin = Origin::file(src, "menus\\globals.xml");
        let counted = self.tile(g).live && self.is_mod_added(g);
        self.tile_mut(g).origin = origin.clone();
        if counted != (self.tile(g).live && self.is_mod_added(g)) {
            self.count_mod_tile(g, !counted);
        }
        // The file is one `<rect name="Strings">` [observed]; its content is globals()'.
        for n in &doc.nodes {
            if let Node::Element(e) = n {
                if is_tile(e) {
                    self.fill(src, g, &e.children, 0, &origin);
                } else {
                    self.fill(src, g, std::slice::from_ref(n), 0, &origin);
                }
            }
        }
        true
    }

    /// Reads a menu file (`menus\…\x_menu.xml`) and builds its menu; a menu of the same
    /// name already loaded is unloaded first. Returns the menu's tile.
    #[track_caller]
    pub fn load_menu(&mut self, src: &dyn Source, path: &str) -> Result<TileId, String> {
        self.entered("load_menu");
        let doc = self.read_doc(src, path).ok_or_else(|| format!("{path}: not found"))?;
        let menu = doc
            .nodes
            .iter()
            .find_map(|n| match n {
                Node::Element(e) if e.is("menu") => Some(e),
                _ => None,
            })
            .ok_or_else(|| format!("{path}: no <menu> element"))?;
        let name = menu.attr("name").unwrap_or("").to_string();
        if let Some(old) = self.menu(&name) {
            self.unload_tile(old);
        }
        let origin = Origin::file(src, path);
        let id = self.new_tile("menu", &name, None, origin.clone());
        self.roots.push(id);
        self.fill(src, id, &menu.children, 0, &origin);
        let entries: Vec<_> = self
            .uio
            .decisions
            .iter()
            .enumerate()
            .filter(|(_, d)| d.enabled && matches!(&d.entry.target, crate::uio::Target::Menu(n) if n.eq_ignore_ascii_case(&name)))
            .map(|(i, d)| (i, d.entry.clone()))
            .collect();
        for (i, entry) in entries {
            let target = match entry.tile {
                Some(ref name) => self.descendant_named(id, name),
                None => Some(id),
            };
            if let Some(target) = target {
                // [hypothesis] Append after existing content: preserves file order and
                // lets list traits override earlier traits; UIO's position is unmeasured.
                let node = uio_include(&entry.include);
                self.fill(src, target, &[node], 0, &origin);
                self.uio.decisions[i].applied += 1;
                self.uio.decisions[i].reason = "applied".into();
            }
        }
        Ok(id)
    }

    fn descendant_named(&self, root: TileId, name: &str) -> Option<TileId> {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if self.tile(id).name.eq_ignore_ascii_case(name) {
                return Some(id);
            }
            stack.extend(self.tile(id).children.iter().rev().copied());
        }
        None
    }

    fn read_doc(&mut self, src: &dyn Source, path: &str) -> Option<xml::Doc> {
        let bytes = src.read(path)?;
        let mut doc = xml::parse(&xml::decode(&bytes));
        for w in &doc.warnings {
            self.warnings.push(format!("{path}: {w}"));
        }
        // [design] Patch prefab syntax before fill/expansion, including named tiles in
        // template bodies that have no TileId until a template is instantiated.
        for decision in &mut self.uio.decisions {
            if !decision.enabled || !matches!(&decision.entry.target, crate::uio::Target::Prefab(p) if *p == norm(path)) {
                continue;
            }
            let target = match &decision.entry.tile {
                Some(name) => named_nodes(&mut doc.nodes, name),
                None => Some(&mut doc.nodes),
            };
            if let Some(nodes) = target {
                nodes.push(uio_include(&decision.entry.include));
                decision.applied += 1;
                decision.reason = "applied".into();
            }
        }
        Some(doc)
    }

    /// Puts content into a tile: child tiles, templates, traits, and the content of
    /// included files where they are included.
    fn fill(&mut self, src: &dyn Source, tile: TileId, nodes: &[Node], depth: usize, origin: &Origin) {
        for n in nodes {
            let Node::Element(e) = n else { continue };
            if is_tile(e) {
                let child = self.new_tile(&e.name, e.attr("name").unwrap_or(""), Some(tile), origin.clone());
                self.fill(src, child, &e.children, depth, origin);
            } else if e.is("template") {
                let name = e.attr("name").unwrap_or("").to_string();
                let body = self.expand_includes(src, &e.children, depth);
                self.tile_mut(tile).templates.push(Template { name, body });
            } else if e.is("include") {
                let file = e.attr("src").unwrap_or("");
                if depth >= MAX_INCLUDE_DEPTH {
                    self.warnings.push(format!("include {file}: nested too deep; left out"));
                    continue;
                }
                let path = prefab_path(file);
                let included_origin = match origin {
                    Origin::File { .. } => Origin::file(src, &path),
                    other => other.clone(),
                };
                match self.read_doc(src, &path) {
                    Some(doc) => {
                        let start = self.tile(tile).children.len();
                        self.fill(src, tile, &doc.nodes, depth + 1, &included_origin);
                        // [design] Track this inclusion's tiles, rather than all uses of
                        // its filename: the same prefab may also occur in vanilla content.
                        // They are the children it added to `tile` and theirs (slots are
                        // used again, so they are no run of slots).
                        if e.attr("nvf_uio") == Some("true") && src.is_loose(&path) {
                            let mut stack = self.tile(tile).children[start..].to_vec();
                            while let Some(id) = stack.pop() {
                                stack.extend(self.tile(id).children.iter().copied());
                                let counted = self.is_mod_added(id);
                                if self.uio_tiles.insert(id) && !counted && self.tile(id).live {
                                    self.count_mod_tile(id, true);
                                }
                            }
                        }
                    }
                    None => self.warnings.push(format!("include {file}: not found")),
                }
            } else {
                self.put_trait(src, tile, e);
            }
        }
    }

    /// A template's content with its includes spliced in, so that tiles made from it
    /// later need no files.
    fn expand_includes(&mut self, src: &dyn Source, nodes: &[Node], depth: usize) -> Vec<Node> {
        let mut out = Vec::new();
        for n in nodes {
            match n {
                Node::Element(e) if e.is("include") && depth < MAX_INCLUDE_DEPTH => {
                    let file = e.attr("src").unwrap_or("");
                    match self.read_doc(src, &prefab_path(file)) {
                        Some(doc) => out.extend(self.expand_includes(src, &doc.nodes, depth + 1)),
                        None => self.warnings.push(format!("include {file}: not found")),
                    }
                }
                Node::Element(e) => {
                    let mut e = e.clone();
                    e.children = self.expand_includes(src, &e.children, depth);
                    out.push(Node::Element(e));
                }
                other => out.push(other.clone()),
            }
        }
        out
    }

    /// A trait element: a later one of the same name replaces the earlier [hypothesis:
    /// how the original treats a trait written twice is not known; includes that set a
    /// trait the including file set too depend on it].
    fn put_trait(&mut self, src: &dyn Source, tile: TileId, e: &Element) {
        let asked = RefCell::new(Vec::new());
        let string = |n: &str| {
            let text = src.game_string(n);
            asked.borrow_mut().push((n.to_ascii_lowercase(), text.is_some()));
            text
        };
        let mut warns = Vec::new();
        let (value, expr) = if expr::has_ops(e) {
            (Value::default(), Some(expr::ops_of(&e.children, &string, &mut |w| warns.push(w))))
        } else {
            (expr::literal(&e.children, &string), None)
        };
        let asked = asked.into_inner();
        let built_in_label = match asked.as_slice() {
            [(name, true)] if expr.is_none() && src.game_string_is_built_in(name) && src.game_string(name).is_some_and(|t| t.trim() == value.text().trim()) => Some(name.clone()),
            _ => None,
        };
        for (name, found) in asked {
            *self.game_strings_read.entry(name).or_insert(false) |= found;
        }
        for w in warns {
            self.warnings.push(format!("<{}>: {w}", e.name));
        }
        self.gen += 1;
        let gen = self.gen;
        let write_serial = self.next_write_serial();
        let t = self.tile_mut(tile);
        let mut new = Trait::new(e.name.clone(), value, expr, gen);
        new.write_serial = write_serial;
        new.built_in_label = built_in_label;
        match t.trait_index(&e.name) {
            Some(k) => t.traits[k] = new,
            None => t.traits.push(new),
        }
    }

    /// Unloads a tile and everything in it, and frees their slots for later tiles;
    /// nothing for an id whose tile is already unloaded.
    #[track_caller]
    pub fn unload_tile(&mut self, id: TileId) {
        self.entered("unload_tile");
        if !self.is_live(id) {
            return;
        }
        // Counted off while the tiles are all there: a tile's root is found through them.
        let mut stack = vec![id];
        let mut gone = Vec::new();
        while let Some(t) = stack.pop() {
            if self.is_mod_added(t) {
                self.count_mod_tile(t, false);
            }
            stack.extend(self.tile(t).children.iter().copied());
            gone.push(t);
        }
        if let Some(p) = self.tile(id).parent {
            self.tile_mut(p).children.retain(|&c| c != id);
        }
        self.roots.retain(|&r| r != id);
        self.dirty = true;
        self.reshape();
        for &t in &gone {
            let i = t.index as usize;
            self.tiles[i] = Tile::dead();
            self.slot_gens[i] = self.slot_gens[i].wrapping_add(1);
            self.free.push(Reverse(t.index));
        }
        // Forget what was kept by the tiles' ids, so nothing grows as menus open and close.
        if !self.uio_tiles.is_empty() || !self.code_written.is_empty() {
            let gone: HashSet<TileId> = gone.into_iter().collect();
            self.uio_tiles.retain(|t| !gone.contains(t));
            self.code_written.retain(|(t, _), _| !gone.contains(t));
        }
        self.gen += 1;
    }

    // ---- paths ----------------------------------------------------------------------

    /// The tile a path names: `Menu/Tile/…/Tile`, with `/` or `\`, names in any case, `*`
    /// for any child and `name:N` (or `*:N`) for the N-th (from 0) of the children that
    /// match, counting from the last child [observed: private jobs hud_hardcore hc0,
    /// hud_xp_limbs all, hud_prompt2 p0-p8; menu-xml.md §5]. Unindexed lookup keeps
    /// file order [hypothesis: menu-xml.md §5].
    pub fn find_tile(&self, path: &str) -> Option<TileId> {
        let segs = split(path);
        let (first, rest) = segs.split_first()?;
        let root = self.root_named(first)?;
        self.walk(root, rest)
    }

    /// The tile and trait a path names: [`Ui::find_tile`] of all but the last segment,
    /// and the last a trait of it.
    pub fn find_trait(&self, path: &str) -> Option<(TileId, usize)> {
        let segs = split(path);
        let (name, tiles) = segs.split_last()?;
        let (first, rest) = tiles.split_first()?;
        let root = self.root_named(first)?;
        self.walk_to_trait(root, rest, name)
    }

    /// A trait by a path under a tile: `Tile/…/trait` from `at`'s children, or a trait of
    /// `at` itself when the path is one segment (SortUIListBox's keys, read on each item)
    /// [design].
    pub fn find_trait_in(&self, at: TileId, path: &str) -> Option<(TileId, usize)> {
        let segs = split(path);
        let (name, tiles) = segs.split_last()?;
        self.walk_to_trait(at, tiles, name)
    }

    fn root_named(&self, seg: &str) -> Option<TileId> {
        self.roots.iter().copied().find(|&r| self.tile(r).live && self.tile(r).name.eq_ignore_ascii_case(seg))
    }

    /// The children of `at` a segment matches, in order.
    fn matching(&self, at: TileId, seg: &str) -> Vec<TileId> {
        let (name, index) = match seg.rsplit_once(':') {
            Some((n, k)) if k.trim().parse::<usize>().is_ok() => (n, k.trim().parse::<usize>().ok()),
            _ => (seg, None),
        };
        let all = self.tile(at).children.iter().copied();
        let named: Vec<TileId> = all.filter(|&c| name == "*" || self.tile(c).name.eq_ignore_ascii_case(name)).collect();
        match index {
            Some(k) => named.iter().rev().nth(k).copied().into_iter().collect(),
            None => named,
        }
    }

    fn walk(&self, at: TileId, segs: &[&str]) -> Option<TileId> {
        let Some((seg, rest)) = segs.split_first() else { return Some(at) };
        self.matching(at, seg).into_iter().find_map(|c| self.walk(c, rest))
    }

    fn walk_to_trait(&self, at: TileId, segs: &[&str], name: &str) -> Option<(TileId, usize)> {
        match segs.split_first() {
            None => self.tile(at).trait_index(name).map(|k| (at, k)),
            Some((seg, rest)) => self.matching(at, seg).into_iter().find_map(|c| self.walk_to_trait(c, rest, name)),
        }
    }

    /// The path of a tile, from its menu: `HUDMainMenu/HitPoints`.
    pub fn path_of(&self, id: TileId) -> String {
        let mut names = vec![self.tile(id).name.clone()];
        let mut at = self.tile(id).parent;
        while let Some(p) = at {
            names.push(self.tile(p).name.clone());
            at = self.tile(p).parent;
        }
        names.reverse();
        names.join("/")
    }

    /// The menu (or pseudo-tile) a tile is in.
    pub fn root_of(&self, mut id: TileId) -> TileId {
        while let Some(p) = self.tile(id).parent {
            id = p;
        }
        id
    }

    // ---- reading and changing traits (the UI functions' ground) ---------------------

    /// A trait's value by path: `GetUIFloat` / `GetUIString`.
    pub fn get(&self, path: &str) -> Option<&Value> {
        let (t, k) = self.find_trait(path)?;
        Some(&self.tile(t).traits[k].value)
    }

    pub fn get_float(&self, path: &str) -> Option<f64> {
        self.get(path).map(Value::num)
    }

    pub fn get_string(&self, path: &str) -> Option<String> {
        self.get(path).map(Value::text)
    }

    /// Sets a trait by path (`SetUIFloat`, `SetUIString`); false when the path names no
    /// trait [hypothesis: the setters make no trait that is not there, AddTraitToTile
    /// exists for that]. The value holds until what the trait's expression reads changes.
    pub fn set(&mut self, path: &str, v: Value) -> bool {
        let Some((t, k)) = self.find_trait(path) else { return false };
        self.set_at(t, k, v);
        true
    }

    pub fn set_float(&mut self, path: &str, v: f64) -> bool {
        self.set(path, Value::Num(v))
    }

    pub fn set_string(&mut self, path: &str, s: &str) -> bool {
        self.set(path, Value::Str(s.to_string()))
    }

    /// Sets a tile's trait by name as [`Ui::set`] does by path: false, and nothing made,
    /// when the tile has no such trait (MCM's shorthands, which find their tile by place).
    pub fn set_existing(&mut self, tile: TileId, name: &str, v: Value) -> bool {
        if !self.is_live(tile) {
            return false;
        }
        let Some(k) = self.tile(tile).trait_index(name) else { return false };
        self.set_at(tile, k, v);
        true
    }

    /// Adds to a trait by path (`ModUIFloat`).
    pub fn mod_float(&mut self, path: &str, by: f64) -> bool {
        let Some((t, k)) = self.find_trait(path) else { return false };
        let v = self.tile(t).traits[k].value.num() + by;
        self.set_at(t, k, Value::Num(v));
        true
    }

    /// Sets a tile's trait by name, making it when it is not there (the engine's own
    /// writes: the game code's `io()` traits, `screen()`'s size).
    /// Nothing for an id whose tile is unloaded.
    #[track_caller]
    pub fn set_trait(&mut self, tile: TileId, name: &str, v: Value) {
        self.entered("set_trait");
        if !self.is_live(tile) {
            return;
        }
        let k = match self.tile(tile).trait_index(name) {
            Some(k) => k,
            None => {
                let t = self.tile_mut(tile);
                t.traits.push(Trait::new(name.to_string(), Value::default(), None, 0));
                t.traits.len() - 1
            }
        };
        self.set_at(tile, k, v);
    }

    /// [design] The setting whose built-in label `name` of `tile` still holds as the file wrote
    /// it: a value a plugin, a mod's file or a script chose answers none.
    pub fn built_in_label(&self, tile: TileId, name: &str) -> Option<&str> {
        let t = self.tile(tile);
        t.traits[t.trait_index(name)?].built_in_label.as_deref()
    }

    /// Trait writes from outside so far ([`Ui::set_trait`] and the like) and how many of
    /// them changed the value, for measuring what a frame writes (the engine's
    /// `NVF_PLAY_TEST_TILE_COUNTS`).
    pub fn trait_write_counts(&self) -> (u64, u64) {
        (self.trait_writes, self.trait_changes)
    }

    /// Last accepted external setter/XML insertion or replacement, including equal values.
    /// None for a missing/stale target or an exhausted clock. Expression evaluation does
    /// not advance it; an expression may have a serial but is not thereby a writable sink.
    pub fn trait_write_serial(&self, tile: TileId, name: &str) -> Option<TraitWriteSerial> {
        let t = self.tile(tile);
        t.traits.get(t.trait_index(name)?)?.write_serial
    }

    fn next_write_serial(&mut self) -> Option<TraitWriteSerial> {
        let next = self.last_write_serial.checked_add(1)?;
        self.last_write_serial = next;
        NonZeroU64::new(next).map(TraitWriteSerial)
    }

    fn set_at(&mut self, t: TileId, k: usize, v: Value) {
        self.gen += 1;
        self.trait_writes += 1;
        let gen = self.gen;
        let write_serial = self.next_write_serial();
        let tr = &mut self.tiles[t.index as usize].traits[k];
        tr.write_serial = write_serial;
        if v.differs(&tr.value) {
            tr.changed = gen;
            self.dirty = true;
            self.trait_changes += 1;
        }
        tr.value = v;
        tr.built_in_label = None;
        // A trait set from outside keeps its value until a source changes again. What it
        // read last may have changed before this write, so the next update works it out
        // to learn its sources again.
        tr.evaluated = gen;
        tr.seen = 0;
    }

    /// Takes a trait off a tile, as if it had never had it (an engine's own write undone
    /// [design]): what reads it reads nothing again. False when the tile has none.
    #[track_caller]
    pub fn remove_trait(&mut self, tile: TileId, name: &str) -> bool {
        self.entered("remove_trait");
        if !self.is_live(tile) {
            return false;
        }
        let Some(k) = self.tile(tile).trait_index(name) else { return false };
        // Whoever read it last works its value out afresh: with no source newer than its own
        // last working out, `update` would keep what it had. Other traits written from outside
        // keep theirs. The kept sources name traits by index, and those after it move down one:
        // a second removal before the next update must find the right readers too.
        let k32 = k as u32;
        for t in self.tiles.iter_mut().filter(|t| t.live) {
            for tr in &mut t.traits {
                let mut read = false;
                for source in tr.sources.iter_mut().filter(|s| s.0 == tile) {
                    read |= source.1 == k32;
                    if source.1 > k32 {
                        source.1 -= 1;
                    }
                }
                if read {
                    tr.evaluated = 0;
                }
                tr.sources.retain(|&(s, j)| !(read && s == tile && j == k32));
            }
        }
        // `tile_mut` marks the tree's shape changed: the sources kept by index are read again.
        self.tile_mut(tile).traits.remove(k);
        self.gen += 1;
        self.dirty = true;
        true
    }

    /// Whether a path names a live menu, tile or trait (`IsComponentLoaded`).
    pub fn is_loaded(&self, path: &str) -> bool {
        self.find_tile(path).is_some() || self.find_trait(path).is_some()
    }

    // ---- injection --------------------------------------------------------------------

    /// Puts XML text into the tile a path names (`InjectUIComponent`): tiles, templates,
    /// traits and includes, as if the tile's file had them at its end. False when the
    /// path names no tile.
    #[track_caller]
    pub fn inject_text(&mut self, src: &dyn Source, at: &str, text: &str) -> bool {
        self.entered("inject_text");
        let Some(tile) = self.find_tile(at) else { return false };
        let doc = xml::parse(text);
        for w in &doc.warnings {
            self.warnings.push(format!("injected into {at}: {w}"));
        }
        self.fill(src, tile, &doc.nodes, 0, &Origin::Injected);
        true
    }

    /// Puts a file's XML into the tile a path names (`InjectUIXML`); the file's path is
    /// under the game's folder (`data\menus\…`) [documented: InjectUIXML notes] or under
    /// `Data`. False when the path names no tile or the file is not there.
    #[track_caller]
    pub fn inject_file(&mut self, src: &dyn Source, at: &str, file: &str) -> bool {
        self.entered("inject_file");
        let f = file.replace('/', "\\");
        let f = match f.get(..5) {
            Some(head) if head.eq_ignore_ascii_case("data\\") => &f[5..],
            _ => &f[..],
        };
        let Some(bytes) = src.read(f) else { return false };
        self.inject_text(src, at, &xml::decode(&bytes))
    }

    /// Makes a tile from a template under the tile a path names
    /// (`AddTileFromTemplate`): the template is looked for on that tile and then on the
    /// tiles above it [hypothesis], the new tile named `name` when given. Returns it.
    #[track_caller]
    pub fn add_from_template(&mut self, src: &dyn Source, at: &str, template: &str, name: Option<&str>) -> Option<TileId> {
        self.entered("add_from_template");
        self.add_template(src, at, template, name, Origin::Template)
    }

    /// [design] Make engine-owned template copies, including every top-level tile and
    /// descendant in the template body. These copies are excluded from mod tile roots.
    #[track_caller]
    pub fn add_code_tile_from_template(&mut self, src: &dyn Source, at: &str, template: &str, name: Option<&str>) -> Option<TileId> {
        self.entered("add_code_tile_from_template");
        self.add_template(src, at, template, name, Origin::Code)
    }

    fn add_template(&mut self, src: &dyn Source, at: &str, template: &str, name: Option<&str>, origin: Origin) -> Option<TileId> {
        let parent = self.find_tile(at)?;
        let mut look = Some(parent);
        let body = loop {
            let t = look?;
            if let Some(tp) = self.tile(t).templates.iter().find(|tp| tp.name.eq_ignore_ascii_case(template)) {
                break tp.body.clone();
            }
            look = self.tile(t).parent;
        };
        let before = self.tile(parent).children.len();
        self.fill(src, parent, &body, 0, &origin);
        let made = *self.tile(parent).children.get(before)?;
        if let Some(n) = name {
            self.tile_mut(made).name = n.to_string();
        }
        Some(made)
    }

    /// [design] Top-most live mod-added roots in file order, including the menu itself
    /// when loaded loose. Loose provenance is captured at load; code tiles are not roots.
    pub fn mod_added_tiles(&self, menu: TileId) -> Vec<TileId> {
        if !self.tile(menu).live || self.tile(menu).kind != "menu" {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut stack = vec![menu];
        while let Some(id) = stack.pop() {
            let tile = self.tile(id);
            if !tile.live {
                continue;
            }
            if tile.origin.mod_added() || self.uio_tiles.contains(&id) {
                out.push(id);
            } else {
                stack.extend(tile.children.iter().rev().copied());
            }
        }
        out
    }

    /// [design] Whether [`Ui::mod_added_tiles`] of `menu` has any, without walking it:
    /// from the count kept as tiles are made, unloaded or change origin, so the engine may
    /// ask every frame [design: #938].
    pub fn has_mod_added_tiles(&self, menu: TileId) -> bool {
        let tile = self.tile(menu);
        tile.live && tile.kind == "menu" && self.mod_tiles.get(&menu).is_some_and(|&n| n > 0)
    }

    /// [design] The name paths ([`Ui::path_of`], lower case) of every live tile under
    /// `root`, itself included: what a menu file's tiles are called when one source builds
    /// them, to tell a loose replacement's own tiles from the archived file's
    /// (menu-xml.md §7).
    pub fn name_paths(&self, root: TileId) -> HashSet<String> {
        let mut out = HashSet::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let tile = self.tile(id);
            if tile.live {
                out.insert(self.path_of(id).to_ascii_lowercase());
                stack.extend(tile.children.iter().copied());
            }
        }
        out
    }

    /// [design] [`Ui::name_paths`] of the menu file at `path` as `src` builds it alone, in
    /// a tree of its own (`None` when the file cannot be read): the vanilla names a loose
    /// replacement is compared against.
    pub fn file_name_paths(src: &dyn Source, path: &str) -> Option<HashSet<String>> {
        let mut ui = Ui::new();
        let root = ui.load_menu(src, path).ok()?;
        Some(ui.name_paths(root))
    }

    /// [design] [`Ui::file_name_paths`] of the archived copy of the menu file at `path`
    /// ([`Archived`]), also under a loose replacement: the names [`Ui::mod_tile_drawn`]
    /// compares a loose file's tiles with (#975). `None` when no archive has the file.
    pub fn archived_name_paths(src: &dyn Source, path: &str) -> Option<HashSet<String>> {
        Ui::file_name_paths(&Archived(src), path)
    }

    /// [design] Whether a tile inside a root of [`Ui::mod_added_tiles`] is the mod's own,
    /// to draw over NVForever's HUD: never a `Code` tile; always an `Injected` or
    /// `Template` one; a `File` tile (a loose replacement's, or an archived prefab's it
    /// includes) only when `vanilla` holds the archived file's names
    /// ([`Ui::file_name_paths`]) and not this tile's name path. Without the vanilla names no
    /// `File` tile is drawn, except a loose UIO include and its file descendants [design].
    pub fn mod_tile_drawn(&self, id: TileId, vanilla: Option<&HashSet<String>>) -> bool {
        match &self.tile(id).origin {
            Origin::Code => false,
            Origin::Injected | Origin::Template => true,
            Origin::File { .. } => {
                // [design] A loose UIO file is an explicit addition, even when no
                // archived comparison exists or a vanilla tile has the same name.
                self.uio_tiles.contains(&id) || vanilla.is_some_and(|names| !names.contains(&self.path_of(id).to_ascii_lowercase()))
            }
        }
    }

    // ---- working traits out -----------------------------------------------------------

    /// Works out every trait with an expression whose sources changed since it was last
    /// worked out (all of them the first time), again and again until nothing changes.
    /// Returns how many trait values changed. Nothing to do when nothing changed since it
    /// last finished.
    pub fn update(&mut self) -> usize {
        if self.shape != self.shape_at_update {
            self.shape_at_update = self.shape;
            self.reshaped_run += 1;
        } else {
            self.reshaped_run = 0;
        }
        if !self.dirty {
            return 0;
        }
        let mut changed = 0;
        let mut sources = Vec::new();
        #[cfg(test)]
        {
            self.passes = 0;
        }
        for _ in 0..MAX_PASSES {
            #[cfg(test)]
            {
                self.passes += 1;
            }
            let mut this_pass = 0;
            for id in 0..self.tiles.len() {
                if !self.tiles[id].live {
                    continue;
                }
                let tid = self.id_at(id);
                for k in 0..self.tiles[id].traits.len() {
                    let tr = &self.tiles[id].traits[k];
                    let Some(ops) = tr.expr.as_ref() else { continue };
                    // What it read last, unchanged since and still what its sources name
                    // (the same shape): working it out would find the same and skip it.
                    let skip = tr.evaluated != 0 && tr.seen == self.shape && tr.sources.iter().all(|&(t, j)| self.tiles[t.index as usize].traits[j as usize].changed <= tr.evaluated);
                    #[cfg(test)]
                    let skip = skip && !self.never_skip;
                    if skip {
                        continue;
                    }
                    #[cfg(test)]
                    {
                        self.evaluations += 1;
                    }
                    let mut deps = 0;
                    sources.clear();
                    let v = self.eval_reading(tid, ops, &mut deps, &mut sources);
                    let shape = self.shape;
                    let tr = &mut self.tiles[id].traits[k];
                    tr.sources.clone_from(&sources);
                    tr.seen = shape;
                    if tr.evaluated != 0 && deps <= tr.evaluated {
                        continue;
                    }
                    let differs = v.differs(&tr.value);
                    self.gen += 1;
                    let gen = self.gen;
                    let tr = &mut self.tiles[id].traits[k];
                    tr.evaluated = gen;
                    if differs {
                        tr.value = v;
                        tr.changed = gen;
                        this_pass += 1;
                    }
                }
            }
            changed += this_pass;
            if this_pass == 0 {
                self.dirty = false;
                return changed;
            }
        }
        self.warnings.push(format!("update: traits still changing after {MAX_PASSES} passes (a loop?)"));
        changed
    }

    /// Whether [`Ui::update`] has something to work out.
    #[cfg(test)]
    pub(crate) fn dirty(&self) -> bool {
        self.dirty
    }

    /// The tile a source names, seen from `me`.
    pub fn resolve(&self, me: TileId, src: &Src) -> Option<TileId> {
        let parent = |t: TileId| self.tile(t).parent;
        let child_named = |t: TileId, n: &str| self.tile(t).children.iter().copied().find(|&c| self.tile(c).name.eq_ignore_ascii_case(n));
        match src {
            Src::Me => Some(me),
            Src::Parent => parent(me),
            Src::Grandparent => parent(parent(me)?),
            Src::Ancestor(n) => {
                let mut at = parent(me);
                while let Some(t) = at {
                    if self.tile(t).name.eq_ignore_ascii_case(n) {
                        return Some(t);
                    }
                    at = parent(t);
                }
                None
            }
            Src::Child(None) => self.tile(me).children.first().copied(),
            Src::Child(Some(n)) => child_named(me, n),
            Src::Sibling(n) => {
                let p = parent(me)?;
                let kids = &self.tile(p).children;
                match n {
                    Some(n) => kids.iter().copied().find(|&c| c != me && self.tile(c).name.eq_ignore_ascii_case(n)),
                    None => {
                        let at = kids.iter().position(|&c| c == me)?;
                        at.checked_sub(1).map(|k| kids[k])
                    }
                }
            }
            Src::Screen => self.screen,
            Src::Globals => self.globals,
            Src::Strings => self.strings,
            Src::Io => Some(self.root_of(me)),
            Src::Named(n) => {
                let root = self.root_of(me);
                self.find_named(root, n).or_else(|| self.root_named(n))
            }
        }
    }

    /// The first tile of that name under `root`, in file order.
    fn find_named(&self, root: TileId, name: &str) -> Option<TileId> {
        let mut stack = vec![root];
        while let Some(t) = stack.pop() {
            if t != root && self.tile(t).name.eq_ignore_ascii_case(name) {
                return Some(t);
            }
            stack.extend(self.tile(t).children.iter().rev().copied());
        }
        None
    }

    fn read_trait(&self, me: TileId, src: &Src, name: &str, deps: &mut u64, sources: &mut Vec<(TileId, u32)>) -> Value {
        let Some(t) = self.resolve(me, src) else { return Value::default() };
        match self.tile(t).trait_index(name) {
            Some(k) => {
                let tr = &self.tile(t).traits[k];
                *deps = (*deps).max(tr.changed);
                sources.push((t, k as u32));
                tr.value.clone()
            }
            // A trait not there reads 0 [hypothesis].
            None => Value::default(),
        }
    }

    /// Works out an operator list on tile `me`; `deps` takes the latest change count of
    /// anything read.
    pub fn eval(&self, me: TileId, ops: &[Op], deps: &mut u64) -> Value {
        self.eval_reading(me, ops, deps, &mut Vec::new())
    }

    /// [`Ui::eval`], and the traits read (tile, index) put in `sources`. What it reads
    /// depends only on the tree's shape and the values read (`_filename_<n>` on a value
    /// worked out before), so while neither changed it reads the same.
    fn eval_reading(&self, me: TileId, ops: &[Op], deps: &mut u64, sources: &mut Vec<(TileId, u32)>) -> Value {
        let mut acc = Value::default();
        for (k, op) in ops.iter().enumerate() {
            let arg = match &op.arg {
                Arg::None => Value::default(),
                Arg::Const(v) => v.clone(),
                Arg::Seq(inner) => self.eval_reading(me, inner, deps, sources),
                Arg::Ref { src, name } => {
                    // `<copy … trait="_filename_"/>` after a number reads `_filename_<n>`
                    // [hypothesis: the meter and barter files pick a picture so]; after a
                    // constant with a known number, by it (`&false;` 0, `&true;` 1,
                    // menu-xml.md §2.3; an unknown constant names no suffix) [hypothesis:
                    // the meter prefab's `_SolidMeter` and vertical_fade_line.xml's
                    // `_bottom_to_top` are `&false;`/`&true;` before the copy, and their
                    // `_filename_0`/`_Filename_1` are the two pictures; T15a].
                    let name = match (&acc, name.ends_with('_')) {
                        (Value::Num(n), true) if op.kind == OpKind::Copy => format!("{name}{}", *n as i64),
                        (Value::Sym(s), true) if op.kind == OpKind::Copy && crate::value::sym_number(s).is_some() => format!("{name}{}", acc.num() as i64),
                        _ => name.clone(),
                    };
                    self.read_trait(me, src, &name, deps, sources)
                }
            };
            acc = apply(op.kind, acc, arg, me, k);
        }
        acc
    }
}

fn is_tile(e: &Element) -> bool {
    TILE_KINDS.iter().any(|k| e.is(k))
}

fn uio_include(file: &str) -> Node {
    Node::Element(Element {
        name: "include".into(),
        attrs: vec![("src".into(), file.into()), ("nvf_uio".into(), "true".into())],
        ..Element::default()
    })
}

fn named_nodes<'a>(nodes: &'a mut [Node], name: &str) -> Option<&'a mut Vec<Node>> {
    for node in nodes {
        if let Node::Element(e) = node {
            if is_tile(e) && e.attr("name").is_some_and(|n| n.eq_ignore_ascii_case(name)) {
                return Some(&mut e.children);
            }
            if let Some(found) = named_nodes(&mut e.children, name) {
                return Some(found);
            }
        }
    }
    None
}

/// An include's file: under `menus\prefabs\` [observed: every vanilla include names a
/// file there; MCM's `MCM\MCM.xml` a file in a folder under it].
fn prefab_path(file: &str) -> String {
    format!("menus\\prefabs\\{}", file.trim().replace('/', "\\"))
}

fn split(path: &str) -> Vec<&str> {
    path.split(['/', '\\']).map(str::trim).filter(|s| !s.is_empty()).collect()
}

fn apply(kind: OpKind, acc: Value, a: Value, me: TileId, k: usize) -> Value {
    let (x, y) = (acc.num(), a.num());
    let num = Value::Num;
    match kind {
        OpKind::Copy | OpKind::Ref => a,
        OpKind::Add => num(x + y),
        OpKind::Sub => num(x - y),
        OpKind::Mul => num(x * y),
        // Dividing by 0 leaves the value [hypothesis].
        OpKind::Div => num(if y == 0.0 { x } else { x / y }),
        OpKind::Mod => num(if y == 0.0 { x } else { x % y }),
        OpKind::Min => num(x.min(y)),
        OpKind::Max => num(x.max(y)),
        // The rounding operators work on the value; what is written in them (always 0
        // or 1 in the vanilla files [observed]) is not used [hypothesis].
        OpKind::Floor => num(x.floor()),
        OpKind::Ceil => num(x.ceil()),
        OpKind::Round => num(x.round()),
        OpKind::Trunc => num(x.trunc()),
        OpKind::Abs => num(x.abs()),
        // A whole number from 0 below the argument, the same for this tile and operator
        // every time [design: when the original draws is not known].
        OpKind::Rand => {
            let n = y.max(1.0) as u64;
            let h = (me.index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (k as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            num((h >> 33) as f64 % n as f64)
        }
        OpKind::Eq => Value::from_bool(acc.same(&a)),
        OpKind::Neq => Value::from_bool(!acc.same(&a)),
        OpKind::Lt => Value::from_bool(x < y),
        OpKind::Lte => Value::from_bool(x <= y),
        OpKind::Gt => Value::from_bool(x > y),
        OpKind::Gte => Value::from_bool(x >= y),
        OpKind::And => Value::from_bool(acc.truthy() && a.truthy()),
        OpKind::Or => Value::from_bool(acc.truthy() || a.truthy()),
        OpKind::Not => Value::from_bool(!a.truthy()),
        // A false condition makes what was worked out so far 0, and the operators after
        // it go on from there [hypothesis: fits every vanilla use, menu-xml.md §4.3].
        OpKind::OnlyIf => {
            if a.truthy() {
                acc
            } else {
                Value::default()
            }
        }
        OpKind::OnlyIfNot => {
            if a.truthy() {
                Value::default()
            } else {
                acc
            }
        }
    }
}

#[cfg(test)]
mod write_serial_tests {
    use super::*;

    #[test]
    fn trait_write_serial_exhaustion_keeps_legacy_values_without_reusing_tokens() {
        let mut ui = Ui::new();
        let screen = ui.screen();
        ui.set_trait(screen, "x", Value::Num(12.0));
        let old = ui.trait_write_serial(screen, "x").unwrap();
        let clock = ui.last_write_serial;
        assert!(!ui.set_float("screen()/missing", 1.0));
        assert!(!ui.set_existing(screen, "missing", Value::Num(1.0)));
        assert_eq!(ui.last_write_serial, clock);
        ui.last_write_serial = u64::MAX - 1;
        ui.set_trait(screen, "x", Value::Num(12.0));
        let last = ui.trait_write_serial(screen, "x").unwrap();
        assert_ne!(old, last);
        assert_eq!(ui.last_write_serial, u64::MAX);
        ui.set_trait(screen, "x", Value::Num(13.0));
        assert_eq!(ui.trait_write_serial(screen, "x"), None);
        assert_eq!(ui.tile(screen).get("x"), Some(&Value::Num(13.0)));
        ui.set_trait(screen, "x", Value::Num(14.0));
        assert_eq!(ui.tile(screen).get("x"), Some(&Value::Num(14.0)));
        assert_eq!(ui.trait_write_serial(screen, "x"), None);
        let src = MemSource::default();
        assert!(ui.inject_text(&src, "screen()", "<x>15</x><y>24</y>"));
        assert_eq!(ui.tile(screen).get("x"), Some(&Value::Num(15.0)));
        assert_eq!(ui.tile(screen).get("y"), Some(&Value::Num(24.0)));
        assert_eq!(ui.trait_write_serial(screen, "x"), None);
        assert_eq!(ui.trait_write_serial(screen, "y"), None);
        assert_eq!(ui.last_write_serial, u64::MAX);
    }
}
