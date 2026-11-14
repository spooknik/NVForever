//! Conditions (`CTDA`): the layout from the Layer 1 schema (`common.json`, type
//! `Condition`), the parameters typed by the xEdit parameter types the function table
//! carries (`xedit_param_types` in `schemas/script/functions.json`), and the AND/OR
//! chaining. Evaluation lives in [`crate::game`] because it calls the same function
//! implementations as scripts (one table for both: index = opcode − 0x1000,
//! docs/spec-notes/script-functions.md §4). Notes: docs/spec-notes/scripts.md §6.

use fnv_script::vm::{ArgValue, Value};
use serde::Serialize;
use serde_json::Value as Json;
use std::collections::HashMap;

/// The comparison in the top three bits of the type byte [documented: xEdit
/// wbDefinitionsFNV.pas, CTDA "Type": Equal to, Not equal to, Greater than, Greater than
/// or equal to, Less than, Less than or equal to].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

impl Cmp {
    fn from_type(t: u8) -> Option<Cmp> {
        Some(match t >> 5 {
            0 => Cmp::Eq,
            1 => Cmp::Ne,
            2 => Cmp::Gt,
            3 => Cmp::Ge,
            4 => Cmp::Lt,
            5 => Cmp::Le,
            _ => return None,
        })
    }

    pub fn test(self, a: f64, b: f64) -> bool {
        match self {
            Cmp::Eq => a == b,
            Cmp::Ne => a != b,
            Cmp::Gt => a > b,
            Cmp::Ge => a >= b,
            Cmp::Lt => a < b,
            Cmp::Le => a <= b,
        }
    }
}

/// Type byte flags [documented: xEdit, CTDA "Type" flags: Or, Run on target, Use global].
pub const FLAG_OR: u8 = 0x01;
pub const FLAG_RUN_ON_TARGET: u8 = 0x02;
pub const FLAG_USE_GLOBAL: u8 = 0x04;

/// Who the function runs on [documented: xEdit, CTDA "Run On": Subject, Target,
/// Reference, Combat Target, Linked Reference].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "on", content = "reference", rename_all = "snake_case")]
pub enum RunOn {
    Subject,
    Target,
    /// The reference stored in the condition (`None` when it does not map).
    Reference(Option<u32>),
    CombatTarget,
    LinkedReference,
    Other(u32),
}

