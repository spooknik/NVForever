//! Which units run from their source text instead of their compiled bytes (task A6 of
//! docs/handoff/extender-core-tasks.md; docs/spec-notes/script-compiler.md §7).
//!
//! The original game runs compiled bytes only. A mod compiled with the script extender
//! loaded has bytes that call functions at opcodes the extender gave out, which the
//! game's own function table does not have. NVForever has no table of those numbers, so it
//! compiles such a unit's source again with its own compiler (`fnv_script::compile`) and
//! runs that. The rule: a unit whose bytes call an opcode the function table does not
//! have, or whose bytes the decoder cannot read (the extender's own statements
//! have byte forms of their own [hypothesis]), and that has source, runs from its source
//! [decision: no shipped unit calls such an opcode or holds unreadable bytes (scripts.md
//! §2.6), so the shipped game is unchanged]. The compile
//! is seeded with the unit's own variable and reference lists, so other scripts' bytes
//! that name its variables by index stay right.
//!
//! `[Extenders] CompileSource` in NVForever.ini (the engine reads it, [`set_mode`]):
//! `auto` (the rule above, the default), `always` (every unit with source, for testing:
//! the shipped units must run the same), `never` (bytes only, as the original game).
//! `NVF_COMPILE_SOURCE` overrides it.

use crate::data::Compiled;
use fnv_esm::LoadOrder;
use fnv_script::compile::{compile, env::LoadOrderIndex, Seed};
use fnv_script::ir::{Arg, Expr, Stmt, Token};
use fnv_script::table::CONSOLE_OPCODE_BASE;
use fnv_script::{decode, FunctionTable, Program};
use fnv_sim::extract::Issues;
use rayon::prelude::*;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Always,
    Never,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Mode::Auto,
            "always" | "1" | "on" => Mode::Always,
            "never" | "0" | "off" => Mode::Never,
            _ => return None,
        })
    }
}

/// The INI section and key of the mode.
pub const SECTION: &str = "Extenders";
pub const KEY: &str = "CompileSource";
/// Environment variable that overrides the INI.
pub const ENV: &str = "NVF_COMPILE_SOURCE";

static MODE: AtomicU8 = AtomicU8::new(0);

/// Set the mode for the game data built after this (the engine, from the INI).
pub fn set_mode(m: Mode) {
    MODE.store(m as u8 + 1, Ordering::Relaxed);
}

/// The mode in effect: the environment variable, else what [`set_mode`] set, else `auto`.
pub fn mode() -> Mode {
    if let Some(m) = std::env::var(ENV).ok().as_deref().and_then(Mode::parse) {
        return m;
    }
    match MODE.load(Ordering::Relaxed) {
        2 => Mode::Always,
        3 => Mode::Never,
        _ => Mode::Auto,
    }
}

/// The highest opcode of the function table's script functions (console functions, which
/// the table numbers itself from `CONSOLE_OPCODE_BASE`, left out).
pub fn highest_opcode(table: &FunctionTable) -> u16 {
    table.functions.keys().copied().filter(|&o| o < CONSOLE_OPCODE_BASE).max().unwrap_or(0)
}

/// Whether the program calls an opcode the function table does not have. "Above the
/// highest" is not enough: the table now holds some of the extender's observed opcodes
/// (eval 0x153A, ar_Size 0x1550 …), and a mod's bytes call others below them [observed:
/// TaleOfTwoWastelands.esm calls 0x146D–0x14AE, run from the bytes as "not in the
/// function table" until this rule].
pub fn calls_unknown(p: &Program, table: &FunctionTable) -> bool {
    fn args(a: &[Arg]) -> bool {
        a.iter().any(|a| matches!(a, Arg::FormatVars(v) if args(v)))
    }
    fn expr(e: &Expr, h: &FunctionTable) -> bool {
        e.0.iter().any(|t| matches!(t, Token::Call { call } if h.get(call.opcode).is_none() || args(&call.args)))
    }
    fn stmts(s: &[Stmt], h: &FunctionTable) -> bool {
        s.iter().any(|s| match s {
            Stmt::Call { call } => h.get(call.opcode).is_none(),
            Stmt::Set { expr: e, .. } => expr(e, h),
            Stmt::If { arms, otherwise } => {
                arms.iter().any(|a| expr(&a.cond, h) || stmts(&a.body, h)) || otherwise.as_ref().is_some_and(|o| stmts(o, h))
            }
            Stmt::Unmatched { cond, .. } => cond.as_ref().is_some_and(|c| expr(c, h)),
            // An extended statement calls only what its tree names.
            Stmt::While { body, .. } | Stmt::ForEach { body, .. } => stmts(body, h),
            Stmt::Return | Stmt::Raw { .. } | Stmt::Let { .. } | Stmt::Eval { .. } | Stmt::Label { .. } | Stmt::GoTo { .. } | Stmt::Break | Stmt::Continue => false,
        })
    }
    stmts(&p.top, table) || p.blocks.iter().any(|b| stmts(&b.body, table))
}

