//! The extender's expressions (task A7; docs/spec-notes/ext-syntax.md, script-compiler.md
//! §9): tokens → a resolved [`XExpr`] tree, by precedence climbing over the operator table
//! of NVSE Expressions [documented: NVSE Expressions].
//!
//! Precedence, loosest first (the page's numbers): `:=` (0, right to left) · `||` (1) ·
//! `&&` and the compound assignments (2) · `:` `::` (3) · `==` `!=` (4) · `<` `>` `<=` `>=`
//! (5) · `|` (6) · `&` (7) · `<<` `>>` (8) · `+` `-` (9) · `*` `/` `%` (10) · `^` (11) ·
//! unary `-` `$` `#` `*` `&` (12) · `!` (13) · `[]` `->` (15). `:=` groups right to left
//! and `&&` binds tighter than `||` [documented; observed in the bytes the GECK with xNVSE
//! wrote, docs/spec-notes/ext-bytecode.md]; the other binary operators group left to right
//! and the compound assignments right to left [hypothesis, probes in
//! tools/capture/ext-probes/expressions].
//!
//! A function inside an expression takes up to as many arguments as it has parameters. An
//! argument is an operand: `Player.GetActorValue Health + 50` is `(GetAV Health) + 50`
//! [documented: Inline Expression; Let]. The arguments of an NVSE-aware function (the
//! wiki's category: Call, Print, SetFunctionValue, the ar_* family…) are whole
//! expressions [documented: Script Compiler Override, "Let, if eval, While, Print and any
//! other NVSE-Aware Function"]. Inside a compiler-override block (`Begin _GameMode`) the
//! wiki says `Sin 14 == 1` is `Sin (14 == 1)`, but xNVSE 6.4.9 gives `Sin 90 > 0.5` 1 there,
//! but `Abs -3 + 1` 2 [observed: probes expressions/08, 09 in the original game, which beat
//! the page]: there an argument takes arithmetic and stops at a comparison. Arguments are separated by a comma or by two
//! operands meeting without an operator (`Rand 0, 10`, `Rand 0 10`), and a `-` starts the
//! first argument (`Abs -3`) [hypothesis, probes in tools/capture/ext-probes/expressions].

use super::{Gen, Val};
use crate::compile::lex::{XKind, XTok};
use crate::compile::parse::DeclType;
use crate::ir::{FuncId, RefIndex, VarRef, VarSlot, XAssign, XCall, XExpr, XOp, XUnary};
use crate::table::ParamEnc;

pub(in crate::compile) struct P<'t> {
    t: &'t [XTok],
    i: usize,
}

impl<'t> P<'t> {
    pub(in crate::compile) fn new(t: &'t [XTok]) -> P<'t> {
        P { t, i: 0 }
    }
    fn peek(&self) -> Option<&'t XTok> {
        self.t.get(self.i)
    }
    fn peek2(&self) -> Option<&'t XTok> {
        self.t.get(self.i + 1)
    }
    fn next(&mut self) -> Option<&'t XTok> {
        let t = self.t.get(self.i);
        self.i += 1;
        t
    }
    pub(in crate::compile) fn done(&self) -> bool {
        self.i >= self.t.len()
    }
    pub(in crate::compile) fn rest(&self) -> String {
        self.t[self.i.min(self.t.len())..].iter().map(|t| t.text.as_str()).collect::<Vec<_>>().join(" ")
    }
}

enum Infix {
    Bin(XOp),
    Assign(XAssign),
}

/// Left and right binding power of an infix operator (twice the page's precedence, plus
/// one; a right-to-left operator's right side binds as loosely as its left).
fn infix(t: &XTok) -> Option<(u8, u8, Infix)> {
    if t.kind != XKind::Op {
        return None;
    }
    let l = |p: u8| (p * 2 + 1, p * 2 + 2);
    let r = |p: u8| (p * 2 + 1, p * 2 + 1);
    let (lb, rb, op) = match t.text.as_str() {
        ":=" | "=" => (r(0).0, r(0).1, Infix::Assign(XAssign::Set)),
        "||" => (l(1).0, l(1).1, Infix::Bin(XOp::Or)),
        "&&" => (l(2).0, l(2).1, Infix::Bin(XOp::And)),
        "+=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Add)),
        "-=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Sub)),
        "*=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Mul)),
        "/=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Div)),
        "^=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Pow)),
        "|=" => (r(2).0, r(2).1, Infix::Assign(XAssign::BitOr)),
        "&=" => (r(2).0, r(2).1, Infix::Assign(XAssign::BitAnd)),
        "%=" => (r(2).0, r(2).1, Infix::Assign(XAssign::Mod)),
        ":" => (l(3).0, l(3).1, Infix::Bin(XOp::Slice)),
        "::" => (l(3).0, l(3).1, Infix::Bin(XOp::Pair)),
        "==" => (l(4).0, l(4).1, Infix::Bin(XOp::Eq)),
        "!=" => (l(4).0, l(4).1, Infix::Bin(XOp::Ne)),
        "<" => (l(5).0, l(5).1, Infix::Bin(XOp::Lt)),
        ">" => (l(5).0, l(5).1, Infix::Bin(XOp::Gt)),
        "<=" => (l(5).0, l(5).1, Infix::Bin(XOp::Le)),
        ">=" => (l(5).0, l(5).1, Infix::Bin(XOp::Ge)),
        "|" => (l(6).0, l(6).1, Infix::Bin(XOp::BitOr)),
        "&" => (l(7).0, l(7).1, Infix::Bin(XOp::BitAnd)),
        "<<" => (l(8).0, l(8).1, Infix::Bin(XOp::Shl)),
        ">>" => (l(8).0, l(8).1, Infix::Bin(XOp::Shr)),
        "+" => (l(9).0, l(9).1, Infix::Bin(XOp::Add)),
        "-" => (l(9).0, l(9).1, Infix::Bin(XOp::Sub)),
        "*" => (l(10).0, l(10).1, Infix::Bin(XOp::Mul)),
        "/" => (l(10).0, l(10).1, Infix::Bin(XOp::Div)),
        "%" => (l(10).0, l(10).1, Infix::Bin(XOp::Mod)),
        "^" => (l(11).0, l(11).1, Infix::Bin(XOp::Pow)),
        _ => return None,
    };
    Some((lb, rb, op))
}

