//! Names, parameter encodings and bytes (docs/spec-notes/script-compiler.md §2–4). The
//! byte layout is docs/spec-notes/scripts.md §2; the source rules are §3.4 there.

use super::lex::{self, Kind, Tok};
use super::parse::{self, DeclType, Kw};
use super::{Compiled, Diag, Env, FormInfo, Seed, PLAYER};
use crate::decode::op;
use crate::ir::VarLetter;
use crate::table::{FunctionTable, ParamEnc, TrailerEnc};
use crate::unit::{RefSlot, VarDecl, VarType};
use std::collections::HashMap;

mod x;

/// Binary operators, higher binds tighter; `||` above `&&` [observed: scripts.md §3.3].
fn precedence(op: &str) -> u8 {
    match op {
        "&&" => 1,
        "||" => 2,
        "==" | "!=" => 3,
        "<" | ">" | "<=" | ">=" => 4,
        "+" | "-" => 5,
        "*" | "/" | "%" => 6,
        _ => 0,
    }
}

const PLAYER_NAMES: [&str; 2] = ["player", "playerref"];

/// The code compiled for an enumeration name the table lacks, in a mod's source compiled
/// by the engine ([`super::Seed::unknown_calls`]): one no animation group, actor value or
/// statistic has [design].
pub(super) const UNKNOWN_ENUM: u16 = u16::MAX;

/// Begin-block spellings in shipped source that differ from the catalogue's block name
/// [observed: scripts.md §3.4].
const BLOCK_ALIASES: [(&str, &str); 1] = [("onpackageend", "onpackagedone")];

/// A resolved operand or parameter.
#[derive(Debug, Clone, PartialEq)]
enum Val {
    /// A number as written (`19.50`, `-3`).
    Num(String),
    Local { index: u16, letter: VarLetter },
    /// A `GLOB`, by reference list index.
    Global(u16),
    /// Another script's variable: reference list index, variable index.
    Remote { on: u16, index: u16, letter: VarLetter },
    /// A reference list entry.
    Ref(u16),
}

/// A function as the compiler sees it.
struct FnInfo {
    opcode: u16,
    /// Arguments taken; `None` = all that follow (an open-ended list).
    params: Option<usize>,
    /// The call has a parameter block (even with count 0).
    block: bool,
}

/// Where a line opens a block lambda, `(begin function`, spaces allowed after the `(` and
/// between the words (`CallWhen ( begin function {}`) [hypothesis: spaces between tokens
/// do not matter, as elsewhere in the grammar; Bullet Belt Framework's ln_ file is written
/// so, #1470; documented: Lambda shows `(begin function`]: the `(`'s offset and the
/// length up to the end of `function`.
fn lambda_open(low: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(k) = low[from..].find('(') {
        let at = from + k;
        let rest = low[at + 1..].trim_start();
        if let Some(r) = rest.strip_prefix("begin") {
            let r2 = r.trim_start();
            if r2.len() < r.len() && r2.starts_with("function") {
                return Some((at, low.len() - r2.len() + "function".len() - at));
            }
        }
        from = at + 1;
    }
    None
}

/// Where a line opens a lambda in the arrow form that spans lines, `({int iBtn} =>` with
/// nothing after the arrow, its body on the next lines and a line starting with `)` closing
/// it [observed: FreePostMojave.esm ships `MessageBoxExAlt ({int iBtn} =>` and `CallWhen
/// ({} =>` written so with compiled bytes holding a lambda of those lines, #2275]: the
/// `(`'s offset and the text between the braces.
fn arrow_open(text: &str) -> Option<(usize, &str)> {
    let t = text.trim_end().strip_suffix("=>")?.trim_end().strip_suffix('}')?;
    let brace = t.rfind('{')?;
    let at = t[..brace].trim_end().strip_suffix('(').map(str::len)?;
    Some((at, &t[brace + 1..]))
}

/// A lambda [`Gen::take_lambdas`] has open: its first line, the text before its `(`, its
/// parameters, its lines, and whether it is the arrow form (closed by `)`, not `end)`).
type OpenLambda = (lex::Line, String, Vec<String>, Vec<lex::Line>, bool);

/// An open `If` chain: where its last marker's skip count goes, and that marker's
/// statement index.
struct Chain {
    skip_at: usize,
    stmt: usize,
}

pub(super) struct Gen<'a> {
    table: &'a FunctionTable,
    env: &'a dyn Env,
    vars: Vec<VarDecl>,
    refs: Vec<RefSlot>,
    /// The spelling each reference list entry was first used with.
    claims: Vec<Option<String>>,
    seeded: bool,
    /// [`Seed::unknown_calls`].
    unknown_calls: bool,
    /// The extender's runtime compiler's refusals apply ([`super::compile_runtime`]).
    pub(super) runtime: bool,
    /// Lower-case names of the locals the source declares `ref`.
    ref_vars: std::collections::HashSet<String>,
    /// Seed reference list forms by lower-case Editor ID.
    seed_edids: HashMap<String, u32>,
    blocks: HashMap<String, u16>,
    enums: HashMap<String, HashMap<String, u16>>,
    diags: Vec<Diag>,
    line: usize,
    out: Vec<u8>,
    /// Statements written so far, as skip counts count them.
    stmts: usize,
    chains: Vec<Chain>,
    /// Byte offset of the open Begin's length field, and of the end of that statement.
    block: Option<(usize, usize)>,
    /// Open `while`/`foreach` loops.
    loops: usize,
    /// The unit uses the extender's grammar somewhere: call lines may then hold inline
    /// expressions (script-compiler.md §8).
    extended: bool,
    /// The text of the statement being compiled.
    text: String,
    /// Inside a compiler-override block (`Begin _GameMode` … `End`): every line is read
    /// with the extender's grammar (task A12, script-compiler.md §8).
    co: bool,
    /// Parsing a function's arguments (not inside brackets of one): a signed number after
    /// a space starts the next argument ([`lex::XTok::sign`]).
    in_args: bool,
    /// Inside a function's argument at any depth, parentheses included: a bare word that
    /// names nothing is text there.
    in_arg_any: bool,
    /// Inside parentheses: an expression, where a string variable is its value.
    in_paren: bool,
    /// Compiling a line that is a call on its own (not `let`, `if eval` or any other
    /// expression).
    call_line: bool,
    /// Compiling a vanilla `set` line read with the extender's grammar.
    set_line: bool,
    /// A variable declared with another type than its seeded slot's takes the declared
    /// one, at the same index: for a unit written in NVSEScript, whose variable list the
    /// extender's second compiler writes with its own iterators and renamed locals
    /// (`__temp_<name>_<n>`) [observed: WeaponModCraftInfo.esp, #2275].
    pub(super) declared_types_win: bool,
    /// The block-form lambdas taken out of the source: parameter names and their lines
    /// (`__lambda<i>` stands in their place).
    lambdas: Vec<(Vec<String>, Vec<lex::Line>)>,
    /// The lambdas made so far in this unit (`XExpr::Lambda::index` of the next).
    pub(super) lambda_seq: u32,
}

fn put16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}

fn letter_of(ty: &VarType) -> VarLetter {
    if *ty == VarType::Int {
        VarLetter::Short
    } else {
        VarLetter::Float
    }
}