/// Whether the source declares a `string_var` or `array_var`: the record's variable list
/// can only say Integer or FloatOrRef [documented: xEdit SLSD], so such a unit runs from
/// its source, where the compiler knows the types (script-compiler.md §7).
pub fn declares_extender_vars(src: &str) -> bool {
    src.lines().any(|l| {
        let w = l.trim_start().split(|c: char| c.is_whitespace()).next().unwrap_or("");
        w.eq_ignore_ascii_case("string_var") || w.eq_ignore_ascii_case("array_var")
    })
}

/// Compile from source the units the mode picks, in place. Units whose source does not
/// compile keep their bytes (noted in `issues`).
pub(crate) fn apply(lo: &LoadOrder<'_>, table: &FunctionTable, units: &mut [(fnv_sim::FormId, Compiled)], issues: &mut Issues) {
    let m = mode();
    if m == Mode::Never {
        return;
    }
    let picked: Vec<usize> = (0..units.len())
        .filter(|&i| {
            let c = &units[i].1;
            c.unit.has_scda
                && c.unit.source.is_some()
                && (m == Mode::Always || calls_unknown(&c.program, table) || c.raw > 0 || declares_extender_vars(c.unit.source.as_deref().unwrap_or("")))
        })
        .collect();
    tracing::info!("script build: source compile: {} of {} units picked, mode {m:?}", picked.len(), units.len());
    if picked.is_empty() {
        return;
    }
    let clock = std::time::Instant::now();
    // Indexing the load order reads every record: done only when a unit needs it.
    let all: Vec<fnv_script::ScriptUnit> = units.iter().map(|(_, c)| c.unit.clone()).collect();
    let ix = LoadOrderIndex::build(lo, &all);
    tracing::info!("script build: source compile: load order indexed: {:.0} ms", clock.elapsed().as_secs_f64() * 1e3);
    type Done = (Vec<u8>, Program, usize, Vec<fnv_script::unit::RefSlot>, Vec<fnv_script::VarDecl>);
    let results: Vec<(usize, Result<Done, String>)> = picked
        .par_iter()
        .map(|&i| {
            let u = &units[i].1.unit;
            let env = ix.env(ix.plugin_index(&u.plugin).unwrap_or(0));
            let c = compile(u.source.as_deref().unwrap_or(""), table, &env, Some(Seed { variables: &u.variables, refs: &u.refs, unknown_calls: true }));
            let first = c.errors().next().map(|e| format!("{}: line {}: {} {}", u.key(), e.line, e.kind, e.detail));
            let r = match first {
                Some(e) => Err(e),
                None => {
                    let d = decode(&c.bytes, table);
                    Ok((c.bytes, d.program, d.report.raw_total(), c.refs, c.variables))
                }
            };
            (i, r)
        })
        .collect();
    for (i, r) in results {
        match r {
            // The seed keeps every existing index; names the lists lacked come after them.
            Ok((bytes, program, raw, refs, variables)) => {
                let c = &mut units[i].1;
                issues.note("script_compiled_from_source", || c.unit.key());
                c.unit.bytes = bytes;
                c.unit.refs = refs;
                c.unit.variables = variables;
                c.program = program;
                c.raw = raw;
            }
            Err(e) => issues.note("script_source_does_not_compile", || e),
        }
    }
}