/// A prefix operator and the binding power of its operand.
fn prefix(t: &XTok) -> Option<(XUnary, u8)> {
    if t.kind != XKind::Op {
        return None;
    }
    Some(match t.text.as_str() {
        "-" => (XUnary::Neg, 25),
        "$" => (XUnary::Stringize, 25),
        "#" => (XUnary::Numericize, 25),
        "*" => (XUnary::Unbox, 25),
        "&" => (XUnary::Box, 25),
        "!" => (XUnary::Not, 27),
        _ => return None,
    })
}

/// Binding power of `[]` and `->` (precedence 15).
const POSTFIX: u8 = 31;
/// An argument of a function: an operand (prefix operators, `[]`, `->` and dot calls on
/// it, no binary operator).
const ARG: u8 = 25;
/// An argument of an NVSE-aware function: everything but the assignments.
const ARG_OVERRIDE: u8 = 2;
/// An argument inside a compiler-override block: arithmetic and the bitwise operators,
/// not the comparisons (`Abs -3 + 1` is Abs(-3 + 1), `Sin 90 > 0.5` is (Sin 90) > 0.5)
/// [observed: probes expressions/08, 09 in the original game; bitwise operators
/// hypothesis].
const ARG_IN_OVERRIDE: u8 = 13;

fn starts_operand(t: Option<&XTok>) -> bool {
    match t {
        Some(t) => match t.kind {
            XKind::Word | XKind::Num | XKind::Str | XKind::Open => true,
            XKind::Op => matches!(t.text.as_str(), "$" | "#" | "!"),
            _ => false,
        },
        None => false,
    }
}

/// A function as the extended grammar sees it.
struct XFn {
    func: FuncId,
    /// Arguments taken; `None`: all that follow.
    params: Option<usize>,
    opcode: Option<u16>,
    /// An NVSE-aware function: whole expressions as arguments.
    aware: bool,
}

