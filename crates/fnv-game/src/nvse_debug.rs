//! xNVSE's debug and documentation functions on the game side (docs/handoff/
//! xnvse-remaining.md §3, "debug and docs"): GetCommandOpcode and GetDebugSelection. The
//! VM answers PrintDebug, GetDebugMode, Assert and Ar_DumpID. The file-writing ones
//! (PrintF, PrintDebugF, Ar_DumpF, DecompileScript, DumpDocs, DumpCommandWikiDoc) wait on
//! the owner's decision on where extender files go (owner-review, J9), and CompileScript
//! on reading UDF files from Data\NVSE\user_defined_functions.

use crate::game::Game;
use fnv_script::vm::XVal;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Every command's opcode by lower-case name or alias, as GetCommandOpcode gave them in
/// the original game [observed: schemas/script/command_opcodes.json, xNVSE 6.4.9, JIP LN
/// 57.30, JohnnyGuitar 528; it agrees with all 455 vanilla opcodes the catalogue holds].
fn opcodes() -> &'static HashMap<String, u32> {
    static TABLE: OnceLock<HashMap<String, u32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let doc: serde_json::Value =
            serde_json::from_str(include_str!("../../../schemas/script/command_opcodes.json")).unwrap_or_default();
        doc["opcodes"]
            .as_object()
            .map(|m| m.iter().filter_map(|(k, v)| Some((k.to_ascii_lowercase(), u32::try_from(v.as_u64()?).ok()?))).collect())
            .unwrap_or_default()
    })
}

/// Every name and alias the opcode table gives `code`, as spelled there.
pub(crate) fn opcode_names(code: u32) -> Vec<String> {
    static TABLE: OnceLock<HashMap<u32, Vec<String>>> = OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let doc: serde_json::Value =
            serde_json::from_str(include_str!("../../../schemas/script/command_opcodes.json")).unwrap_or_default();
        let mut by: HashMap<u32, Vec<String>> = HashMap::new();
        if let Some(m) = doc["opcodes"].as_object() {
            for (k, v) in m {
                if let Some(c) = v.as_u64().and_then(|c| u32::try_from(c).ok()) {
                    by.entry(c).or_default().push(k.clone());
                }
            }
        }
        by
    });
    t.get(&code).cloned().unwrap_or_default()
}

/// A command's opcode by name or alias, any case.
pub(crate) fn opcode_of(name: &str) -> Option<u32> {
    opcodes().get(&name.to_ascii_lowercase()).copied()
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(_g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let v = match name {
        // The opcode of a command by name or alias, any case; 0 for no such command (or
        // one of a plugin NVForever does not stand in for) [documented: GetCommandOpcode,
        // Activate 4109; observed: nvse-sixth, opcode.* rows].
        "getcommandopcode" => {
            let n = match args.first() {
                Some(XVal::Str(s)) => s.to_ascii_lowercase(),
                _ => String::new(),
            };
            XVal::Num(f64::from(opcodes().get(&n).copied().unwrap_or(0)))
        }
        // The reference last selected in the console [documented: GetDebugSelection];
        // none when nothing is [observed: nvse-sixth, 00000000]. The selection is the
        // engine's (prid and clicks send it a request), so the game side reads none until
        // the engine tells it.
        "getdebugselection" => XVal::Ref(0),
        _ => return None,
    };
    Some(Ok(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_table_has_the_observed_values() {
        let t = opcodes();
        assert_eq!(t.get("activate"), Some(&4109));
        assert_eq!(t.get("getnvseversion"), Some(&0x1400));
        assert_eq!(t.get("sv_length"), Some(&0x156E));
        assert_eq!(t.get("getlnversion"), Some(&0x2200));
        assert_eq!(t.get("gettype"), t.get("getobjecttype"));
        assert!(!t.contains_key("nvfnosuchcommand"));
    }
}