impl<'a> Gen<'a> {
    pub(super) fn new(table: &'a FunctionTable, env: &'a dyn Env, seed: Option<Seed<'_>>) -> Gen<'a> {
        let mut blocks: HashMap<String, u16> = table.blocks.iter().map(|(c, n)| (n.to_ascii_lowercase(), *c)).collect();
        for (alias, name) in BLOCK_ALIASES {
            if let Some(&c) = blocks.get(name) {
                blocks.insert(alias.to_string(), c);
            }
        }
        let mut enums: HashMap<String, HashMap<String, u16>> = table
            .enums
            .iter()
            .map(|(k, m)| (k.clone(), m.iter().map(|(c, n)| (n.to_ascii_lowercase(), *c)).collect()))
            .collect();
        // Actor values the shipped scripts never name still have their names, without
        // spaces (`IgnoreCrippledLimbs`) [documented: xEdit ActorValue, `AV_NAMES`;
        // observed: JustAssortedMods.esp names IgnoreCrippledLimbs].
        let av = enums.entry("actor_value".into()).or_default();
        for (c, n) in crate::table::AV_NAMES.iter().enumerate() {
            av.entry(n.replace(' ', "").to_ascii_lowercase()).or_insert(c as u16);
        }
        // And their script names (`CritChance`) [observed: gaps7, `GetActorValue
        // "CritChance"` compiles in the original].
        for (c, n) in crate::table::SCRIPT_AV_NAMES {
            av.entry(n.to_ascii_lowercase()).or_insert(c);
        }
        let mut seed_edids = HashMap::new();
        if let Some(s) = seed {
            for r in s.refs {
                if let RefSlot::Form(f) = *r {
                    if let Some(e) = env.editor_id(f) {
                        seed_edids.entry(e.to_ascii_lowercase()).or_insert(f);
                    }
                }
            }
        }
        Gen {
            table,
            env,
            vars: seed.map(|s| s.variables.to_vec()).unwrap_or_default(),
            refs: seed.map(|s| s.refs.to_vec()).unwrap_or_default(),
            claims: Vec::new(),
            seeded: seed.is_some(),
            unknown_calls: seed.is_some_and(|s| s.unknown_calls),
            runtime: false,
            ref_vars: Default::default(),
            seed_edids,
            blocks,
            enums,
            diags: Vec::new(),
            line: 0,
            out: Vec::new(),
            stmts: 0,
            chains: Vec::new(),
            block: None,
            loops: 0,
            extended: false,
            text: String::new(),
            co: false,
            in_args: false,
            in_arg_any: false,
            in_paren: false,
            call_line: false,
            set_line: false,
            declared_types_win: false,
            lambdas: Vec::new(),
            lambda_seq: 0,
        }
    }

    fn diag(&mut self, kind: &'static str, error: bool, detail: impl Into<String>) {
        self.diags.push(Diag { line: self.line, kind, error, detail: detail.into() });
    }

    pub(super) fn unit(mut self, src: &str) -> Compiled {
        let mut long = Vec::new();
        let lines = self.take_lambdas(lex::join_open_conditions(lex::lines(src, &mut long)));
        for n in long {
            self.diags.push(Diag { line: n, kind: "line_too_long", error: false, detail: format!("over {} characters", lex::MAX_LINE) });
        }
        let mut stmts = Vec::new();
        for l in &lines {
            match parse::statement(l) {
                Ok(s) => stmts.push(s),
                Err(e) => self.diags.push(Diag { line: e.line, kind: "syntax", error: true, detail: e.msg }),
            }
        }
        // Declarations first: a variable may be declared below its first use
        // (six shipped units declare inside a block, scripts.md §2.6).
        self.extended = stmts.iter().any(|s| {
            s.kw.extended()
                || !s.decls.is_empty()
                || matches!(&s.kw, Kw::Call(t) if self.function(&t[0]).is_none() && self.xfunction_name(&t[0].text))
        });
        self.declare_all(&stmts);
        for s in &stmts {
            self.line = s.line;
            self.text = s.text.clone();
            self.statement(&s.kw);
        }
        Compiled { bytes: self.out, variables: self.vars, refs: self.refs, diags: self.diags }
    }

    /// Take the block-form lambdas (`(begin function {…}` … `end)`, which span lines
    /// [documented: Lambda]) out of the lines; each leaves `__lambda<i>` in its place, which
    /// the expression parser compiles where it stands.
    fn take_lambdas(&mut self, lines: Vec<lex::Line>) -> Vec<lex::Line> {
        let mut out: Vec<lex::Line> = Vec::new();
        let mut open: Vec<OpenLambda> = Vec::new();
        for l in lines {
            let low = l.text.to_ascii_lowercase();
            if let Some((at, inside)) = arrow_open(&l.text) {
                let names = inside.split([',', ' ']).filter(|w| !w.is_empty()).map(str::to_string).collect();
                open.push((l.clone(), l.text[..at].to_string(), names, Vec::new(), true));
                continue;
            }
            if let Some((at, len)) = lambda_open(&low) {
                // The parameters are what the braces hold; an arrow after them, `(begin
                // function{} =>`, is no parameter [observed: LifeAfterDeath.esm ships a
                // CallAfter lambda written so with compiled bytes, #2275].
                let after = &l.text[at + len..];
                let after = after.find('}').map_or(after, |c| &after[..c]);
                let names = after.split(['{', '}', ',', ' ']).filter(|w| !w.is_empty()).map(str::to_string).collect();
                let names: Vec<String> = names;
                open.push((l.clone(), l.text[..at].to_string(), names, Vec::new(), false));
                continue;
            }
            let closes = {
                let t = low.trim_start();
                match open.last() {
                    Some(o) if o.4 => t.starts_with(')'),
                    _ => t.starts_with("end") && t[3..].trim_start().starts_with(')'),
                }
            };
            if closes && !open.is_empty() {
                let (first, before, names, body, _) = open.pop().unwrap_or_else(|| unreachable!());
                let i = self.lambdas.len();
                self.lambdas.push((names, body));
                let rest = &l.text[l.text.find(')').map_or(l.text.len(), |p| p + 1)..];
                let text = format!("{before}__lambda{i}{rest}");
                let merged = lex::Line { no: first.no, toks: lex::tokens(&text), text };
                match open.last_mut() {
                    Some(o) => o.3.push(merged),
                    None => out.push(merged),
                }
                continue;
            }
            match open.last_mut() {
                Some(o) => o.3.push(l),
                None => out.push(l),
            }
        }
        for (first, ..) in open {
            self.diags.push(Diag { line: first.no, kind: "lambda_without_end", error: true, detail: String::new() });
        }
        out
    }

    /// Compile lambda `i`'s statements with this unit's variables and references; the
    /// statements, as the decoder reads them back.
    pub(super) fn lambda(&mut self, i: usize) -> Option<crate::ir::XExpr> {
        let (names, lines) = self.lambdas.get(i).cloned()?;
        let names = self.typed_params(&names);
        let mut params = Vec::new();
        for n in &names {
            let Some(v) = self.var(n) else {
                self.diag("unresolved_name", true, n.clone());
                return None;
            };
            params.push(crate::ir::VarRef {
                on: None,
                var: crate::ir::VarSlot::Local { index: v.index as u16, letter: letter_of(&v.ty) },
            });
        }
        let saved = (std::mem::take(&mut self.out), self.stmts, std::mem::take(&mut self.chains), self.block.take(), self.loops, self.line, self.text.clone());
        self.stmts = 0;
        self.loops = 0;
        let mut stmts = Vec::new();
        for l in &lines {
            match parse::statement(l) {
                Ok(s) => stmts.push(s),
                Err(e) => self.diags.push(Diag { line: e.line, kind: "syntax", error: true, detail: e.msg }),
            }
        }
        // The body's declarations first, as the unit's (`unit`): its variables are the
        // maker's [documented: Lambda].
        self.declare_all(&stmts);
        for s in &stmts {
            self.line = s.line;
            self.text = s.text.clone();
            self.statement(&s.kw);
        }
        let bytes = std::mem::replace(&mut self.out, saved.0);
        self.stmts = saved.1;
        self.chains = saved.2;
        self.block = saved.3;
        self.loops = saved.4;
        self.line = saved.5;
        self.text = saved.6;
        let body = crate::decode::decode(&bytes, self.table).program.top;
        let index = self.next_lambda();
        Some(crate::ir::XExpr::Lambda { params, body, value: None, index })
    }

    /// The variables a run of statements declares: `int a`, `int a = 1` and the inline
    /// declarations of extender lines ([`parse::inline_decls`]).
    fn declare_all(&mut self, stmts: &[parse::Statement]) {
        for s in stmts {
            self.line = s.line;
            if let Kw::Decl { ty, names, .. } = &s.kw {
                for n in names {
                    self.declare(n, *ty);
                }
            }
            for (ty, n) in &s.decls {
                self.declare(n, *ty);
            }
        }
    }

    /// The number of the next lambda of this unit.
    pub(super) fn next_lambda(&mut self) -> u32 {
        self.lambda_seq += 1;
        self.lambda_seq - 1
    }

    /// Brace parameters, each name possibly after a type word (`{float fD, ref rS}`),
    /// which declares it [observed: the original's runtime-compiled function scripts take
    /// typed parameters there, jip-events.md §4]; the names.
    pub(super) fn typed_params(&mut self, words: &[String]) -> Vec<String> {
        let mut out = Vec::new();
        let mut ty = None;
        for w in words {
            if let Some(t) = DeclType::from_word(w) {
                ty = Some(t);
                continue;
            }
            if let Some(t) = ty.take() {
                if self.var(w).is_none() {
                    self.declare(w, t);
                }
            }
            out.push(w.clone());
        }
        out
    }

    // ------------------------------------------------------------------ variables, names

    fn declare(&mut self, name: &str, ty: DeclType) {
        // A variable may not take an extender function's name or alias [observed: a
        // runtime-compiled script declaring `ref rK` (ReleaseKey's alias) is refused, rF
        // rG rB rD rP are not; hypothesis that every extender name is refused].
        // In a mod's source compiled by the engine it is a warning: the unit shipped with
        // compiled bytes, so the compiler of its day took the name [observed: Project
        // Nevada - Core.esm 2.5 declares a variable `break` in a stage's result script],
        // and a local is read before a function of its name (`x::xword`).
        if self.table.unnumbered.contains_key(&name.to_ascii_lowercase()) {
            self.diag("variable_named_like_a_function", !self.unknown_calls, name.to_string());
        }
        // A string or array variable holds an id [documented: Array Variable; String
        // Variable]; NVForever keeps the kind in the type (the record's list cannot).
        let vt = match ty {
            DeclType::Int => VarType::Int,
            DeclType::String => VarType::Other("string_var".into()),
            DeclType::Array => VarType::Other("array_var".into()),
            DeclType::Float | DeclType::Ref => VarType::FloatOrRef,
        };
        if ty == DeclType::Ref {
            self.ref_vars.insert(name.to_ascii_lowercase());
        }
        // A record's variable list can only say Integer or FloatOrRef: the editor with xNVSE
        // stores a string_var as Integer and an array_var as FloatOrRef [observed:
        // docs/spec-notes/ext-bytecode.md]. A seeded slot declared string_var or array_var
        // takes that type, at the same index.
        if let Some(v) = self.vars.iter_mut().find(|v| v.name.eq_ignore_ascii_case(name)) {
            if matches!(v.ty, VarType::FloatOrRef | VarType::Int) && matches!(ty, DeclType::String | DeclType::Array) {
                v.ty = vt;
                return;
            }
        }
        if self.declared_types_win {
            if let Some(v) = self.vars.iter_mut().find(|v| v.name.eq_ignore_ascii_case(name)) {
                v.ty = vt;
                return;
            }
        }
        if let Some(v) = self.var(name) {
            if v.ty != vt {
                let d = format!("{name}: declared {ty:?}, the unit's variable list says {:?}", v.ty);
                self.diag("variable_type_differs", false, d);
            }
            return;
        }
        let index = self.vars.iter().map(|v| v.index).max().unwrap_or(0) + 1;
        if self.seeded {
            self.diag("variable_added", false, name.to_string());
        }
        self.vars.push(VarDecl { index, name: name.to_string(), ty: vt });
    }

    fn var(&self, name: &str) -> Option<&VarDecl> {
        self.vars.iter().find(|v| v.name.eq_ignore_ascii_case(name))
    }

    fn form(&self, name: &str) -> Option<FormInfo> {
        // A quoted reference (`"Name".Function`, lex::tokens): the name inside, or a FormID
        // in hex of any length when the load order has it [observed: Veronica Outfit
        // Replacer.esp, `"E32A9".GetItemCount` calls on 0x000E32A9, #2275; hypothesis:
        // the editor reads a quoted name as an Editor ID first].
        if let Some(inner) = name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) {
            if let Some(f) = self.form(inner) {
                return Some(f);
            }
            if !(1..=8).contains(&inner.len()) || !inner.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            let g = self.env.local_form(u32::from_str_radix(inner, 16).ok()?)?;
            return self.env.info(g);
        }
        let low = name.to_ascii_lowercase();
        if PLAYER_NAMES.contains(&low.as_str()) {
            return Some(FormInfo { form: PLAYER, sig: *b"REFR" });
        }
        if let Some(&f) = self.seed_edids.get(&low) {
            return Some(self.env.info(f).unwrap_or(FormInfo { form: f, sig: [0; 4] }));
        }
        if let Some(i) = self.env.form(name) {
            return Some(i);
        }
        if name.len() == 8 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
            let g = self.env.local_form(u32::from_str_radix(name, 16).ok()?)?;
            return Some(self.env.info(g).unwrap_or(FormInfo { form: g, sig: [0; 4] }));
        }
        None
    }

    /// A vanilla parameter's name that nothing has, in a mod's source compiled by the
    /// engine: a FormID written short, when the load order has that form [observed: ABM-
    /// Patched Perks.esp ships `player.AddItem f iAmount` with compiled bytes adding
    /// 0x0000000F, the caps, #2275; hypothesis: any length below 8, and only a vanilla
    /// parameter, where a word cannot be text].
    fn short_form_id(&self, name: &str) -> Option<FormInfo> {
        if !self.unknown_calls || !(1..8).contains(&name.len()) || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        self.env.info(self.env.local_form(u32::from_str_radix(name, 16).ok()?)?)
    }

    /// A FormID of 1 to 8 hex digits, when the load order has that form (the player's
    /// reference 0x14 included): an extender function's form parameter takes one
    /// written bare ([`x`]'s xargs).
    fn hex_form_id(&self, name: &str) -> Option<FormInfo> {
        if !(1..=8).contains(&name.len()) || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let id = self.env.local_form(u32::from_str_radix(name, 16).ok()?)?;
        if id == PLAYER {
            return Some(FormInfo { form: PLAYER, sig: *b"REFR" });
        }
        self.env.info(id)
    }

    /// The reference list entry for `slot` named as `spelling`. One form named two ways
    /// (`player`, `PlayerREF`) gets an entry per spelling, in order of first use
    /// [observed: script-compiler.md §3].
    fn ref_index(&mut self, slot: RefSlot, spelling: &str) -> u16 {
        let sp = spelling.to_ascii_lowercase();
        self.claims.resize(self.refs.len(), None);
        let mine = (0..self.refs.len()).find(|&i| self.refs[i] == slot && self.claims[i].as_deref() == Some(sp.as_str()));
        let free = || (0..self.refs.len()).find(|&i| self.refs[i] == slot && self.claims[i].is_none());
        if let Some(i) = mine.or_else(free) {
            self.claims[i] = Some(sp);
            return i as u16 + 1;
        }
        if self.seeded {
            self.diag("reference_added", false, format!("{slot:?} as {spelling}"));
        }
        self.refs.push(slot);
        self.claims.push(Some(sp));
        self.refs.len() as u16
    }

    /// The reference list entry for a name used as a reference (`Name.Func`, `Name.var`).
    fn reference(&mut self, name: &str) -> Option<u16> {
        if let Some(v) = self.var(name) {
            let idx = v.index;
            return Some(self.ref_index(RefSlot::Local(idx), name));
        }
        let f = self.form(name)?;
        Some(self.ref_index(RefSlot::Form(f.form), name))
    }

    /// `Head.var`: a variable of the script that `Head` runs.
    fn remote(&mut self, head: &str, tail: &str) -> Option<Val> {
        if self.var(head).is_some() {
            self.diag("remote_variable_on_local_ref", true, format!("{head}.{tail}"));
            return None;
        }
        let Some(f) = self.form(head) else {
            self.diag("unresolved_name", true, head.to_string());
            return None;
        };
        let Some(v) = self.env.script_vars(f.form).and_then(|vs| vs.into_iter().find(|v| v.name.eq_ignore_ascii_case(tail)))
        else {
            self.diag("unresolved_remote_variable", true, format!("{head}.{tail}"));
            return None;
        };
        let on = self.ref_index(RefSlot::Form(f.form), head);
        Some(Val::Remote { on, index: v.index as u16, letter: letter_of(&v.ty) })
    }

    /// An operand or parameter that is not a function call.
    fn atom(&mut self, t: &Tok) -> Option<Val> {
        match t.kind {
            Kind::Str => {
                // A quoted Editor ID names that form [observed: scripts.md §3.4].
                let f = self.form(&t.text)?;
                Some(self.form_val(f, &t.text))
            }
            Kind::Word if lex::is_number(&t.text) || is_neg_number(&t.text) => Some(Val::Num(t.text.clone())),
            Kind::Word => {
                if let Some((head, tail)) = t.text.split_once('.') {
                    if !head.is_empty() {
                        return self.remote(head, tail);
                    }
                }
                if let Some(v) = self.var(&t.text) {
                    return Some(Val::Local { index: v.index as u16, letter: letter_of(&v.ty) });
                }
                let f = self.form(&t.text)?;
                Some(self.form_val(f, &t.text))
            }
            _ => None,
        }
    }

    fn form_val(&mut self, f: FormInfo, spelling: &str) -> Val {
        let r = self.ref_index(RefSlot::Form(f.form), spelling);
        if &f.sig == b"GLOB" {
            Val::Global(r)
        } else {
            Val::Ref(r)
        }
    }

    /// The opcode a name compiles to. A console command's number gives way to an extender
    /// function of the same name with a page of its own: the catalogue's console row for
    /// `SetStringSetting` comes from another page (ShowFullSceneGraph's), and took the
    /// name from LU's two-parameter function, so every mod's `SetStringSetting "s" x`
    /// compiled as a console command with no arguments [observed: RG_PrimmPlayerHome.esm
    /// and P6UltimateHome.esp ship it with both arguments in their compiled bytes, #2275].
    pub(super) fn numbered(&self, name: &str) -> Option<u16> {
        let op = self.table.by_name(name)?;
        let extender = op >= crate::table::CONSOLE_OPCODE_BASE && self.table.unnumbered.get(&name.to_ascii_lowercase()).is_some_and(|u| u.own_page);
        (!extender).then_some(op)
    }

    fn function(&mut self, tok: &Tok) -> Option<(Option<String>, FnInfo)> {
        if tok.kind != Kind::Word {
            return None;
        }
        let (head, name) = match tok.text.split_once('.') {
            Some((h, n)) => ((!h.is_empty()).then(|| h.to_string()), n),
            None => (None, tok.text.as_str()),
        };
        if head.is_none() && self.var(name).is_some() {
            return None;
        }
        let opcode = self.numbered(name)?;
        let f = self.table.get(opcode)?;
        let params = match f.param_count {
            Some(n) => Some(n),
            None if f.params.iter().any(|p| p.name.starts_with("...")) => None,
            None => Some(f.params.len().max(f.encodings.len())),
        };
        let block = f.block.unwrap_or(params != Some(0) || !f.trailer.is_empty());
        Some((head, FnInfo { opcode, params, block }))
    }

    // ------------------------------------------------------------------ parameters

    fn tagged(&mut self, b: &mut Vec<u8>, v: &Val) {
        match v {
            Val::Num(text) => {
                if text.contains('.') {
                    b.push(b'z');
                    b.extend_from_slice(&text.parse::<f64>().unwrap_or(0.0).to_le_bytes());
                } else {
                    b.push(b'n');
                    let n = text.parse::<i64>().unwrap_or(0).clamp(i32::MIN as i64, i32::MAX as i64) as i32;
                    b.extend_from_slice(&n.to_le_bytes());
                }
            }
            Val::Local { index, letter } => {
                b.push(letter.byte());
                put16(b, *index);
            }
            Val::Global(r) => {
                b.push(b'G');
                put16(b, *r);
            }
            Val::Remote { on, index, letter } => {
                b.push(b'r');
                put16(b, *on);
                b.push(letter.byte());
                put16(b, *index);
            }
            Val::Ref(r) => {
                b.push(b'r');
                put16(b, *r);
            }
        }
    }

    fn param(&mut self, b: &mut Vec<u8>, enc: &ParamEnc, kind: &str, t: &Tok) {
        match enc {
            // A `ref` local passed as a parameter is its reference list entry (`SCRV`),
            // not the variable [observed: script-compiler.md §3].
            ParamEnc::Value if t.kind == Kind::Word && self.ref_vars.contains(&t.text.to_ascii_lowercase()) => {
                let idx = self.var(&t.text).map(|v| v.index).unwrap_or(0);
                let r = self.ref_index(RefSlot::Local(idx), &t.text);
                self.tagged(b, &Val::Ref(r));
            }
            // A number in a float parameter is stored as a float, whatever it looks like
            // [observed: script-compiler.md §3].
            ParamEnc::Value if kind == "float" && t.kind == Kind::Word && (lex::is_number(&t.text) || is_neg_number(&t.text)) => {
                b.push(b'z');
                b.extend_from_slice(&t.text.parse::<f64>().unwrap_or(0.0).to_le_bytes());
            }
            ParamEnc::Value => match self.atom(t) {
                // A global in a form parameter is the form, not its value
                // (`GetGlobalValue`) [observed: script-compiler.md §3].
                Some(Val::Global(r)) if !matches!(kind, "int" | "float" | "variable") => self.tagged(b, &Val::Ref(r)),
                Some(v) => self.tagged(b, &v),
                None => match self.short_form_id(&t.text) {
                    Some(f) => {
                        let v = self.form_val(f, &t.text);
                        self.tagged(b, &v);
                    }
                    None => {
                        self.diag("unresolved_name", true, t.text.clone());
                        self.tagged(b, &Val::Num("0".into()));
                    }
                },
            },
            ParamEnc::Enum(kind) => {
                let code = t.text.parse::<u16>().ok().or_else(|| {
                    self.enums.get(kind).and_then(|m| m.get(&t.text.to_ascii_lowercase())).copied()
                });
                // In a mod's source compiled by the engine a name the table lacks is the
                // code no group, value or stat has, with a warning, so the unit still
                // runs: the call then finds nothing of that name [decision; the table
                // holds only the names seen in compiled scripts].
                if code.is_none() {
                    self.diag("unknown_enum_name", !self.unknown_calls, format!("{kind}: {}", t.text));
                }
                put16(b, code.unwrap_or(if self.unknown_calls { UNKNOWN_ENUM } else { 0 }));
            }
            ParamEnc::Axis => {
                let c = t.text.chars().next().unwrap_or('X').to_ascii_uppercase();
                if !matches!(c, 'X' | 'Y' | 'Z') || t.text.len() != 1 {
                    self.diag("bad_axis", true, t.text.clone());
                }
                b.push(c as u8);
            }
            ParamEnc::String => {
                let s = fnv_util::text::encode(&t.text).unwrap_or_else(|| t.text.bytes().collect());
                put16(b, s.len() as u16);
                b.extend_from_slice(&s);
            }
        }
    }

    /// The catalogue's kind of parameter `k` (`int`, `float`, `reference`, …), with the
    /// observed number types over it.
    fn param_kind(&self, opcode: u16, k: usize) -> String {
        let f = self.table.get(opcode);
        match f.and_then(|f| f.number_types.get(&k)) {
            Some(true) => "float".into(),
            Some(false) => "int".into(),
            None => f.and_then(|f| f.params.get(k)).map(|p| p.kind.clone()).unwrap_or_default(),
        }
    }

    /// The parameter block of a call: empty, or count + parameters (+ trailer).
    fn params(&mut self, f: &FnInfo, args: &[Tok]) -> Vec<u8> {
        if !f.block {
            return Vec::new();
        }
        let trailer = self.table.trailer(f.opcode).to_vec();
        let mut b = Vec::new();
        if trailer.is_empty() {
            let n = f.params.map_or(args.len(), |p| p.min(args.len()));
            put16(&mut b, n as u16);
            for (k, t) in args[..n].iter().enumerate() {
                let enc = self.table.param_enc(f.opcode, k);
                let kind = self.param_kind(f.opcode, k);
                self.param(&mut b, &enc, &kind, t);
            }
            return b;
        }
        // A formatted message: its own parameters, then the trailer (scripts.md §2.4).
        let own = self.table.get(f.opcode).map_or(0, |x| x.encodings.len()).min(args.len());
        put16(&mut b, own as u16);
        for (k, t) in args[..own].iter().enumerate() {
            let enc = self.table.param_enc(f.opcode, k);
            let kind = self.param_kind(f.opcode, k);
            self.param(&mut b, &enc, &kind, t);
        }
        let rest = &args[own..];
        let numbers: Vec<&Tok> = rest.iter().filter(|t| lex::is_number(&t.text) || is_neg_number(&t.text)).collect();
        let vars: Vec<&Tok> = rest.iter().filter(|t| !(lex::is_number(&t.text) || is_neg_number(&t.text))).collect();
        for tr in trailer {
            match tr {
                TrailerEnc::FormatVars => {
                    put16(&mut b, vars.len() as u16);
                    for t in &vars {
                        self.param(&mut b, &ParamEnc::Value, "variable", t);
                    }
                }
                TrailerEnc::Int32 => {
                    let v = numbers.last().and_then(|t| t.text.parse::<i32>().ok()).unwrap_or(0);
                    b.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        b
    }

    /// Statement and Begin arguments: a `-` before a number makes it negative; parentheses
    /// and stray punctuation are dropped [observed: scripts.md §3.4].
    fn arg_tokens(toks: &[Tok]) -> Vec<Tok> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            if t.is_op("-") && toks.get(i + 1).is_some_and(|n| n.kind == Kind::Word && lex::is_number(&n.text)) {
                out.push(Tok { kind: Kind::Word, text: format!("-{}", toks[i + 1].text) });
                i += 2;
                continue;
            }
            if matches!(t.kind, Kind::Word | Kind::Str) {
                out.push(t.clone());
            }
            i += 1;
        }
        out
    }

    // ------------------------------------------------------------------ expressions

    fn expr(&mut self, toks: &[Tok]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut ops: Vec<&'static str> = Vec::new();
        let mut prev_operand = false;
        let emit_op = |out: &mut Vec<u8>, op: &str| {
            out.push(b' ');
            out.extend_from_slice(op.as_bytes());
        };
        let mut i = 0;
        while i < toks.len() {
            let t = &toks[i];
            match t.kind {
                Kind::Open => {
                    ops.push("(");
                    prev_operand = false;
                }
                Kind::Close => {
                    while let Some(o) = ops.pop() {
                        if o == "(" {
                            break;
                        }
                        emit_op(&mut out, o);
                    }
                    prev_operand = true;
                }
                Kind::Op if t.text == "-" && !prev_operand => {
                    ops.push("~");
                    prev_operand = false;
                }
                Kind::Op => {
                    let op = static_op(&t.text);
                    let p = precedence(op);
                    while let Some(&top) = ops.last() {
                        if top != "(" && (top == "~" || precedence(top) >= p) {
                            emit_op(&mut out, top);
                            ops.pop();
                        } else {
                            break;
                        }
                    }
                    ops.push(op);
                    prev_operand = false;
                }
                Kind::Punct => self.diag("stray_punctuation", false, t.text.clone()),
                Kind::Word | Kind::Str => {
                    if let Some((head, f)) = self.function(t) {
                        let mut args = Vec::new();
                        while f.params.is_none_or(|n| args.len() < n)
                            && toks.get(i + 1).is_some_and(|n| !matches!(n.kind, Kind::Op | Kind::Open | Kind::Close))
                        {
                            i += 1;
                            args.push(toks[i].clone());
                        }
                        let args = Self::arg_tokens(&args);
                        out.push(b' ');
                        if let Some(h) = head {
                            match self.reference(&h) {
                                Some(r) => {
                                    out.push(b'r');
                                    put16(&mut out, r);
                                }
                                None => self.diag("unresolved_name", true, h),
                            }
                        }
                        let body = self.params(&f, &args);
                        out.push(b'X');
                        put16(&mut out, f.opcode);
                        put16(&mut out, body.len() as u16);
                        out.extend_from_slice(&body);
                    } else {
                        match self.atom(t) {
                            Some(v) => self.token(&mut out, &v),
                            None => {
                                self.diag("unresolved_name", true, t.text.clone());
                                out.extend_from_slice(b" 0");
                            }
                        }
                    }
                    prev_operand = true;
                }
            }
            i += 1;
        }
        while let Some(o) = ops.pop() {
            if o != "(" {
                emit_op(&mut out, o);
            }
        }
        out
    }

    fn token(&mut self, out: &mut Vec<u8>, v: &Val) {
        out.push(b' ');
        match v {
            Val::Num(text) => out.extend_from_slice(text.as_bytes()),
            Val::Ref(r) => {
                out.push(b'Z');
                put16(out, *r);
            }
            other => self.tagged(out, other),
        }
    }

    // ------------------------------------------------------------------ statements

    /// Write one statement; returns the byte offset of its body.
    fn emit(&mut self, on: Option<u16>, opcode: u16, body: &[u8]) -> usize {
        if let Some(r) = on {
            put16(&mut self.out, op::REF_PREFIX);
            put16(&mut self.out, r);
        }
        put16(&mut self.out, opcode);
        put16(&mut self.out, body.len() as u16);
        let at = self.out.len();
        self.out.extend_from_slice(body);
        self.stmts += 1;
        at
    }

    fn patch_skip(&mut self, c: &Chain, here: usize) {
        let skip = (here - c.stmt - 1) as u16;
        self.out[c.skip_at..c.skip_at + 2].copy_from_slice(&skip.to_le_bytes());
    }

    /// `If`/`ElseIf`/`Else` marker: skip count (patched later), then the condition.
    fn marker(&mut self, opcode: u16, cond: Option<&[Tok]>) {
        let e = cond.map(|c| self.expr(c));
        self.marker_bytes(opcode, e);
    }

    /// An extended expression as NVForever's own token (script-compiler.md §8).
    fn xbytes(&mut self, e: &crate::ir::XExpr) -> Vec<u8> {
        let json = serde_json::to_vec(e).unwrap_or_default();
        let mut b = vec![b' ', op::X_TOKEN];
        put16(&mut b, json.len() as u16);
        b.extend_from_slice(&json);
        b
    }

    /// Parse a whole extended expression, reporting what is left over.
    fn xwhole(&mut self, toks: &[lex::XTok]) -> Option<crate::ir::XExpr> {
        let mut p = x::P::new(toks);
        let e = self.xexpr(&mut p, 0)?;
        if !p.done() {
            let rest = p.rest();
            self.diag("syntax", true, format!("unexpected {rest}"));
            return None;
        }
        Some(e)
    }

    /// A call line as one extended expression. Arguments past those the function takes
    /// are dropped with a warning, as the vanilla compiler drops them (§3 rule 1 of
    /// script-compiler.md) [hypothesis: the extender's call lines do the same; the owner's
    /// mods pass `SetJohnnyOnRenderUpdateEventHandler 1 (begin function {} … end) 0 0`,
    /// four arguments to three parameters, ln_B42Optics.txt, #1470].
    fn xcall_line(&mut self, toks: &[lex::XTok]) -> Option<crate::ir::XExpr> {
        let mut p = x::P::new(toks);
        let e = self.xexpr(&mut p, 0)?;
        if p.done() {
            return Some(e);
        }
        let rest = p.rest();
        if matches!(e, crate::ir::XExpr::Call { .. }) {
            // The dropped arguments leave nothing behind: what reading them registered
            // (reference list entries and their spellings, a lambda's number and the
            // variables its body declares) and what it reported is put back. One that
            // cannot be read at all (`Mistyped.Fn`) still fails the line: an argument is
            // dropped only when it is one [hypothesis, observer-checks.md "Surplus
            // arguments on an extender call line"].
            let saved = (self.refs.clone(), self.claims.clone(), self.vars.clone(), self.ref_vars.clone(), self.lambda_seq, self.diags.len());
            let n = self.surplus_args(&mut p);
            (self.refs, self.claims, self.vars, self.ref_vars, self.lambda_seq) = (saved.0, saved.1, saved.2, saved.3, saved.4);
            self.diags.truncate(saved.5);
            if let Some(n) = n {
                self.diag("surplus_arguments", false, format!("{n} dropped: {rest}"));
                return Some(e);
            }
        }
        self.diag("syntax", true, format!("unexpected {rest}"));
        None
    }

    fn marker_bytes(&mut self, opcode: u16, cond: Option<Vec<u8>>) {
        let here = self.stmts;
        let mut body = vec![0, 0];
        if let Some(e) = cond {
            put16(&mut body, e.len() as u16);
            body.extend_from_slice(&e);
        }
        let at = self.emit(None, opcode, &body);
        let new = Chain { skip_at: at, stmt: here };
        if opcode == op::IF {
            self.chains.push(new);
        } else if let Some(c) = self.chains.pop() {
            self.patch_skip(&c, here);
            self.chains.push(new);
        } else {
            // With nothing open the marker starts a chain of its own: its skip count runs to
            // the next marker [observed: script-compiler.md §3].
            self.diag("unmatched_marker", false, if opcode == op::ELSE { "Else" } else { "ElseIf" });
            self.chains.push(new);
        }
    }

    fn statement(&mut self, kw: &Kw) {
        // A line read with the extender's grammar must close its strings [observed: the
        // GECK's "Mismatched quotes", ext-bytecode.md §7]; a vanilla line's unclosed string
        // runs to the end of the line.
        let x_line = self.co
            || matches!(kw, Kw::Let(_) | Kw::IfEval(_) | Kw::ElseIfEval(_) | Kw::Eval(_) | Kw::While(_) | Kw::ForEach(_))
            || matches!(kw, Kw::Call(toks) if self.x_call_line(toks));
        if x_line && lex::unclosed_quote(&self.text) {
            self.diag("mismatched_quotes", true, self.text.clone());
            return;
        }
        // In a compiler-override block the vanilla statements take the extender's grammar,
        // and so does a vanilla `if` or `set` that calls an extender function
        // (`if rItem.GetBaseObject == rGun`) [observed: the peer's nvse-inv probes compile in
        // the original with xNVSE].
        if self.co || (matches!(kw, Kw::If(_) | Kw::ElseIf(_) | Kw::Set { .. }) && (self.calls_extender() || self.calls_unknown())) {
            let x = lex::xtokens(&self.text);
            let mapped = match kw {
                Kw::If(_) => Some(Kw::IfEval(x[1..].to_vec())),
                Kw::ElseIf(_) => Some(Kw::ElseIfEval(x[1..].to_vec())),
                Kw::Set { .. } => x.iter().position(|t| t.is(lex::XKind::Word, "to")).map(|k| {
                    let mut t = x[1..k].to_vec();
                    t.push(lex::XTok { kind: lex::XKind::Op, text: ":=".into(), sign: false });
                    t.extend_from_slice(&x[k + 1..]);
                    Kw::Let(t)
                }),
                _ => None,
            };
            if let Some(m) = mapped {
                self.set_line = matches!(kw, Kw::Set { .. });
                self.statement_inner(&m);
                self.set_line = false;
                return;
            }
        }
        self.statement_inner(kw)
    }

    /// Whether the line names an extender function (one the vanilla table does not know),
    /// alone or after a reference (`ref.Function`).
    fn calls_extender(&self) -> bool {
        lex::xtokens(&self.text).iter().filter(|t| t.kind == lex::XKind::Word).any(|t| {
            let name = t.text.rsplit('.').next().unwrap_or(&t.text);
            self.numbered(name).is_none() && self.table.unnumbered.contains_key(&name.to_ascii_lowercase()) && self.var(name).is_none()
        })
    }

    /// Whether a call line is one extended expression: in a unit of the extender's grammar
    /// or in a mod's source compiled by the engine ([`super::Seed::unknown_calls`]), one
    /// holding parentheses or calling a function the vanilla table lacks, so a unit whose
    /// only extender use is one undocumented function still compiles. The engine compiles
    /// only units whose bytes call the extender, which the editor wrote with the extender
    /// loaded, where a vanilla function's argument in parentheses is an inline expression
    /// [observed: S6S Base Game Perks Redux.esp's perk entries, `Player.RestoreAv Health
    /// (player.getav survival)`, and Trap Tweaks.esp's `rewardXP (GetIniFltOrCreateC …)`
    /// ship compiled bytes with an inline expression there, in units with no other
    /// extender statement (#2275)].
    fn x_call_line(&mut self, toks: &[Tok]) -> bool {
        let vanilla = self.function(&toks[0]).is_some();
        (self.extended || self.unknown_calls) && (!vanilla || toks.iter().any(|t| t.kind == lex::Kind::Open))
    }

    /// In a mod's source compiled by the engine ([`super::Seed::unknown_calls`]): whether
    /// the line calls a function nothing knows, `Name args` or `Ref.Name`. Such a vanilla
    /// `if` or `set` is read with the extender's grammar, where the name becomes a call
    /// that reports "not built" when it runs (`x::Gen::xword`), instead of failing the
    /// whole unit as an unresolved name [decision: one undocumented plugin function must
    /// not cost a mod's script; observed: JustAssortedMods.esp 4.6 calls four functions the
    /// catalogue has no page for].
    fn calls_unknown(&self) -> bool {
        if !self.unknown_calls {
            return false;
        }
        let x = lex::xtokens(&self.text);
        (0..x.len()).any(|i| {
            let t = &x[i];
            // A word in a call's place: the line's first operand, or after an operator or
            // an opening parenthesis; `set x to …` and `if …` lead with their keyword.
            let operand_place = i > 0 && (matches!(x[i - 1].kind, lex::XKind::Op | lex::XKind::Open) || (x[i - 1].kind == lex::XKind::Word && matches!(x[i - 1].text.to_ascii_lowercase().as_str(), "if" | "elseif" | "to")));
            t.kind == lex::XKind::Word && operand_place && self.names_nothing(&t.text)
        })
    }

    /// Whether `word` (`Name`, `Ref.Name` or `Quest.refVar.Name`) names no function,
    /// variable, form, enumeration name or number, while what stands before its last dot
    /// does resolve: the shape of a call of a function the catalogue lacks.
    pub(super) fn names_nothing(&self, word: &str) -> bool {
        let (head, name) = match word.rsplit_once('.') {
            Some((h, n)) => (Some(h), n),
            None => (None, word),
        };
        let low = name.to_ascii_lowercase();
        let ident = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !ident || self.table.by_name(name).is_some() || self.table.unnumbered.contains_key(&low) || self.enums.values().any(|m| m.contains_key(&low)) {
            return false;
        }
        match head {
            None => self.var(name).is_none() && self.form(name).is_none(),
            Some(h) => match h.split_once('.') {
                // `Quest.refVar.Name`: the quest's script has the variable.
                Some((q, v)) => self.var(q).is_none() && self.form(q).is_some_and(|f| self.env.script_vars(f.form).is_some_and(|vs| vs.iter().any(|x| x.name.eq_ignore_ascii_case(v)))),
                // `refVar.Name`, or `Form.Name` where the form's script has no such variable.
                None if self.var(h).is_some() => true,
                None => self.form(h).is_some_and(|f| !self.env.script_vars(f.form).is_some_and(|vs| vs.iter().any(|x| x.name.eq_ignore_ascii_case(name)))),
            },
        }
    }

    fn statement_inner(&mut self, kw: &Kw) {
        match kw {
            Kw::ScriptName(_) => {
                self.emit(None, op::SCRIPT_NAME, &[]);
            }
            // No statement, but the editor counts a `ref` declaration in skip counts
            // [observed: scripts.md §2.6, six units, all `ref`; one `short` inside an If
            // is not counted; hypothesis: the type decides].
            Kw::Decl { ty: DeclType::Ref, init: None, .. } => self.stmts += 1,
            // `int i = 5`: the declaration, then the assignment.
            Kw::Decl { init: Some(toks), .. } => self.statement(&Kw::Let(toks.clone())),
            Kw::Decl { .. } => {}
            Kw::Begin { block, args } => {
                let co = block.starts_with('_');
                let block = &block.trim_start_matches('_').to_string();
                self.co = co;
                // `Begin Function {a, b}`: a user-defined function; its parameters are
                // variables of the script [documented: User Defined Function].
                if block.eq_ignore_ascii_case("function") {
                    let words: Vec<String> = Self::arg_tokens(args).into_iter().map(|t| t.text).collect();
                    let params: Vec<Tok> = self.typed_params(&words).into_iter().map(|text| Tok { kind: Kind::Word, text }).collect();
                    let mut body = Vec::new();
                    put16(&mut body, crate::ir::FUNCTION_BLOCK);
                    body.extend_from_slice(&0u32.to_le_bytes());
                    put16(&mut body, params.len() as u16);
                    for t in &params {
                        match self.var(&t.text) {
                            Some(v) => {
                                let val = Val::Local { index: v.index as u16, letter: letter_of(&v.ty) };
                                self.tagged(&mut body, &val);
                            }
                            None => {
                                self.diag("unresolved_name", true, t.text.clone());
                                self.tagged(&mut body, &Val::Num("0".into()));
                            }
                        }
                    }
                    let at = self.emit(None, op::BEGIN, &body);
                    self.block = Some((at + 2, self.out.len()));
                    return;
                }
                let Some(&code) = self.blocks.get(&block.to_ascii_lowercase()) else {
                    self.diag("unknown_block", true, block.clone());
                    return;
                };
                let args = Self::arg_tokens(args);
                let mut body = Vec::new();
                put16(&mut body, code);
                body.extend_from_slice(&0u32.to_le_bytes());
                if self.table.begin_params.contains(&code) {
                    put16(&mut body, args.len() as u16);
                    for t in &args {
                        self.param(&mut body, &ParamEnc::Value, "", t);
                    }
                }
                let at = self.emit(None, op::BEGIN, &body);
                self.block = Some((at + 2, self.out.len()));
            }
            Kw::End => {
                if self.loops > 0 {
                    self.diag("while_without_loop", true, "");
                    self.loops = 0;
                }
                self.co = false;
                self.emit(None, op::END, &[]);
                match self.block.take() {
                    Some((len_at, after)) => {
                        let len = (self.out.len() - after) as u32;
                        self.out[len_at..len_at + 4].copy_from_slice(&len.to_le_bytes());
                    }
                    None => self.diag("end_without_begin", true, ""),
                }
            }
            Kw::If(c) => self.marker(op::IF, Some(c)),
            Kw::IfEval(c) | Kw::ElseIfEval(c) => {
                let opcode = if matches!(kw, Kw::IfEval(_)) { op::IF } else { op::ELSEIF };
                // A condition that does not compile still opens its chain, so the
                // markers after it pair up.
                let e = self.xwhole(c).map(|e| self.xbytes(&e)).unwrap_or_else(|| b" 0".to_vec());
                self.marker_bytes(opcode, Some(e));
            }
            Kw::While(toks) => {
                let e = self.xwhole(toks).unwrap_or(crate::ir::XExpr::Num { value: 0.0, text: "0".into() });
                let json = serde_json::to_vec(&e).unwrap_or_default();
                self.emit(None, op::X_WHILE, &json);
                self.loops += 1;
            }
            Kw::ForEach(toks) => {
                // `foreach (item <- source)`: the whole head in one pair of parentheses
                // [observed: Unofficial Patch NVSE Plus and JSawyer Ultimate ship such
                // lines with compiled bytes, `foreach (aEntry <- ar_List 0, 1, …)`].
                let toks = lex::unwrapped(toks);
                let arrow = toks.iter().position(|t| t.is(lex::XKind::Op, "<-"));
                let head = arrow.and_then(|k| {
                    let var = self.xwhole(&toks[..k])?;
                    let source = self.xwhole(&toks[k + 1..])?;
                    Some(crate::ir::XForEach { var, source })
                });
                match &head {
                    None if arrow.is_none() => self.diag("syntax", true, "foreach without <-"),
                    Some(h) if !matches!(h.var, crate::ir::XExpr::Var { .. }) => self.diag("bad_foreach_variable", true, ""),
                    _ => {}
                }
                let head = head.unwrap_or(crate::ir::XForEach {
                    var: crate::ir::XExpr::Num { value: 0.0, text: "0".into() },
                    source: crate::ir::XExpr::Num { value: 0.0, text: "0".into() },
                });
                let json = serde_json::to_vec(&head).unwrap_or_default();
                self.emit(None, op::X_FOREACH, &json);
                self.loops += 1;
            }
            Kw::Loop => {
                if self.loops == 0 {
                    self.diag("loop_without_while", true, "");
                    return;
                }
                self.loops -= 1;
                self.emit(None, op::X_LOOP, &[]);
            }
            Kw::Label(n) => {
                self.emit(None, op::X_LABEL, &n.to_le_bytes());
            }
            Kw::GoTo(n) => {
                self.emit(None, op::X_GOTO, &n.to_le_bytes());
            }
            Kw::Break | Kw::Continue => {
                if self.loops == 0 {
                    self.diag("outside_a_loop", true, if matches!(kw, Kw::Break) { "break" } else { "continue" });
                    return;
                }
                self.emit(None, if matches!(kw, Kw::Break) { op::X_BREAK } else { op::X_CONTINUE }, &[]);
            }
            // `eval <expression>`: run for what it does; an assignment is a `let`.
            Kw::Eval(toks) => {
                let Some(e) = self.xwhole(toks) else { return };
                let json = serde_json::to_vec(&e).unwrap_or_default();
                self.emit(None, if matches!(e, crate::ir::XExpr::Assign { .. }) { op::X_LET } else { op::X_EVAL }, &json);
            }
            Kw::Let(toks) => {
                let Some(e) = self.xwhole(toks) else { return };
                if !matches!(e, crate::ir::XExpr::Assign { .. }) {
                    self.diag("let_without_assignment", true, "");
                    return;
                }
                let json = serde_json::to_vec(&e).unwrap_or_default();
                self.emit(None, op::X_LET, &json);
            }
            Kw::ElseIf(c) => self.marker(op::ELSEIF, Some(c)),
            Kw::Else => self.marker(op::ELSE, None),
            Kw::EndIf => {
                let here = self.stmts;
                self.emit(None, op::ENDIF, &[]);
                match self.chains.pop() {
                    Some(c) => self.patch_skip(&c, here),
                    None => self.diag("unmatched_marker", false, "EndIf"),
                }
            }
            Kw::Return => {
                // `Return value` compiles in a function and is a plain Return: Call gives
                // what SetFunctionValue set, not the value [observed: probe
                // udf.return_value, ext-bytecode.md].
                self.emit(None, op::RETURN, &[]);
            }
            Kw::Set { target, expr, on } => {
                let on = match on {
                    Some(h) => match self.reference(h) {
                        Some(r) => Some(r),
                        None => {
                            self.diag("unresolved_name", true, h.clone());
                            return;
                        }
                    },
                    None => None,
                };
                let mut body = Vec::new();
                let tv = if target.len() == 1 { self.atom(&target[0]) } else { None };
                match tv {
                    Some(v @ (Val::Local { .. } | Val::Global(_) | Val::Remote { .. })) => self.tagged(&mut body, &v),
                    _ => {
                        let t: Vec<&str> = target.iter().map(|t| t.text.as_str()).collect();
                        self.diag("bad_set_target", true, t.join(" "));
                        return;
                    }
                }
                let e = self.expr(expr);
                put16(&mut body, e.len() as u16);
                body.extend_from_slice(&e);
                self.emit(on, op::SET, &body);
            }
            Kw::Call(toks) if self.co || self.x_call_line(toks) => {
                // An inline expression among the arguments, or an extender function: the
                // whole line is an extended expression run for what it does.
                let x = lex::xtokens(&self.text);
                self.call_line = true;
                let e = self.xcall_line(&x);
                self.call_line = false;
                let Some(mut e) = e else { return };
                // `DebugPrint "t= " + $f`: the call, and an operator after its arguments
                // with the rest of the line, which is dropped [observed:
                // DynamicDetectionSystem.esp ships two such lines with compiled bytes that
                // print the string alone, #2275].
                while let crate::ir::XExpr::Binary { l, .. } = e {
                    e = *l;
                    if matches!(e, crate::ir::XExpr::Call { .. }) {
                        self.diag("rest_of_line_dropped", false, self.text.clone());
                    }
                }
                if !matches!(e, crate::ir::XExpr::Call { .. }) {
                    self.diag("not_a_call", true, self.text.clone());
                    return;
                }
                let json = serde_json::to_vec(&e).unwrap_or_default();
                self.emit(None, op::X_EVAL, &json);
            }
            Kw::Call(toks) => {
                let Some((head, f)) = self.function(&toks[0]) else {
                    self.diag("unknown_function", true, toks[0].text.clone());
                    return;
                };
                let on = match head {
                    Some(h) => match self.reference(&h) {
                        Some(r) => Some(r),
                        None => {
                            self.diag("unresolved_name", true, h);
                            return;
                        }
                    },
                    None => None,
                };
                // A `:` right after an argument ends the arguments, in a mod's source
                // compiled by the engine [observed: four plugins of the owner's Blue Moon
                // list ship `player.say Hit:Ugh! || player.say Hit:Oof! …` with compiled
                // bytes saying the topic `Hit` alone, #2275; hypothesis: the editor's other
                // punctuation does not do so, `ShowMessage Msg :` drops it, scripts.md
                // §3.4].
                let end = if self.unknown_calls { toks.iter().skip(1).position(|t| t.kind == Kind::Punct && t.text == ":").map_or(toks.len(), |k| k + 1) } else { toks.len() };
                if end < toks.len() {
                    self.diag("rest_of_line_dropped", false, self.text.clone());
                }
                let args = Self::arg_tokens(&toks[1..end]);
                let body = self.params(&f, &args);
                self.emit(on, f.opcode, &body);
            }
        }
    }
}

fn is_neg_number(s: &str) -> bool {
    s.strip_prefix('-').is_some_and(lex::is_number)
}

fn static_op(s: &str) -> &'static str {
    ["==", "!=", "<=", ">=", "&&", "||", "<", ">", "+", "-", "*", "/", "%"].into_iter().find(|o| *o == s).unwrap_or("+")
}