impl Gen<'_> {
    fn xerr(&mut self, kind: &'static str, detail: impl Into<String>) -> Option<XExpr> {
        self.diag(kind, true, detail);
        None
    }

    /// A whole expression (`min` = the loosest binding power accepted).
    pub(in crate::compile) fn xexpr(&mut self, p: &mut P<'_>, min: u8) -> Option<XExpr> {
        let mut left = match p.peek().and_then(prefix) {
            Some((op, bp)) => {
                p.next();
                // `!-1` is refused, `-!0` compiles [observed: the GECK with xNVSE,
                // docs/spec-notes/ext-bytecode.md].
                if op == XUnary::Not && p.peek().is_some_and(|t| t.is(XKind::Op, "-")) {
                    return self.xerr("syntax", "too many operators (! before -)");
                }
                let e = self.xexpr(p, bp)?;
                XExpr::Unary { op, e: Box::new(e) }
            }
            None => self.xprimary(p)?,
        };
        while let Some(t) = p.peek() {
            if t.kind == XKind::LBracket && POSTFIX >= min {
                p.next();
                let was = std::mem::replace(&mut self.in_args, false);
                let inner = self.xexpr(p, 0);
                self.in_args = was;
                let inner = inner?;
                if p.next().map(|t| t.kind) != Some(XKind::RBracket) {
                    return self.xerr("syntax", "missing ]");
                }
                left = XExpr::Binary { op: XOp::Subscript, l: Box::new(left), r: Box::new(inner) };
                continue;
            }
            // `expr.Function args`: a call on a result or an element [documented:
            // Chained dot syntax, NVSE Expressions `.`].
            if t.kind == XKind::Word && t.text.starts_with('.') && t.text.len() > 1 && POSTFIX >= min {
                let name = &t.text[1..];
                let f = match self.xfunction(name) {
                    Some(f) => f,
                    // In a mod's source compiled by the engine: a call of that name, as
                    // [`Self::unknown_call`] [observed: JustAssortedMods.esp 4.6,
                    // `aA[0].ySIGetTagTrait "string"`, a function with no page].
                    None if self.unknown_calls => {
                        self.diag("unknown_function", false, name.to_string());
                        XFn { func: FuncId::Name(name.to_string()), params: None, opcode: None, aware: false }
                    }
                    None => return self.xerr("unknown_function", name.to_string()),
                };
                p.next();
                // A quoted hex FormID as the calling reference is that form: AguaFria.esm
                // ships `"171B39".AddItemAlt …` with bytes that call on 0x00171B39
                // [observed: SCPT 0x06000DFA, #2275].
                if let XExpr::Str { s } = &left {
                    let hex = (1..9).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit());
                    let form = hex.then(|| u32::from_str_radix(s, 16).ok()).flatten().and_then(|l| self.env.local_form(l)).and_then(|g| self.env.info(g));
                    if let Some(fi) = form {
                        let r = self.ref_index(crate::unit::RefSlot::Form(fi.form), s);
                        left = XExpr::Ref { r: RefIndex(r) };
                    }
                }
                let args = self.xargs(p, &f)?;
                left = XExpr::Call { call: XCall { on: Some(Box::new(left)), func: f.func, args } };
                continue;
            }
            if t.is(XKind::Op, "->") && POSTFIX >= min {
                p.next();
                let Some(k) = p.next().filter(|k| matches!(k.kind, XKind::Word | XKind::Str)) else {
                    return self.xerr("syntax", "-> without a key");
                };
                left = XExpr::Member { l: Box::new(left), key: k.text.clone() };
                continue;
            }
            if t.sign && self.in_args {
                break;
            }
            let Some((lb, rb, op)) = infix(t) else { break };
            if lb < min {
                break;
            }
            p.next();
            let right = self.xexpr(p, rb)?;
            left = match op {
                Infix::Bin(op) => XExpr::Binary { op, l: Box::new(left), r: Box::new(right) },
                Infix::Assign(op) => {
                    // `1 && iX += 5` compiles in the editor and fails when it runs, and the script
                    // goes on [observed: probe expressions/04]: a warning here, a run-time
                    // error in the VM; only a literal on the left is refused.
                    if matches!(left, XExpr::Num { .. } | XExpr::Str { .. }) {
                        return self.xerr("bad_assignment_target", t.text.clone());
                    }
                    if !matches!(left, XExpr::Var { .. } | XExpr::Binary { op: XOp::Subscript, .. } | XExpr::Member { .. }) {
                        self.diag("assignment_to_a_value", false, t.text.clone());
                    }
                    XExpr::Assign { op, target: Box::new(left), value: Box::new(right) }
                }
            };
        }
        Some(left)
    }

    fn xprimary(&mut self, p: &mut P<'_>) -> Option<XExpr> {
        let Some(t) = p.next() else { return self.xerr("syntax", "expression ends early") };
        match t.kind {
            XKind::Num => Some(XExpr::Num { value: t.text.parse().unwrap_or(0.0), text: t.text.clone() }),
            XKind::Str => Some(XExpr::Str { s: t.text.clone() }),
            XKind::Open => {
                // `(float f)` or `(int i = …)`: the type word of a variable declared there
                // ([`crate::compile::parse::inline_decls`]) is dropped.
                if p.peek().is_some_and(|w| w.kind == XKind::Word && DeclType::from_word(&w.text).is_some())
                    && p.peek2().is_some_and(|n| n.kind == XKind::Word && self.var(&n.text).is_some())
                    && p.t.get(p.i + 2).is_some_and(|a| a.kind == XKind::Close || a.is(XKind::Op, "=") || a.is(XKind::Op, ":="))
                {
                    p.next();
                }
                let was = std::mem::replace(&mut self.in_args, false);
                let was_paren = std::mem::replace(&mut self.in_paren, true);
                let e = self.xexpr(p, 0);
                self.in_args = was;
                self.in_paren = was_paren;
                let e = e?;
                // A name nothing knows, taken as its text, is not such an expression: an
                // operand after it means a call the catalogue lacks.
                let unquoted = ends_in_text(&e) && p.i > 0 && p.t[p.i - 1].kind == XKind::Word;
                if p.peek().is_some_and(|c| c.kind == XKind::Comma || c.kind != XKind::Close && (ends_in_call(&e) || !unquoted && starts_operand(Some(c)))) {
                    // What is left before the `)` once the expression inside is whole and
                    // ends in a call, or from a comma on (a function's surplus argument, `,
                    // x`, `38 || …`), is dropped [observed:
                    // the owner's Blue Moon list ships `(ar_size (GetActiveObjectives
                    // QuestREF))`, `((GetFormDescription Plural), iBonus)` and
                    // `(GetWeaponSkill rWeapon 38 || GetWeaponSkill rWeapon 45)` with
                    // compiled bytes holding `GetActiveObjectives` with no argument, the
                    // description alone and `GetWeaponSkill rWeapon` alone, #2275]. So is an
                    // operand that meets the whole expression without an operator (`("a" +
                    // $s ".nif")` is `"a" + $s`) [observed: MrShersh Functional
                    // Backpack.esp ships `AttachModel Player 2 ("…" + $sv ".nif")` with
                    // compiled bytes whose string sum ends at the variable, #2275].
                    let from = p.i;
                    let mut depth = 0i32;
                    while let Some(c) = p.peek() {
                        match c.kind {
                            XKind::Open | XKind::LBracket | XKind::LBrace => depth += 1,
                            XKind::Close if depth == 0 => break,
                            XKind::Close | XKind::RBracket | XKind::RBrace => depth -= 1,
                            _ => {}
                        }
                        p.next();
                    }
                    if p.peek().is_none() {
                        p.i = from;
                        return self.xerr("syntax", "missing )");
                    }
                    let dropped: Vec<&str> = p.t[from..p.i].iter().map(|t| t.text.as_str()).collect();
                    self.diag("rest_of_parentheses_dropped", false, dropped.join(" "));
                }
                if p.next().map(|c| c.kind) != Some(XKind::Close) {
                    return self.xerr("syntax", "missing )");
                }
                // On a `set` line the editor drops the parentheses (§3), so a call they
                // close before it has all its arguments takes the ones after the `)`:
                // `set t to 1 + (GetVariable (…)) Quest` reads the quest's variable
                // [observed: Animated Ingestibles.esp ships that line with compiled bytes
                // holding GetVariable with the inline expression and the quest, #2275].
                let mut e = e;
                if self.set_line {
                    if let XExpr::Call { call } = &mut e {
                        self.more_args(p, call)?;
                    }
                }
                Some(e)
            }
            XKind::Word if t.text.starts_with("__lambda") => match t.text["__lambda".len()..].parse::<usize>() {
                Ok(i) => self.lambda(i),
                Err(_) => self.xerr("unresolved_name", t.text.clone()),
            },
            XKind::Word => self.xword(p, &t.text),
            // `{params} => expr` [documented: Lambda].
            XKind::LBrace => {
                let mut words = Vec::new();
                while let Some(n) = p.next() {
                    match n.kind {
                        XKind::RBrace => break,
                        XKind::Comma => {}
                        XKind::Word => words.push(n.text.clone()),
                        _ => return self.xerr("syntax", format!("{} in a lambda's parameters", n.text)),
                    }
                }
                let mut params = Vec::new();
                for w in self.typed_params(&words) {
                    match self.var(&w) {
                        Some(v) => params.push(VarRef { on: None, var: VarSlot::Local { index: v.index as u16, letter: super::letter_of(&v.ty) } }),
                        None => return self.xerr("unresolved_name", w),
                    }
                }
                if !p.next().is_some_and(|a| a.is(XKind::Op, "=>")) {
                    return self.xerr("syntax", "a lambda's {params} without =>");
                }
                // The value takes the rest of the expression, an assignment included:
                // `({} => iInventory = iRefresh = 1)` assigns when the lambda runs
                // [observed: JustAssortedMods.esp 4.6 ships that line in JLMScript, as an
                // event handler and as a stored callback, with compiled bytes; hypothesis:
                // the original binds it so. Stopping before the assignment made the line
                // an assignment to the lambda itself, refused when it ran].
                let value = self.xexpr(p, 0)?;
                Some(XExpr::Lambda { params, body: Vec::new(), value: Some(Box::new(value)), index: self.next_lambda() })
            }
            _ => self.xerr("syntax", format!("unexpected {}", t.text)),
        }
    }

    /// More arguments for `call`, up to as many as its function takes.
    fn more_args(&mut self, p: &mut P<'_>, call: &mut XCall) -> Option<()> {
        let name = match &call.func {
            FuncId::Name(n) => n.clone(),
            FuncId::Opcode(op) => self.table.get(*op).map(|f| f.name.clone())?,
        };
        let Some(f) = self.xfunction(&name) else { return Some(()) };
        let left = match f.params {
            Some(n) if n <= call.args.len() => return Some(()),
            Some(n) => Some(n - call.args.len()),
            None => None,
        };
        if !starts_operand(p.peek()) {
            return Some(());
        }
        let more = self.xargs(p, &XFn { params: left, ..f })?;
        call.args.extend(more);
        Some(())
    }

    /// Whether `name` (with or without `Ref.`) is a function the catalogue lists without
    /// an opcode.
    pub(in crate::compile) fn xfunction_name(&self, name: &str) -> bool {
        let n = name.rsplit_once('.').map_or(name, |(_, n)| n);
        self.table.unnumbered.contains_key(&n.to_ascii_lowercase())
    }

    fn xfunction(&self, name: &str) -> Option<XFn> {
        if let Some(op) = self.numbered(name) {
            let f = self.table.get(op)?;
            let params = match f.param_count {
                Some(n) => Some(n),
                None if f.params.iter().any(|p| p.name.starts_with("...")) => None,
                None => Some(f.params.len().max(f.encodings.len())),
            };
            return Some(XFn { func: FuncId::Opcode(op), params, opcode: Some(op), aware: false });
        }
        let u = self.table.unnumbered.get(&name.to_ascii_lowercase())?;
        // `Call udf [up to 15 arguments]` [documented: Call; User Defined Function]; the
        // catalogue's rows for it come from example pages.
        let named = u.params.iter().filter(|p| !p.name.is_empty()).count();
        let params = if u.name.eq_ignore_ascii_case("call") {
            Some(16)
        // A format string takes up to 20 values [documented: String Formatting]; the
        // catalogue marks them as a parameter named or typed "Formatting"/"formatable".
        // SetNameEx takes format values too, though its page shows none [observed:
        // nvse-fifth, `SetNameEx "Gun %g" 5 rObj` compiles and names it "Gun 5"]; the
        // path setters' pages call their string a "formatable string" and list no
        // values, and the base form follows them (`SetModelPathEx "a.nif%z…%z" s … s
        // rVodka`, 20 values) [documented: SetModelPathEx, Bugs; SetIconPathEx;
        // SetBipedModelPathEx; SetBipedIconPathEx; SetNthFactionRankNameEx].
        } else if FORMAT_EX.iter().any(|n| u.name.eq_ignore_ascii_case(n))
            || u.params.iter().any(|p| {
            p.name.starts_with("...")
                || p.name.to_ascii_lowercase().contains("format")
                || p.wiki_type.as_deref().is_some_and(|t| t.to_ascii_lowercase().contains("format"))
        }) {
            None
        } else {
            // A parameter typed `arg(up to 15)` (DispatchEventAlt, the Call… family) or
            // `variable(up to 14)` stands for that many [documented: their pages' types],
            // and so does one named `filters(up-to-16)` (SetEventHandlerAlt) [documented:
            // its page; observed: FreePostMojave.esm and WeaponRequirementSystem.esp pass
            // two filters and a `"priority"::n` pair, five arguments, with compiled bytes
            // holding all five, #2275].
            let repeats: usize = u.params.iter().filter_map(|p| p.wiki_type.as_deref().and_then(up_to).or_else(|| up_to(&p.name))).map(|n| n.saturating_sub(1)).sum();
            Some(named + repeats)
        };
        Some(XFn { func: FuncId::Name(u.name.clone()), params, opcode: None, aware: u.aware })
    }

    /// A word: a call (`[Ref.]Function args`), `Quest.var`, a local, a form.
    fn xword(&mut self, p: &mut P<'_>, w: &str) -> Option<XExpr> {
        // `0x1F`, `0b101` [documented: Hexadecimal Notation; Binary Notation].
        let lw = w.to_ascii_lowercase();
        for (prefix, radix) in [("0x", 16), ("0b", 2)] {
            if let Some(d) = lw.strip_prefix(prefix) {
                if let Ok(v) = i64::from_str_radix(d, radix) {
                    return Some(XExpr::Num { value: v as f64, text: v.to_string() });
                }
            }
        }
        // `Quest.refVar.Function`: the function on the reference a quest variable holds
        // [observed: JustAssortedMods.esp, `JLM.rContainer.LNGetName`].
        if let Some((target, fname)) = w.rsplit_once('.').filter(|(t, _)| t.contains('.')) {
            if let (Some(f), Some((q, v))) = (self.xfunction(fname), target.split_once('.')) {
                let on = self.remote(q, v)?;
                let args = self.xargs(p, &f)?;
                return Some(XExpr::Call { call: XCall { on: Some(Box::new(val_expr(on))), func: f.func, args } });
            }
        }
        let (head, name) = match w.split_once('.') {
            Some((h, n)) if !h.is_empty() => (Some(h), n),
            Some((_, n)) => (None, n),
            None => (None, w),
        };
        let is_local = head.is_none() && self.var(name).is_some();
        if !is_local {
            if let Some(f) = self.xfunction(name) {
                let on = match head {
                    Some(h) => match self.reference(h) {
                        Some(r) => Some(Box::new(XExpr::Ref { r: RefIndex(r) })),
                        // `GetSelf.Function`: the call on what a function of no arguments
                        // returns [documented: Chained dot syntax, "GetSelf is a function
                        // that returns a reference"; observed: the projectile set's
                        // proj.reg rows compile in the original, #2036].
                        None => match self.xfunction(h) {
                            Some(hf) => Some(Box::new(XExpr::Call { call: XCall { on: None, func: hf.func, args: Vec::new() } })),
                            None => return self.xerr("unresolved_name", h.to_string()),
                        },
                    },
                    None => None,
                };
                let args = self.xargs(p, &f)?;
                return Some(XExpr::Call { call: XCall { on, func: f.func, args } });
            }
        }
        // `Ref.Name args` or `Quest.refVar.Name args` where nothing knows `Name`, in a
        // mod's source compiled by the engine: a call of that name on the reference.
        if self.unknown_calls && head.is_some() && self.names_nothing(w) {
            return self.unknown_call(p, w);
        }
        if let Some(h) = head {
            let v = self.remote(h, name)?;
            return Some(val_expr(v));
        }
        if let Some(v) = self.var(name) {
            return Some(XExpr::Var {
                var: VarRef { on: None, var: VarSlot::Local { index: v.index as u16, letter: super::letter_of(&v.ty) } },
                form: self.ref_vars.contains(&name.to_ascii_lowercase()),
            });
        }
        match self.form(name) {
            Some(f) => Some(val_expr(self.form_val(f, name))),
            // A bare X, Y or Z that names nothing is an axis, for extender functions whose
            // parameter types the table does not carry (`GetLocalGravity Z`) [observed:
            // jip-getters localgravity rows compile in the original].
            None if self.in_args && name.len() == 1 && matches!(name.to_ascii_uppercase().as_str(), "X" | "Y" | "Z") => {
                Some(XExpr::Axis { c: name.to_ascii_uppercase().chars().next().unwrap_or('Z') })
            }
            // In parentheses a name nothing knows with an operand right after it can only
            // be a call (as text it would leave two operands side by side), in a mod's
            // source compiled by the engine [observed: JIP Companions Command & Control
            // 2.60, `SetFunctionValue (CCCInFaction SomeFaction == 0)`].
            None if self.unknown_calls && head.is_none() && self.in_paren && !self.in_args && starts_operand(p.peek()) => self.unknown_call(p, name),
            // Any other bare word that names nothing is its own text, with the editor's
            // "Unquoted argument" warning, parenthesised too [observed: nvse-fourth,
            // `SetFunctionValue (GetFOSEVersion)` returns the text "GetFOSEVersion" in xNVSE
            // 6.4.9].
            None if self.in_arg_any => {
                self.diag("unquoted_argument", false, name.to_string());
                Some(XExpr::Str { s: name.to_string() })
            }
            // An unknown name where a call can stand (a call line's start, an operand of
            // a `let`, an `if eval` or an `eval`), in a mod's source compiled by the
            // engine: a call of that name, reporting "not built" when it runs
            // ([`super::super::Seed::unknown_calls`]) [observed: TaleOfTwoWastelands.esm
            // SCPT 001880 calls TTW_EnableRadioFix and InitRockItLauncher, plugin functions
            // the wiki does not document; JustAssortedMods.esp 4.6, `sATag =
            // ySIGetTagTrait "string" (aA[0])`]. As an argument the name stays its own
            // text (the arm above).
            None if self.unknown_calls && head.is_none() => self.unknown_call(p, name),
            None => self.xerr("unresolved_name", name.to_string()),
        }
    }

    /// A call of a function nothing knows, by its name: `Name`, `Ref.Name` or
    /// `Quest.refVar.Name` ([`Gen::names_nothing`] said so for the dotted forms), with
    /// every argument that follows. It compiles with a warning and reports "not built"
    /// when it runs, which skips the statement or reads 0 and lets the script go on.
    fn unknown_call(&mut self, p: &mut P<'_>, w: &str) -> Option<XExpr> {
        let (target, name) = match w.rsplit_once('.') {
            Some((t, n)) => (Some(t), n),
            None => (None, w),
        };
        let on = match target.map(|t| (t, t.split_once('.'))) {
            None => None,
            Some((_, Some((q, v)))) => Some(Box::new(val_expr(self.remote(q, v)?))),
            Some((t, None)) => match self.reference(t) {
                Some(r) => Some(Box::new(XExpr::Ref { r: RefIndex(r) })),
                None => return self.xerr("unresolved_name", t.to_string()),
            },
        };
        self.diag("unknown_function", false, name.to_string());
        let f = XFn { func: FuncId::Name(name.to_string()), params: None, opcode: None, aware: false };
        let args = self.xargs(p, &f)?;
        Some(XExpr::Call { call: XCall { on, func: f.func, args } })
    }

    /// Whether the catalogue gives `f` a first parameter that is not optional.
    fn first_param_required(&self, f: &XFn) -> bool {
        let first = match (&f.func, f.opcode) {
            (_, Some(o)) => self.table.get(o).and_then(|x| x.params.first()),
            (FuncId::Name(n), None) => self.table.unnumbered.get(&n.to_ascii_lowercase()).and_then(|u| u.params.first()),
            _ => None,
        };
        first.is_some_and(|p| !p.optional)
    }

    /// The call stands inside an expression: an `if eval`, `let` or `while` line's, an
    /// argument of another call, or parentheses; not the line's own call.
    fn in_expression(&self) -> bool {
        !self.call_line || self.in_args || self.in_paren
    }

    /// Parameter `k` of `f` is documented as a whole expression (`eval`, `TestExpr`,
    /// `Print`, `while`, `ToString`), or `f` is `let`, whose assignment is one
    /// (`eval x && let s += "+"` [observed: JustAssortedMods.esp ships such lines with
    /// compiled bytes]).
    fn expression_param(&self, f: &XFn, k: usize) -> bool {
        let FuncId::Name(name) = &f.func else { return false };
        if name.eq_ignore_ascii_case("let") {
            return true;
        }
        let p = self.table.unnumbered.get(&name.to_ascii_lowercase()).and_then(|u| u.params.get(k));
        p.and_then(|p| p.wiki_type.as_deref()).is_some_and(|t| matches!(t.to_ascii_lowercase().as_str(), "expression" | "exp"))
    }

    /// The function is xNVSE's (or FOSE's) and its parameter `k` is a plain string, not a
    /// format string.
    fn plain_string_param(&self, f: &XFn, k: usize) -> bool {
        let FuncId::Name(n) = &f.func else { return false };
        let Some(u) = self.table.unnumbered.get(&n.to_ascii_lowercase()) else { return false };
        let formatted = u.params.iter().any(|p| p.name.starts_with("...") || p.name.to_ascii_lowercase().contains("format"));
        // The wiki's type is "string", not "string_var" (Sv_Erase takes the variable).
        matches!(u.origin.as_str(), "NVSE" | "FOSE1") && !formatted && u.params.get(k).is_some_and(|p| p.kind == "string" && p.wiki_type.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("string")))
    }

    /// The plugin function's parameter `k` is a string (`kind: "string"`).
    fn string_param(&self, f: &XFn, k: usize) -> bool {
        let FuncId::Name(n) = &f.func else { return false };
        self.table.unnumbered.get(&n.to_ascii_lowercase()).and_then(|u| u.params.get(k)).is_some_and(|p| p.kind == "string")
    }

    /// An extender function without an opcode whose parameter `k` the catalogue gives as
    /// an axis (`kind: "axis"`): `X`, `Y` or `Z` there is the axis even when a record of
    /// the load order has that editor ID [observed: sup3, `SetTFCAngle Z 45` read back 45
    /// in the original with OldWorldBlues.esm and LonesomeRoad.esm loaded, each with a
    /// reference named `Z`].
    fn axis_param(&self, f: &XFn, k: usize) -> bool {
        let FuncId::Name(n) = &f.func else { return false };
        f.opcode.is_none() && self.table.unnumbered.get(&n.to_ascii_lowercase()).and_then(|u| u.params.get(k)).is_some_and(|p| p.kind == "axis" || p.wiki_type.as_deref().is_some_and(axis_letters))
    }

    /// The function's parameter `k` takes a form, or is named for a global (the base
    /// game's GetGlobalValue included).
    fn form_param(&self, f: &XFn, k: usize) -> bool {
        let params = match (&f.func, f.opcode) {
            (_, Some(op)) => self.table.get(op).map(|d| &d.params),
            (FuncId::Name(n), None) => self.table.unnumbered.get(&n.to_ascii_lowercase()).map(|u| &u.params),
            _ => None,
        };
        params.and_then(|ps| ps.get(k)).is_some_and(|p| matches!(p.kind.as_str(), "form" | "reference") || p.name.to_ascii_lowercase().contains("global"))
    }

    fn xargs(&mut self, p: &mut P<'_>, f: &XFn) -> Option<Vec<XExpr>> {
        let mut args = Vec::new();
        loop {
            if f.params.is_some_and(|n| args.len() >= n) {
                break;
            }
            let mut comma = false;
            // A comma between arguments, and one between the name and a first argument
            // the function cannot go without (`ListGetNthForm, SomeList, i`) [observed:
            // Unofficial Patch NVSE Plus ships `rForm = ListGetNthForm, <list>, iPos`
            // with compiled bytes; hypothesis: the original's compiler takes a comma
            // there as it takes a space]. Before a first argument the function can go
            // without, or none, the comma ends the call (a list's next element).
            let leading = args.is_empty() && self.first_param_required(f) && starts_operand(p.peek2());
            if (!args.is_empty() || leading) && p.peek().is_some_and(|t| t.kind == XKind::Comma) {
                p.next();
                comma = true;
            }
            // A `-` starts the first argument, or one after a comma (`Rand -5, -1`), or a
            // signed number after a space (`GetMaxOf -1 -5`).
            let minus = p.peek().is_some_and(|t| t.is(XKind::Op, "-"));
            let minus_first = f.params != Some(0) && minus && (comma || args.is_empty() || p.peek().is_some_and(|t| t.sign));
            // A `&` after a comma boxes the argument (`Call Fn, &rRef, 0`) [documented: NVSE
            // Expressions, unary `&` "Box"; observed: Famine.esp ships that line with
            // compiled bytes holding the variable and the box operator as the first of
            // five arguments, #2275].
            let boxed = comma && p.peek().is_some_and(|t| t.is(XKind::Op, "&"));
            // `{expr}` not followed by `=>` is the expression, one argument (`call Fn
            // {(BuildRef 0 n)}`, `call Fn {Quest.var}`) [observed: Animated Ingestibles.esp
            // ships both with compiled bytes holding the value alone as the one argument,
            // #2275].
            let braced = p.peek().is_some_and(|t| t.kind == XKind::LBrace) && !lambda_at(p);
            let boxing = f.params != Some(0) && p.peek().is_some_and(|t| t.sign && (t.is(XKind::Op, "*") || t.is(XKind::Op, "&")));
            if !starts_operand(p.peek()) && !minus_first && !boxed && !boxing && !braced {
                break;
            }
            if braced {
                p.next();
                let was = std::mem::replace(&mut self.in_args, false);
                let was_paren = std::mem::replace(&mut self.in_paren, true);
                let e = self.xexpr(p, 0);
                self.in_args = was;
                self.in_paren = was_paren;
                let e = e?;
                if p.next().map(|c| c.kind) != Some(XKind::RBrace) {
                    self.xerr("syntax", "missing }");
                    return None;
                }
                args.push(e);
                continue;
            }
            // A string variable given by name to an xNVSE function's plain string
            // parameter, on a line that is that call alone, is its name as text [observed: nvse-sixth,
            // `SetStringIniSetting "sLanguage:General" sText` set the text "sText";
            // documented: SetNameEx, "does not accept string variables directly … must be
            // converted", `$sv`; hypothesis: JIP's and the other plugins' functions differ].
            if self.call_line && !self.in_args && !f.aware && !self.in_paren && self.plain_string_param(f, args.len()) {
                if let Some(t) = p.peek().filter(|t| t.kind == XKind::Word && !t.text.contains('.')).cloned() {
                    let string_var = self.var(&t.text).is_some_and(|v| matches!(&v.ty, crate::unit::VarType::Other(s) if s == "string_var"));
                    let alone = !p.peek2().is_some_and(|n| matches!(n.kind, XKind::LBracket | XKind::Op));
                    if string_var && alone {
                        p.next();
                        args.push(XExpr::Str { s: t.text.clone() });
                        continue;
                    }
                }
            }
            // A game setting's name given alone to a plugin function's string parameter
            // is the name as text, not the setting's record [observed: WaterToTravel.esp
            // ships `SetStringSetting sNoFastTravelScriptBlock "…"` with compiled bytes
            // whose reference list does not hold the setting, #2275].
            if self.string_param(f, args.len()) {
                if let Some(t) = p.peek().filter(|t| t.kind == XKind::Word && !t.text.contains('.')).cloned() {
                    let alone = !p.peek2().is_some_and(|n| matches!(n.kind, XKind::LBracket | XKind::Op));
                    let setting = self.var(&t.text).is_none() && self.form(&t.text).is_some_and(|fi| &fi.sig == b"GMST");
                    if alone && setting {
                        p.next();
                        args.push(XExpr::Str { s: t.text.clone() });
                        continue;
                    }
                }
            }
            // A global's name given alone to a plugin function's form parameter (or one
            // named for a global) is the global, not its value [observed: plugins3,
            // `GetGlobalValue TimeScale` reads 30 and `SetGlobalValue TimeScale 31` sets
            // it in the original].
            if self.form_param(f, args.len()) {
                if let Some(t) = p.peek().filter(|t| t.kind == XKind::Word && !t.text.contains('.')).cloned() {
                    let alone = !p.peek2().is_some_and(|n| matches!(n.kind, XKind::LBracket | XKind::Op));
                    let global = self.var(&t.text).is_none().then(|| self.form(&t.text)).flatten().filter(|fi| &fi.sig == b"GLOB");
                    if let (true, Some(fi)) = (alone, global) {
                        p.next();
                        let r = self.ref_index(crate::unit::RefSlot::Form(fi.form), &t.text);
                        args.push(XExpr::Ref { r: RefIndex(r) });
                        continue;
                    }
                }
                // A word or number of hex digits that names nothing, given alone to a form
                // parameter, is the form with that FormID when the load order has one
                // [observed 2026-11-14, nvse-cmd-probes in the stock game, the runtime
                // compiler: `AuxVarSetFlt "*_q" 7 0 f` reads back 7 under Caps001 (0xF),
                // `a` under Lockpick (0xA), `14` under PlayerRef; `g` and `FFFFFF` (no form)
                // do not compile; Armor Damage Fix's gr_CondDamFix.txt ships `… 0 f`].
                if let Some(t) = p.peek().filter(|t| matches!(t.kind, XKind::Word | XKind::Num) && !t.text.contains('.')).cloned() {
                    let alone = !p.peek2().is_some_and(|n| matches!(n.kind, XKind::LBracket | XKind::Op));
                    let named = self.var(&t.text).is_some() || self.form(&t.text).is_some();
                    if let (true, false, Some(fi)) = (alone, named, self.hex_form_id(&t.text)) {
                        p.next();
                        args.push(val_expr(self.form_val(fi, &t.text)));
                        continue;
                    }
                }
            }
            let enc = match f.opcode {
                Some(o) => self.table.param_enc(o, args.len()),
                None if self.axis_param(f, args.len()) => ParamEnc::Axis,
                None => ParamEnc::Value,
            };
            // Inside an expression an NVSE-aware function's argument stops at a comparison
            // or a logical operator, which then applies to the call's result: shipped
            // bytes of `if eval IsModLoaded "X.esm" == 0` call it with the string alone and
            // compare its result with 0, and of `if eval Ar_HasKey aConvo, "Conditions" ==
            // 1` pass two arguments and compare with 1 [observed: mil_Add-On.esp SCPT
            // 0x0700254B, Party Banter 2.esm SCPT 0x0701D892, #2275]. Arithmetic stays in
            // the argument and a range or pair (`1:2`, `"k"::v`) continues it, as in an
            // override block [hypothesis].
            let nested_aware = f.aware && self.in_expression() && !self.expression_param(f, args.len());
            let min = if f.aware && !nested_aware {
                ARG_OVERRIDE
            } else if self.co || nested_aware {
                ARG_IN_OVERRIDE
            } else {
                ARG
            };
            let was = std::mem::replace(&mut self.in_args, true);
            let was_any = std::mem::replace(&mut self.in_arg_any, true);
            let mut a = self.xarg(p, &enc, min);
            while nested_aware && a.is_some() {
                let op = match p.peek() {
                    Some(t) if t.is(XKind::Op, ":") => XOp::Slice,
                    Some(t) if t.is(XKind::Op, "::") => XOp::Pair,
                    _ => break,
                };
                p.next();
                let r = self.xarg(p, &ParamEnc::Value, min);
                a = match (a, r) {
                    (Some(l), Some(r)) => Some(XExpr::Binary { op, l: Box::new(l), r: Box::new(r) }),
                    _ => None,
                };
            }
            self.in_args = was;
            self.in_arg_any = was_any;
            let a = a?;
            if self.runtime {
                self.runtime_refusal(f, args.len(), &a);
            }
            args.push(a);
        }
        Some(args)
    }

    /// What the extender's runtime compiler refuses in argument `k` of `f`
    /// ([`crate::compile::compile_runtime`]; docs/spec-notes/ext-syntax.md §11):
    ///
    /// - a number literal with a fraction for an int parameter of a plugin function
    ///   (`SetBaseKarma 37.5`, `ApplyEasingAlt 0.5 1.5 0`) [observed: jip.n7 karma.set,
    ///   jip6 dest.settrait.1.1.3.5, showoff3 so3.ease.badcode and so3.seed.range.fraction,
    ///   jg3 casino.setwin.tops.frac, jg4 regn.setpriority.gs.frac, sup1 roundalt.flag2 and
    ///   words.7p9 did not compile; the whole-number rows beside each did]. A literal
    ///   with nothing after the point (`37.0`) is not refused [hypothesis: no row];
    /// - the result of a command registered without a return type given to a form
    ///   parameter (`GetFormIDString (GetPackageLocation rG)`) [observed: nvse.misc
    ///   pkg.nth0.location, nvse-seventh pkg.removeat0, jg6 GetCurrentFurnitureRef
    ///   (ext-jg6.md), all under GetFormIDString; hypothesis: any form parameter].
    fn runtime_refusal(&mut self, f: &XFn, k: usize, a: &XExpr) {
        let (FuncId::Name(name), None) = (&f.func, f.opcode) else { return };
        let low = name.to_ascii_lowercase();
        let literal = match a {
            XExpr::Num { value, text } => Some((*value, text.clone())),
            XExpr::Unary { op: XUnary::Neg, e } => match &**e {
                XExpr::Num { value, text } => Some((-*value, format!("-{text}"))),
                _ => None,
            },
            _ => None,
        };
        if let Some((_, text)) = literal.filter(|(v, _)| v.fract() != 0.0) {
            // A plugin's function, as every row observed; xNVSE's own keep the fraction
            // [hypothesis: the owner's Blue Moon list's Vanilla SWEEP files pass
            // `SetWeaponFireRate 7.5` (NVSE, whose page types the rate int while
            // GetWeaponFireRate gives a float), #2275].
            let plugin = self.table.unnumbered.get(&low).is_some_and(|u| !matches!(u.origin.as_str(), "NVSE" | "FOSE1" | "GECK1"));
            let int = INT_PARAMS_OBSERVED.contains(&(low.as_str(), k))
                || plugin && self.table.unnumbered.get(&low).and_then(|u| u.params.get(k)).is_some_and(|p| p.kind == "int");
            if int {
                self.diag("fraction_for_int_parameter", true, format!("{name}: {text}"));
            }
        }
        if let XExpr::Call { call: XCall { func: FuncId::Name(inner), .. } } = a {
            if UNTYPED_RESULTS.iter().any(|u| inner.eq_ignore_ascii_case(u)) && self.form_param(f, k) {
                self.diag("untyped_result_for_form", true, format!("{inner} as {name}'s argument"));
            }
        }
    }

    /// Arguments left after a call line's call has all it takes, parsed as operands and
    /// dropped: their count, or `None` when what is left is not a run of arguments.
    pub(in crate::compile) fn surplus_args(&mut self, p: &mut P<'_>) -> Option<usize> {
        let mut n = 0;
        while !p.done() {
            if p.peek().is_some_and(|t| t.kind == XKind::Comma) {
                p.next();
            }
            let minus = p.peek().is_some_and(|t| t.is(XKind::Op, "-") && t.sign);
            if !starts_operand(p.peek()) && !minus {
                return None;
            }
            let was = std::mem::replace(&mut self.in_args, true);
            let was_any = std::mem::replace(&mut self.in_arg_any, true);
            let a = self.xexpr(p, ARG);
            self.in_args = was;
            self.in_arg_any = was_any;
            a?;
            n += 1;
        }
        Some(n)
    }

    /// One argument. A name in a parameter that takes an enumeration, an axis or a string
    /// is that, when nothing follows it but the next argument.
    fn xarg(&mut self, p: &mut P<'_>, enc: &ParamEnc, min: u8) -> Option<XExpr> {
        // Outside an override block an argument is an operand, so what follows the name
        // (`Health + 50`) is not part of it.
        let single = p.peek().is_some_and(|t| matches!(t.kind, XKind::Word | XKind::Str))
            && !p.peek2().is_some_and(|n| n.kind == XKind::LBracket || n.is(XKind::Op, "->"));
        if single && *enc != ParamEnc::Value {
            let t = p.next()?;
            return match enc {
                ParamEnc::Enum(kind) => {
                    let code = t.text.parse::<u16>().ok().or_else(|| self.enums.get(kind).and_then(|m| m.get(&t.text.to_ascii_lowercase())).copied());
                    match code {
                        Some(code) => Some(XExpr::Enum { code }),
                        // As the vanilla grammar's (`Gen::arg`): a warning and the code
                        // nothing has, in a mod's source compiled by the engine.
                        None if self.unknown_calls => {
                            self.diag("unknown_enum_name", false, format!("{kind}: {}", t.text));
                            Some(XExpr::Enum { code: super::UNKNOWN_ENUM })
                        }
                        None => self.xerr("unknown_enum_name", format!("{kind}: {}", t.text)),
                    }
                }
                ParamEnc::Axis => match t.text.to_ascii_uppercase().as_str() {
                    a @ ("X" | "Y" | "Z") => Some(XExpr::Axis { c: a.chars().next().unwrap_or('X') }),
                    _ => self.xerr("bad_axis", t.text.clone()),
                },
                _ => Some(XExpr::Str { s: t.text.clone() }),
            };
        }
        self.xexpr(p, min)
    }
}