/// What the function's result is compared with.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Operand {
    Value(f64),
    /// "Use global": the comparison value is a GLOB, read when evaluated.
    Global(Option<u32>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Condition {
    pub cmp: Cmp,
    /// Joined to the next condition with OR.
    pub or: bool,
    pub run_on: RunOn,
    /// Function index (opcode − 0x1000).
    pub function: u16,
    #[serde(skip)]
    pub params: Vec<ArgValue>,
    pub rhs: Operand,
}

impl Condition {
    pub fn opcode(&self) -> u16 {
        0x1000 + self.function
    }
}

/// xEdit parameter types per condition function index.
#[derive(Debug, Clone, Default)]
pub struct ParamTypes(pub HashMap<u16, Vec<String>>);

impl ParamTypes {
    /// From the parsed `functions.json`.
    pub fn from_catalogue(functions: &Json) -> ParamTypes {
        let mut m = HashMap::new();
        for f in functions.get("functions").and_then(Json::as_array).into_iter().flatten() {
            if let Some(i) = f.get("condition_index").and_then(Json::as_u64) {
                let t: Vec<String> = f
                    .get("xedit_param_types")
                    .and_then(Json::as_array)
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                    .unwrap_or_default();
                m.insert(i as u16, t);
            }
        }
        // Condition functions with no page (so no entry) in the catalogue, by xEdit's
        // table [documented: xEdit wbCTDAFunctions; animation.md §1.2].
        for (i, t) in XEDIT_ONLY {
            m.entry(*i).or_insert_with(|| t.iter().map(|s| s.to_string()).collect());
        }
        // These two named extender getters already have portable implementations and
        // observed opcodes in schemas/script/command_opcodes.json. The catalogue lists
        // a perk-form parameter for GetPerkRank and none for GetPCUsingIronSights, but
        // has no condition_index metadata for them. Keep this admission limited to
        // those two aliases (extender-condition-aliases.md).
        m.entry(GET_PERK_RANK).or_insert_with(|| vec!["ptPerk".into()]);
        m.entry(GET_PC_USING_IRON_SIGHTS).or_default();
        ParamTypes(m)
    }
}

/// Condition indices are the existing observed command opcodes minus 0x1000.
pub(crate) const GET_PERK_RANK: u16 = 0x05C3;
pub(crate) const GET_PC_USING_IRON_SIGHTS: u16 = 0x177A;

/// Condition indices xEdit knows that the catalogue has no entry for, with their
/// parameter types: 451 IsLastIdlePlayed takes an IDLE; 518–521 and 525–528, the VATS
/// area functions (`crate::vats::is_area_function`), a reference [documented: xEdit
/// wbCTDAFunctions].
const XEDIT_ONLY: &[(u16, &[&str])] = &[
    (451, &["ptIdleForm"]),
    (518, &["ptReference"]),
    (519, &["ptReference"]),
    (520, &["ptReference"]),
    (521, &["ptReference"]),
    (525, &["ptReference"]),
    (526, &["ptReference"]),
    (527, &["ptReference"]),
    (528, &["ptReference"]),
];

/// How a raw 4-byte parameter is read for an xEdit parameter type. Types that name a
/// record are FormIDs (mapped through the plugin's master list); enumerations are codes;
/// the rest are integers (a quest stage, an objective, a variable index) [documented:
/// xEdit parameter types; the reading per type is ours].
pub(crate) fn param(ty: Option<&str>, raw: u32, map: &dyn Fn(u32) -> Option<u32>) -> ArgValue {
    const INTS: &[&str] = &["ptInteger", "ptQuestStage", "ptQuestObjective", "ptVariableName", "ptVATSValueParam"];
    const ENUMS: &[&str] = &[
        "ptActorValue",
        "ptSex",
        "ptAxis",
        "ptMiscStat",
        "ptCriticalStage",
        "ptMenuMode",
        "ptFormType",
        "ptAlignment",
        "ptEquipType",
        "ptBodyLocation",
        "ptPlayerAction",
        "ptCreatureType",
        "ptVATSValueFunction",
        "ptCrimeType",
    ];
    match ty {
        None => ArgValue::Value(Value::Num(raw as i32 as f64)),
        Some(t) if INTS.contains(&t) => ArgValue::Value(Value::Num(raw as i32 as f64)),
        Some(t) if ENUMS.contains(&t) => ArgValue::Enum(raw as u16),
        Some(_) if raw == 0 => ArgValue::Value(Value::NONE),
        Some(_) => ArgValue::Value(Value::Ref(map(raw).unwrap_or(0))),
    }
}

/// Read one CTDA from its schema-decoded value. `map` turns a FormID local to the
/// record's plugin into a load-order FormID.
pub fn parse(v: &Json, types: &ParamTypes, map: &dyn Fn(u32) -> Option<u32>) -> Result<Condition, String> {
    let n = |k: &str| v.get(k).and_then(Json::as_u64).ok_or_else(|| format!("CTDA without {k}"));
    let ty = n("type")? as u8;
    let cmp = Cmp::from_type(ty).ok_or_else(|| format!("CTDA comparison {}", ty >> 5))?;
    let function = n("function")? as u16;
    let raw_value = n("comparison_value_raw")? as u32;
    let rhs = if ty & FLAG_USE_GLOBAL != 0 {
        Operand::Global(map(raw_value))
    } else {
        Operand::Value(f32::from_bits(raw_value) as f64)
    };
    let pt = types.0.get(&function);
    let p1 = param(pt.and_then(|t| t.first()).map(String::as_str), n("parameter1_raw")? as u32, map);
    let p2 = param(pt.and_then(|t| t.get(1)).map(String::as_str), n("parameter2_raw")? as u32, map);
    let run_on = match v.get("run_on").and_then(Json::as_u64) {
        None | Some(0) if ty & FLAG_RUN_ON_TARGET != 0 => RunOn::Target,
        None | Some(0) => RunOn::Subject,
        Some(1) => RunOn::Target,
        Some(2) => RunOn::Reference(v.get("reference_raw").and_then(Json::as_u64).and_then(|r| map(r as u32))),
        Some(3) => RunOn::CombatTarget,
        Some(4) => RunOn::LinkedReference,
        Some(o) => RunOn::Other(o as u32),
    };
    // Functions take at most two parameters in a condition; trailing ones the function
    // does not have are ignored by the implementations.
    let nparams = pt.map(|t| t.len()).unwrap_or(2).min(2);
    let params = [p1, p2].into_iter().take(nparams).collect();
    Ok(Condition { cmp, or: ty & FLAG_OR != 0, run_on, function, params, rhs })
}

/// Combine evaluated conditions: a condition with the OR flag is joined to the next one;
/// the OR groups are joined with AND, so OR binds tighter than AND [documented: GECK
/// wiki Category:Conditions, "AND/OR Precedence": consecutive ORs are one block, and
/// A AND B OR C AND D is A AND (B OR C) AND D]. An empty list passes.
pub fn combine(results: &[(bool, bool)]) -> bool {
    combine_iter(results.iter().copied())
}

/// [`combine`] over (passed, or-flag) pairs as they come.
pub fn combine_iter(results: impl IntoIterator<Item = (bool, bool)>) -> bool {
    let mut all = true;
    let mut group = false;
    let mut last_or = false;
    for (passed, or) in results {
        group |= passed;
        last_or = or;
        if !or {
            all &= group;
            group = false;
        }
    }
    // A trailing OR flag on the last condition: its group still counts.
    if last_or {
        all &= group;
    }
    all
}

/// One condition as the game evaluated it (`Game::eval_conditions`): what the editor's
/// debugger shows, from the same evaluation the game decides by
/// (docs/design/editor.md §14.3).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CondResult {
    /// The function's value.
    pub value: f64,
    /// What it was compared with (the operand, or the global's value).
    pub rhs: f64,
    pub passed: bool,
    /// Joined to the next condition with OR.
    pub or: bool,
    /// The reference the function ran on (`None`: nothing to run on).
    pub on: Option<u32>,
}

