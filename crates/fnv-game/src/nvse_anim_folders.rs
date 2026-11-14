//! kNVSE's AnimGroupOverride folders (#1461; docs/spec-notes/knvse-anim-folders.md):
//! animation files a mod puts under `meshes\AnimGroupOverride`, with JSON files there
//! saying which forms they are for and on what condition, replace the animations of
//! actors with no plugin and no script [documented: CopyAnimationsToForm, "the kNVSE
//! AnimGroupOverride animations, including conditions and other attributes"; observed:
//! the file trees of 25 installed mods, 2026-11-03]. No kNVSE source is read.
//!
//! The game keeps the entries read once from the folders and, a few times a second, the
//! files each drawn actor's matching entries give it ([`AnimFolders::active`]), which the
//! engine adds to SetActorAnimationPath's replacements (`Game::anim_overrides`).

use crate::game::Game;
use fnv_sim::FormId;
use serde_json::Value as Json;
use std::collections::{HashMap, HashSet};

/// The folder under meshes.
pub const ROOT: &str = "AnimGroupOverride";

/// How often the actors' entries are checked again, in frames (at 60 a second, four
/// times a second) [decision: the original checks a condition when an animation starts,
/// or every frame with `pollCondition` (hypothesis); a quarter second is close enough
/// for both and keeps the cost of the conditions small].
const REFRESH_FRAMES: u64 = 15;

/// One entry of a JSON file.
#[derive(Debug, Clone)]
pub struct FolderEntry {
    /// The forms it is for: actor references, base actors or weapons; empty for every
    /// actor [observed: 68 of 778 entries have no `mod`/`form`, their conditions test
    /// the actor (`GetCreatureType == -1`, `GetIsSex Female`)].
    pub forms: Vec<FormId>,
    /// The folder under [`ROOT`], as the file spells it.
    pub folder: String,
    /// A script condition, tested on the actor [observed: `this != Player`].
    pub condition: Option<String>,
    /// Higher wins over lower [hypothesis: values 1, 2 and 10 next to entries without
    /// one, the more special case given the higher number].
    pub priority: i64,
    /// Where it came in the files read (later wins at the same priority).
    pub order: usize,
    /// Its third-person (`_male`) and first-person (`_1stPerson`) files, under meshes.
    pub third: Vec<String>,
    pub first: Vec<String>,
}

/// What the folders give.
#[derive(Debug, Default)]
pub struct AnimFolders {
    loaded: bool,
    pub entries: Vec<FolderEntry>,
    /// The files each actor's entries give now, by (actor, first person), the lowest
    /// priority first (a later file of the same group wins, as with the engine's
    /// replacements).
    pub active: HashMap<(FormId, bool), Vec<String>>,
    /// Condition texts that failed, warned about once.
    warned: HashSet<String>,
}

/// An entry's `form`: one hex ID or a list of them [observed: both, 254 lists].
fn form_ids(v: Option<&Json>) -> Vec<u32> {
    let one = |v: &Json| match v {
        Json::String(s) => u32::from_str_radix(s.trim().trim_start_matches("0x").trim_start_matches("0X"), 16).ok(),
        Json::Number(n) => n.as_u64().map(|n| n as u32),
        _ => None,
    };
    match v {
        Some(Json::Array(a)) => a.iter().filter_map(one).collect(),
        Some(v) => one(v).into_iter().collect(),
        None => Vec::new(),
    }
}

/// A folder's files by view: the first part after the folder names it (`_male`,
/// `_1stPerson`, any case) [observed: 3,974 and 4,441 files; no `_female`]. Files under
/// a `Mod1`–`Mod3` folder are left out: they look like the files for a weapon with that
/// mod [hypothesis: `_1stPerson\Mod1\1hpReloadL v2.14.kf` beside the plain reload;
/// follow-up]. Only `.kf` files count (a `chances.txt` and `.bak` files sit beside them).
fn folder_files(folder: &str) -> (Vec<String>, Vec<String>) {
    let (mut third, mut first) = (Vec::new(), Vec::new());
    let base = format!("meshes\\{ROOT}\\{folder}");
    for rel in crate::mod_files::files_under(&base) {
        if !rel.to_ascii_lowercase().ends_with(".kf") {
            continue;
        }
        let parts: Vec<&str> = rel.split('\\').collect();
        if parts.iter().any(|p| ["mod1", "mod2", "mod3"].contains(&p.to_ascii_lowercase().as_str())) {
            continue;
        }
        let path = format!("{ROOT}\\{folder}\\{rel}");
        match parts.first().map(|p| p.to_ascii_lowercase()) {
            Some(v) if v == "_male" && parts.len() > 1 => third.push(path),
            Some(v) if v == "_1stperson" && parts.len() > 1 => first.push(path),
            _ => {}
        }
    }
    (third, first)
}