/// A wiki type of axis letters, `X/Z` or `X/Y/Z` (SetTFCAngle's page types its axis
/// `X/Z`, which the catalogue does not take for an axis).
fn axis_letters(t: &str) -> bool {
    t.len() >= 3 && t.split('/').all(|c| matches!(c.trim(), "X" | "Y" | "Z" | "x" | "y" | "z"))
}

/// Whether the `{` at the parser's place opens a lambda's parameters (`{a, b} =>`).
fn lambda_at(p: &P<'_>) -> bool {
    let mut j = p.i + 1;
    while let Some(t) = p.t.get(j) {
        match t.kind {
            XKind::RBrace => return p.t.get(j + 1).is_some_and(|a| a.is(XKind::Op, "=>")),
            XKind::Word | XKind::Comma => j += 1,
            _ => return false,
        }
    }
    false
}

/// The expression's last operand is text (`a || SomeName`, a name taken as its text).
fn ends_in_text(e: &XExpr) -> bool {
    match e {
        XExpr::Str { .. } => true,
        XExpr::Binary { r, .. } => ends_in_text(r),
        _ => false,
    }
}

/// The expression's last operand is a call (`a && GetWeaponSkill w`).
fn ends_in_call(e: &XExpr) -> bool {
    match e {
        XExpr::Call { .. } => true,
        XExpr::Binary { r, .. } => ends_in_call(r),
        XExpr::Unary { e, .. } => ends_in_call(e),
        _ => false,
    }
}

