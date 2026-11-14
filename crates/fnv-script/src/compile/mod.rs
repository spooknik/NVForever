//! The script compiler: source text (`SCTX`) → compiled bytes (`SCDA`), the local variable
//! list (`SLSD`/`SCVR`) and the reference list (`SCRO`/`SCRV`) (docs/design/extender-compat.md
//! §5.2, docs/handoff/extender-core-tasks.md A1–A5, docs/spec-notes/script-compiler.md).
//!
//! It writes the same bytes the editor writes, so the decoder ([`crate::decode`]) turns them
//! into the IR the interpreter runs, and every shipped unit with source is a test: compiled
//! from its `SCTX`, it must give back its own `SCDA` (`fnvtool script compile`).
//!
//! Written from the byte layout of docs/spec-notes/scripts.md §2 and the source rules of
//! §3.4, and from comparing our output with the shipped bytes. No editor or extender code is
//! read.
//!
//! - [`lex`]: lines and tokens;
//! - [`parse`]: statements of the vanilla grammar;
//! - `gen`: names, parameter encodings and the bytes;
//! - [`env`]: the names of a load order, as one plugin sees them;
//! - `nvsescript`: the extender's second language, written out in the first.

pub mod env;
mod gen;
pub mod lex;
mod nvsescript;
pub mod parse;

use crate::table::FunctionTable;
use crate::unit::{RefSlot, VarDecl};

/// A form named in source, as the unit's plugin sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormInfo {
    /// Load-order (global) FormID.
    pub form: u32,
    /// Record signature (`GLOB` forms are global variables).
    pub sig: [u8; 4],
}

/// What the compiler needs to know about the world around a unit.
pub trait Env {
    /// The form with this Editor ID (case-insensitive).
    fn form(&self, editor_id: &str) -> Option<FormInfo>;
    /// A form by load-order FormID.
    fn info(&self, form: u32) -> Option<FormInfo>;
    fn editor_id(&self, form: u32) -> Option<String>;
    /// A FormID written out in source: stored file-local to the unit's plugin.
    fn local_form(&self, local: u32) -> Option<u32>;
    /// The local variables of the script `form` runs (a quest's script, a reference's base
    /// form's script, or the form's own).
    fn script_vars(&self, form: u32) -> Option<Vec<VarDecl>>;
}

/// The variables and references a unit already has. Compiling against them keeps every
/// index the same, so bytecode elsewhere that names this script's variables by index stays
/// right (docs/spec-notes/scripts.md §2.5).
#[derive(Debug, Clone, Copy)]
pub struct Seed<'a> {
    pub variables: &'a [VarDecl],
    pub refs: &'a [RefSlot],
    /// A call line that starts with a name nothing knows compiles as a call of that name
    /// (it reports "not built" when it runs), so one undocumented plugin function does
    /// not cost a mod's whole script [decision: for the engine's compile of mods' source;
    /// the editor-parity compile keeps the error].
    pub unknown_calls: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    /// 1-based source line; 0 for the unit as a whole.
    pub line: usize,
    pub kind: &'static str,
    /// An error is something the editor would refuse to compile [hypothesis for each kind
    /// until checked against the editor, extender-core-tasks.md G2]; a warning is not.
    pub error: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    pub bytes: Vec<u8>,
    pub variables: Vec<VarDecl>,
    pub refs: Vec<RefSlot>,
    pub diags: Vec<Diag>,
}

impl Compiled {
    pub fn errors(&self) -> impl Iterator<Item = &Diag> {
        self.diags.iter().filter(|d| d.error)
    }
}

/// Compile one unit's source. With a `seed`, names resolve against the unit's own
/// reference list first and keep its indices; new ones are added after them.
pub fn compile(src: &str, table: &FunctionTable, env: &dyn Env, seed: Option<Seed<'_>>) -> Compiled {
    either(src, |s, nvse| {
        let mut g = gen::Gen::new(table, env, seed);
        g.declared_types_win = nvse;
        g.unit(s)
    })
}

/// A unit in NVSEScript is compiled from its text in the first grammar
/// (`nvsescript::translate`), its diagnostics naming the source's lines; any other unit
/// as it is.
fn either(src: &str, unit: impl FnOnce(&str, bool) -> Compiled) -> Compiled {
    if !nvsescript::is_nvsescript(src) {
        return unit(src, false);
    }
    match nvsescript::translate(src) {
        Ok(t) => {
            let mut c = unit(&t.text, true);
            for d in &mut c.diags {
                if d.line >= 1 {
                    d.line = t.lines.get(d.line - 1).copied().unwrap_or(d.line);
                }
            }
            c.diags.extend(t.diags);
            c
        }
        Err(d) => Compiled { bytes: Vec::new(), variables: Vec::new(), refs: Vec::new(), diags: vec![d] },
    }
}

/// Compile source as the extender's runtime compiler does: the console, the Script
/// Runner's files, CompileScript and nvf_capture's query steps. On top of [`compile`] it
/// refuses what xNVSE 6.4.9's runtime compiler refuses (docs/spec-notes/ext-syntax.md,
/// "Refused at run time"): a fractional number literal for an extender function's int
/// parameter, and the result of a command registered without a return type given to a
/// form parameter. Scripts the editor compiled keep their bytes, and the source a plugin
/// carries is compiled by [`compile`] with a seed, so neither meets these refusals
/// [decision: #2037, owner 2026-11-10 "Yes, refuse them"].
pub fn compile_runtime(src: &str, table: &FunctionTable, env: &dyn Env) -> Compiled {
    either(src, |s, _| {
        let mut g = gen::Gen::new(table, env, None);
        g.runtime = true;
        g.unit(s)
    })
}

/// A mod's text compiled at run time (JIP's Script Runner files, CompileScript's): as
/// [`compile_runtime`], and as a mod's source the engine compiles ([`Seed::unknown_calls`]):
/// a name nothing knows where a call stands is a call of that name, which reports "not
/// built" when it runs, instead of costing the whole file [decision, #2275: the owner's Blue
/// Moon list skipped whole Script Runner files over one plugin function the catalogue has
/// no page for (EnableImprovedRecipeMenu, GetExtraAccuracyPenaltyMult, …)].
pub fn compile_mod_runtime(src: &str, table: &FunctionTable, env: &dyn Env) -> Compiled {
    let mut g = gen::Gen::new(table, env, Some(Seed { variables: &[], refs: &[], unknown_calls: true }));
    g.runtime = true;
    g.unit(src)
}

/// The player reference, which source may call `player` or `playerref`
/// [observed: scripts.md §3.4].
pub const PLAYER: u32 = 0x14;