/// Every entry of the JSON files directly under the folder, the files read in name order
/// without regard to case [hypothesis: mods name files `1Another_Millenia_base.json` and
/// `zzz_heavy_fatman.json` to come first or last]. A file that is empty, not JSON, or an
/// entry without a folder, whose plugin is not loaded, that names a plugin but no form
/// [hypothesis: a slip, not an entry for every actor], or whose folder is missing is
/// logged and left out (the broken-files rule) [observed: 59 empty files, 20 entries
/// naming a folder that is not there].
fn load(g: &Game) -> Vec<FolderEntry> {
    let mut out = Vec::new();
    let root = format!("meshes\\{ROOT}");
    let files: Vec<String> = crate::mod_files::files_in(&root).into_iter().filter(|f| f.to_ascii_lowercase().ends_with(".json")).collect();
    // A folder's files, walked once however many entries name it.
    let mut walked: HashMap<String, (Vec<String>, Vec<String>)> = HashMap::new();
    for file in &files {
        let Some(bytes) = crate::mod_files::read_loose(&format!("{root}\\{file}")) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim_start_matches('\u{feff}').trim();
        if text.is_empty() {
            continue;
        }
        let items = match serde_json::from_str::<Json>(text) {
            Ok(Json::Array(a)) => a,
            Ok(v @ Json::Object(_)) => vec![v],
            Ok(_) => {
                tracing::warn!("AnimGroupOverride: {file}: not a list of entries");
                continue;
            }
            Err(e) => {
                tracing::warn!("AnimGroupOverride: {file}: {e}");
                continue;
            }
        };
        for it in items {
            let Some(folder) = it.get("folder").and_then(Json::as_str).map(|s| s.trim_matches(['\\', '/']).to_string()) else {
                tracing::warn!("AnimGroupOverride: {file}: an entry without a folder");
                continue;
            };
            let ids = form_ids(it.get("form"));
            let forms = match it.get("mod").and_then(Json::as_str) {
                Some(m) => {
                    let Some(index) = g.data.load_order.iter().position(|p| p.eq_ignore_ascii_case(m)) else {
                        tracing::debug!("AnimGroupOverride: {file}: {m} is not loaded");
                        continue;
                    };
                    if ids.is_empty() {
                        // Not every actor: a plugin was named, the form was left out.
                        tracing::warn!("AnimGroupOverride: {file}: an entry for {m} without a form");
                        continue;
                    }
                    ids.iter().map(|id| FormId(((index as u32) << 24) | (id & 0x00FF_FFFF))).collect()
                }
                None if ids.is_empty() => Vec::new(),
                // A form with no plugin: a full ID [hypothesis].
                None => ids.into_iter().map(FormId).collect(),
            };
            let (third, first) = walked.entry(folder.to_ascii_lowercase()).or_insert_with(|| folder_files(&folder)).clone();
            if third.is_empty() && first.is_empty() {
                tracing::warn!("AnimGroupOverride: {file}: folder {folder} has no animation files");
                continue;
            }
            let condition = it.get("condition").and_then(Json::as_str).map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
            let priority = it.get("priority").and_then(Json::as_i64).unwrap_or(0);
            out.push(FolderEntry { forms, folder, condition, priority, order: out.len(), third, first });
        }
    }
    if !out.is_empty() {
        tracing::info!("AnimGroupOverride: {} entries from {} files", out.len(), files.len());
    }
    out
}

