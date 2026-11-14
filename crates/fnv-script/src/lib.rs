//! Compiled scripts (Layer 5, format side): reading script units out of records,
//! decoding their bytecode into the IR of docs/design/layer5.md §3.2, the function table,
//! and a canonical decompiler used for validation. The byte format is described in
//! docs/spec-notes/scripts.md §1–3; this crate knows the bytecode, not what any function
//! does (that is `fnv-game`).
//!
//! - [`unit`]: `ScriptUnit` from SCPT, QUST stages, INFO, PACK, TERM, PERK, REFR patrols
//! - [`ir`]: the decoded tree (`Program`, `Stmt`, `Expr`, `Call`, `Arg`, `VarRef`)
//! - [`decode`]: bytes → IR, with a report of everything not understood
//! - [`table`]: the function table from `schemas/script/functions.json`
//! - [`decompile`]: IR → canonical text
//! - [`compile`]: source text → compiled bytes, variables and references (the editor's job)
//! - [`check`]: consistency checks between a unit and its decoded program
//! - [`resolve`]: which script a form runs and its variables (for `ref.var`)
//! - [`vm`]: the interpreter over the IR, against a `Host` (docs/spec-notes/scripts.md §4)

pub mod check;
pub mod compile;
pub mod decode;
pub mod decompile;
pub mod ir;
pub mod keys;
pub mod resolve;
pub mod table;
pub mod unit;
pub mod vm;

pub use decode::{decode, Decoded, Issue, Report};
pub use ir::{Arg, Arm, Block, Call, Expr, Marker, Op, Program, RefIndex, Stmt, Token, VarLetter, VarRef, VarSlot};
pub use table::{FunctionTable, ParamEnc};
pub use unit::{ScriptKind, ScriptUnit, UnitOwner, VarDecl, VarType};