/// [`combine`] over evaluated conditions.
pub fn combine_results(results: &[CondResult]) -> bool {
    combine_iter(results.iter().map(|r| (r.passed, r.or)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn or_binds_tighter_than_and() {
        // (A or B) and C
        assert!(combine(&[(false, true), (true, false), (true, false)]));
        assert!(!combine(&[(false, true), (false, false), (true, false)]));
        assert!(!combine(&[(true, true), (false, false), (false, false)]));
        assert!(combine(&[]));
        assert!(combine(&[(false, true), (true, true)]));
    }

    #[test]
    fn parse_reads_type_function_and_params() {
        let mut types = ParamTypes::default();
        types.0.insert(59, vec!["ptQuest".into(), "ptQuestStage".into()]);
        let v = json!({"type": 0x60 | FLAG_OR, "comparison_value_raw": 1.0f32.to_bits(), "function": 59,
                        "parameter1_raw": 0x0100_0ABC, "parameter2_raw": 10, "run_on": 0, "reference_raw": 0});
        let c = parse(&v, &types, &|l| Some(l & 0x00FF_FFFF | 0x0200_0000)).unwrap();
        assert_eq!(c.cmp, Cmp::Ge);
        assert!(c.or);
        assert_eq!(c.opcode(), 0x103B);
        assert_eq!(c.params, vec![ArgValue::Value(Value::Ref(0x0200_0ABC)), ArgValue::Value(Value::Num(10.0))]);
        assert_eq!(c.rhs, Operand::Value(1.0));
        assert_eq!(c.run_on, RunOn::Subject);
        let g = json!({"type": FLAG_USE_GLOBAL | 0x80, "comparison_value_raw": 0x33, "function": 1,
                        "parameter1_raw": 0, "parameter2_raw": 0, "run_on": 2, "reference_raw": 0x14});
        let c = parse(&g, &types, &|l| Some(l)).unwrap();
        assert_eq!(c.cmp, Cmp::Lt);
        assert_eq!(c.rhs, Operand::Global(Some(0x33)));
        assert_eq!(c.run_on, RunOn::Reference(Some(0x14)));
    }
}