impl Game<'_> {
    /// The files an actor's animations are replaced with, in the order they apply (a
    /// later file of a group wins): the folders' first, then SetActorAnimationPath's
    /// [hypothesis: a script's call wins over the files a mod installed].
    ///
    /// A script's path with a condition is left out here (no script can run from a
    /// shared borrow): [`Game::anim_overrides_now`] runs the conditions. A folder path
    /// gives its KF files.
    pub fn anim_overrides(&self, actor: FormId, first: bool) -> Vec<String> {
        let mut v = self.anim_folders.active.get(&(actor, first)).cloned().unwrap_or_default();
        v.extend(self.weapon_overrides(actor, first));
        for p in self.anim_paths.actor.get(&(actor, first)).into_iter().flatten() {
            if !self.anim_paths.conditions.contains_key(&(actor, first, crate::nvse_anim::path_key(p))) {
                v.extend(crate::nvse_anim::kf_files(p));
            }
        }
        v
    }

    /// SetWeaponAnimationPath's files for the weapon the actor has equipped [documented:
    /// SetWeaponAnimationPath, "replaces an animation for a specified weapon base form";
    /// hypothesis: they apply to whoever holds it, before the actor's own paths, which
    /// win over them]. A folder path gives its KF files.
    fn weapon_overrides(&self, actor: FormId, first: bool) -> Vec<String> {
        let held = self.world.peek(actor).and_then(|s| s.inventory.as_ref()).and_then(|inv| {
            inv.equipped().into_iter().find(|&i| self.world.forms.item(i).is_some_and(|it| it.kind == fnv_sim::forms::ItemKind::Weapon))
        });
        let Some(w) = held else { return Vec::new() };
        self.anim_paths.weapon.get(&(w, first)).into_iter().flatten().flat_map(|p| crate::nvse_anim::kf_files(p)).collect()
    }

    /// As [`Game::anim_overrides`], with SetActorAnimationPath's conditional paths whose
    /// condition holds now: each condition function runs on the actor (`this`) and a
    /// true result keeps its path [documented: SetActorAnimationPath, the condition
    /// function; hypothesis: evaluated each time the engine asks, every frame, which
    /// covers `pollConditionFunction` 1]. A condition that fails to run counts false.
    pub fn anim_overrides_now(&mut self, actor: FormId, first: bool) -> Vec<String> {
        let mut v = self.anim_folders.active.get(&(actor, first)).cloned().unwrap_or_default();
        v.extend(self.weapon_overrides(actor, first));
        let paths = self.anim_paths.actor.get(&(actor, first)).cloned().unwrap_or_default();
        for p in paths {
            let holds = match self.anim_paths.conditions.get(&(actor, first, crate::nvse_anim::path_key(&p))).cloned() {
                None => true,
                Some(c) => self.run_function(&c, Some(fnv_script::vm::Value::Ref(actor.0)), Vec::new(), Some(actor), Default::default()).is_ok_and(|r| r.truthy()),
            };
            if holds {
                v.extend(crate::nvse_anim::kf_files(&p));
            }
        }
        v
    }

    /// Read the folders once (when the Data folder is known), then every
    /// [`REFRESH_FRAMES`] the entries of the player and each actor the engine draws.
    pub(crate) fn anim_folders_tick(&mut self) {
        self.load_anim_folders();
        if self.anim_folders.entries.is_empty() || self.frame % REFRESH_FRAMES != 1 {
            return;
        }
        self.refresh_anim_folders();
    }

    /// Read the folders, once the Data folder is known.
    fn load_anim_folders(&mut self) {
        if !self.anim_folders.loaded && crate::mod_files::data_dir().is_some() {
            self.anim_folders.loaded = true;
            self.anim_folders.entries = load(self);
        }
    }

    /// The entries that hold for each actor now.
    pub fn refresh_anim_folders(&mut self) {
        let mut actors: Vec<FormId> = self.senses.anim.keys().copied().collect();
        if !actors.contains(&FormId::PLAYER_REF) {
            actors.push(FormId::PLAYER_REF);
        }
        let entries = std::mem::take(&mut self.anim_folders.entries);
        let mut active = HashMap::new();
        for a in actors {
            let base = crate::functions::base_of(self, a);
            let weapon = crate::functions::items::equipped_weapon(self, a);
            let mut hold: Vec<&FolderEntry> = Vec::new();
            for e in &entries {
                if !e.forms.is_empty() && !e.forms.iter().any(|&f| f == a || Some(f) == base || Some(f) == weapon) {
                    continue;
                }
                if let Some(c) = &e.condition {
                    match self.evaluate_on(c, Some(a)) {
                        Ok(true) => {}
                        Ok(false) => continue,
                        Err(err) => {
                            if self.anim_folders.warned.insert(c.clone()) {
                                tracing::warn!("AnimGroupOverride: {}: condition {c:?}: {err}", e.folder);
                            }
                            continue;
                        }
                    }
                }
                hold.push(e);
            }
            hold.sort_by_key(|e| (e.priority, e.order));
            for first in [false, true] {
                let files: Vec<String> = hold.iter().flat_map(|e| if first { e.first.iter() } else { e.third.iter() }).cloned().collect();
                if !files.is_empty() {
                    active.insert((a, first), files);
                }
            }
        }
        self.anim_folders.entries = entries;
        self.anim_folders.active = active;
    }

    /// CopyAnimationsToForm: the folders' entries for `from` are for `to` too, their
    /// conditions with them [documented: "including conditions and other attributes"].
    pub(crate) fn copy_folder_entries(&mut self, from: FormId, to: FormId) {
        self.load_anim_folders();
        for e in &mut self.anim_folders.entries {
            if e.forms.contains(&from) && !e.forms.contains(&to) {
                e.forms.push(to);
            }
        }
    }
}
