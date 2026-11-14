//! Load order, FormID remapping and record overriding.
//!
//! A FormID stored in a plugin is *file-local*: its top byte indexes that file's own
//! master list (MAST order), and the value equal to the master count means "this
//! file". At runtime the top byte is replaced by the load-order index of the file it
//! refers to. Later files in the load order override earlier definitions of the same
//! (global) FormID. [documented: UESP "Tes4Mod:Mod File Format", FormID section;
//! behaviour consistent with every shipped master, observed]

use crate::plugin::Plugin;
use crate::raw::{visit_records, Record};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadOrderError {
    #[error("{0} plugins; at most 255 can be loaded")]
    TooManyPlugins(usize),
    #[error("{plugin} requires master {master}, which is not in the load order")]
    MissingMaster { plugin: String, master: String },
    #[error("{plugin} requires master {master}, which loads after it")]
    MasterAfterDependent { plugin: String, master: String },
    #[error("{0} appears twice in the load order")]
    Duplicate(String),
}

/// Location of one record definition: (plugin load index, ordinal in that plugin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefRef {
    pub plugin: u8,
    pub ordinal: u32,
}

/// Every definition of a FormID, in load order. Nearly every FormID has one (780 of the
/// masters' 628,464 have more [observed 2026-11-12]), so the first is kept inline: no
/// allocation per record at each start (#1994).
pub type Defs = smallvec::SmallVec<[DefRef; 1]>;

pub struct LoadOrder<'a> {
    pub plugins: &'a [Plugin],
    /// For each plugin, maps its master-list index -> load-order index.
    master_map: Vec<Vec<u8>>,
    /// Records of each plugin in file order.
    pub records: Vec<Vec<&'a Record>>,
    /// Global FormID -> every definition in load order (last one wins).
    pub index: HashMap<u32, Defs>,
    /// Records whose own FormID could not be mapped (none since an out-of-range master
    /// index means the plugin itself; kept for reports).
    pub unmappable: Vec<(u8, u32)>,
}

impl<'a> LoadOrder<'a> {
    pub fn new(plugins: &'a [Plugin]) -> Result<LoadOrder<'a>, LoadOrderError> {
        if plugins.len() > 255 {
            return Err(LoadOrderError::TooManyPlugins(plugins.len()));
        }
        let lower: Vec<String> = plugins.iter().map(|p| p.name.to_ascii_lowercase()).collect();
        for (i, n) in lower.iter().enumerate() {
            if lower[..i].contains(n) {
                return Err(LoadOrderError::Duplicate(plugins[i].name.clone()));
            }
        }
        let mut master_map = Vec::with_capacity(plugins.len());
        for (i, p) in plugins.iter().enumerate() {
            let mut m = Vec::with_capacity(p.header.masters.len());
            for mr in &p.header.masters {
                let ml = mr.name.to_ascii_lowercase();
                match lower.iter().position(|n| *n == ml) {
                    Some(j) if j < i => m.push(j as u8),
                    Some(_) => {
                        return Err(LoadOrderError::MasterAfterDependent {
                            plugin: p.name.clone(),
                            master: mr.name.clone(),
                        })
                    }
                    None => {
                        return Err(LoadOrderError::MissingMaster { plugin: p.name.clone(), master: mr.name.clone() })
                    }
                }
            }
            master_map.push(m);
        }
        let mut lo = LoadOrder { plugins, master_map, records: Vec::new(), index: HashMap::new(), unmappable: Vec::new() };
        for (pi, p) in plugins.iter().enumerate() {
            let mut recs: Vec<&'a Record> = Vec::new();
            visit_records(&p.nodes[1..], &mut |r, _| recs.push(r));
            for (ord, r) in recs.iter().enumerate() {
                match lo.to_global(pi, r.header.form_id) {
                    Some(g) => {
                        // A plugin holding two records under one FormID keeps the first:
                        // oHUD.esm's QUST `aHUD` before its MESG `iHUDMainMenu` at
                        // 0x000801 is in the game a running quest with its variables, and
                        // the message's editor ID names nothing [observed: #2276 job B].
                        let defs = lo.index.entry(g).or_default();
                        if defs.last().is_none_or(|d| d.plugin as usize != pi) {
                            defs.push(DefRef { plugin: pi as u8, ordinal: ord as u32 });
                        }
                    }
                    None => {
                        tracing::debug!(
                            plugin = %p.name,
                            form_id = format_args!("{:08X}", r.header.form_id),
                            "FormID's master index is outside this plugin's master list"
                        );
                        lo.unmappable.push((pi as u8, r.header.form_id))
                    }
                }
            }
            lo.records.push(recs);
        }
        Ok(lo)
    }

    /// Convert a FormID local to plugin `pi` into a load-order (global) FormID.
    ///
    /// A master index at or beyond the plugin's master list means the plugin itself, as
    /// the game reads it [observed: capture `l1_oob_formid`, GRA's 0x02000801 placed as
    /// GRA's own; a mod whose own records all carry 0x12 with one master loads them in
    /// the game; docs/spec-notes/esm.md]. So every FormID maps; `None` is kept for callers.
    pub fn to_global(&self, pi: usize, local: u32) -> Option<u32> {
        let idx = (local >> 24) as usize;
        let masters = &self.master_map[pi];
        let target = if idx < masters.len() { masters[idx] } else { pi as u8 };
        Some(((target as u32) << 24) | (local & 0x00FF_FFFF))
    }

    /// Convert a global FormID into one local to plugin `pi`, if `pi` can refer to it.
    pub fn to_local(&self, pi: usize, global: u32) -> Option<u32> {
        let target = (global >> 24) as u8;
        let low = global & 0x00FF_FFFF;
        if target as usize == pi {
            return Some(((self.master_map[pi].len() as u32) << 24) | low);
        }
        self.master_map[pi].iter().position(|&m| m == target).map(|i| ((i as u32) << 24) | low)
    }

    /// The winning (last-loaded) definition of a global FormID.
    pub fn winner(&self, global: u32) -> Option<(&'a Plugin, &'a Record)> {
        let d = *self.index.get(&global)?.last()?;
        Some((&self.plugins[d.plugin as usize], self.records[d.plugin as usize][d.ordinal as usize]))
    }

    pub fn exists(&self, global: u32) -> bool {
        self.index.contains_key(&global)
    }

    pub fn master_map(&self, pi: usize) -> &[u8] {
        &self.master_map[pi]
    }
}

/// A plugin left out of a lenient load order ([`drop_unloadable`]), with why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedPlugin {
    pub plugin: String,
    pub reason: LoadOrderError,
}