fn val_expr(v: Val) -> XExpr {
    match v {
        Val::Num(text) => XExpr::Num { value: text.parse().unwrap_or(0.0), text },
        Val::Local { index, letter } => XExpr::Var { var: VarRef { on: None, var: VarSlot::Local { index, letter } }, form: false },
        Val::Global(r) => XExpr::Var { var: VarRef { on: None, var: VarSlot::Global { global: RefIndex(r) } }, form: false },
        Val::Remote { on, index, letter } => XExpr::Var { var: VarRef { on: Some(RefIndex(on)), var: VarSlot::Local { index, letter } }, form: false },
        Val::Ref(r) => XExpr::Ref { r: RefIndex(r) },
    }
}

/// Plugin functions' parameters the original's runtime compiler takes as int though the
/// catalogue gives a float, by lower-case name and position: SetBaseKarma's karma
/// [observed: jip.n7 karma.set, `SetBaseKarma 37.5 rBase` did not compile where
/// karma.set.int, `SetBaseKarma 37 rBase`, did].
const INT_PARAMS_OBSERVED: [(&str, usize); 1] = [("setbasekarma", 0)];

/// Commands registered without a return type whose result may not be given to a form
/// parameter at run time [observed: nvse.misc pkg.nth0.location, nvse-seventh
/// pkg.removeat0 and ext-jg6.md's GetCurrentFurnitureRef, each `GetFormIDString (…)`
/// refused in the original; the catalogue's return types do not tell them apart].
const UNTYPED_RESULTS: [&str; 3] = ["GetPackageLocation", "RemovePackageAt", "GetCurrentFurnitureRef"];

/// Functions whose string takes format values the catalogue does not list (the VM's
/// `…Ex` setters, vm.rs).
const FORMAT_EX: [&str; 6] = ["SetNameEx", "SetNthFactionRankNameEx", "SetModelPathEx", "SetIconPathEx", "SetBipedModelPathEx", "SetBipedIconPathEx"];

/// The count of a catalogue type or name `…(up to N)`: `arg(up to 15)` is 15,
/// `filters(up-to-16)` 16.
fn up_to(wiki_type: &str) -> Option<usize> {
    let t = wiki_type.to_ascii_lowercase().replace("up-to-", "up to ");
    let n = t.split_once("(up to ")?.1.strip_suffix(')')?;
    n.trim().parse().ok()
}

#[cfg(test)]
mod up_to_tests {
    #[test]
    fn repeated_parameters() {
        assert_eq!(super::up_to("arg(up to 15)"), Some(15));
        assert_eq!(super::up_to("variable(up to 14)"), Some(14));
        assert_eq!(super::up_to("string"), None);
        assert_eq!(super::up_to("filters(up-to-16)"), Some(16));
    }
}