/// The plugins [`LoadOrder::new`] accepts, in their order, and the ones left out: a
/// plugin whose master is missing or loads after it, a second copy of a name, and any
/// past the 255th. Nothing is reordered. A plugin dropped here takes its dependents with
/// it (masters load first, so one pass in load order is enough). For the game, which
/// must start with a broken mod in the list; tools keep `LoadOrder::new`'s errors.
pub fn drop_unloadable(plugins: Vec<Plugin>) -> (Vec<Plugin>, Vec<DroppedPlugin>) {
    let masters: Vec<Vec<&str>> = plugins.iter().map(|p| p.header.masters.iter().map(|m| m.name.as_str()).collect()).collect();
    let names: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
    let (keep, dropped) = loadable(&names, &masters);
    let kept = plugins.into_iter().zip(keep).filter_map(|(p, k)| k.then_some(p)).collect();
    (kept, dropped)
}

/// [`drop_unloadable`] on names: which to keep, and why the others go.
fn loadable(names: &[&str], masters: &[Vec<&str>]) -> (Vec<bool>, Vec<DroppedPlugin>) {
    let lower: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    let mut keep = vec![false; names.len()];
    let mut dropped = Vec::new();
    let mut kept = 0usize;
    for i in 0..names.len() {
        let is_kept = |n: &str| (0..i).any(|j| keep[j] && lower[j] == n);
        let reason = if is_kept(&lower[i]) {
            Some(LoadOrderError::Duplicate(names[i].to_string()))
        } else if kept >= 255 {
            Some(LoadOrderError::TooManyPlugins(names.len()))
        } else {
            masters[i].iter().find(|m| !is_kept(&m.to_ascii_lowercase())).map(|m| {
                let (plugin, master) = (names[i].to_string(), m.to_string());
                if lower[i..].contains(&m.to_ascii_lowercase()) {
                    LoadOrderError::MasterAfterDependent { plugin, master }
                } else {
                    LoadOrderError::MissingMaster { plugin, master }
                }
            })
        };
        match reason {
            Some(reason) => dropped.push(DroppedPlugin { plugin: names[i].to_string(), reason }),
            None => {
                keep[i] = true;
                kept += 1;
            }
        }
    }
    (keep, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(list: &[(&str, &[&str])]) -> (Vec<bool>, Vec<String>) {
        let names: Vec<&str> = list.iter().map(|(n, _)| *n).collect();
        let masters: Vec<Vec<&str>> = list.iter().map(|(_, m)| m.to_vec()).collect();
        let (keep, dropped) = loadable(&names, &masters);
        (keep, dropped.iter().map(|d| d.reason.to_string()).collect())
    }

    #[test]
    fn a_plugin_without_its_master_goes_and_takes_its_dependents() {
        let (keep, why) = run(&[
            ("FalloutNV.esm", &[]),
            ("Mod.esm", &["FalloutNV.esm", "Gone.esm"]),
            ("Patch.esp", &["FalloutNV.esm", "Mod.esm"]),
            ("Fine.esp", &["FalloutNV.esm"]),
        ]);
        assert_eq!(keep, [true, false, false, true]);
        assert_eq!(why[0], "Mod.esm requires master Gone.esm, which is not in the load order");
        assert_eq!(why[1], "Patch.esp requires master Mod.esm, which is not in the load order");
    }

    #[test]
    fn a_master_loading_after_its_dependent_drops_the_dependent() {
        let (keep, why) = run(&[("FalloutNV.esm", &[]), ("A.esp", &["B.esm"]), ("B.esm", &["falloutnv.ESM"]), ("b.esm", &[])]);
        assert_eq!(keep, [true, false, true, false]);
        assert_eq!(why[0], "A.esp requires master B.esm, which loads after it");
        assert_eq!(why[1], "b.esm appears twice in the load order");
    }

    #[test]
    fn no_more_than_255_plugins() {
        let names: Vec<String> = (0..300).map(|i| format!("P{i}.esp")).collect();
        let list: Vec<(&str, &[&str])> = names.iter().map(|n| (n.as_str(), &[][..])).collect();
        let (keep, why) = run(&list);
        assert_eq!(keep.iter().filter(|k| **k).count(), 255);
        assert_eq!(why.len(), 45);
        assert!(keep[254] && !keep[255]);
    }
}
