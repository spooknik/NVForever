//! IR → canonical text (docs/spec-notes/scripts.md §3): one line per compiled statement,
//! in compiled order, in a normalised form that the independent oracle
//! (`tools/capture/oracle_script.py`) can also produce from the shipped source text.
//!
//! The form in short: lower-case words; functions by their catalogue name; references
//! as load-order FormIDs (`0x0011A2B3`); local variables by declared name; another
//! script's variable as `<FormID>.<name>`; numbers normalised; expressions in postfix
//! order, calls inside them as `name(args)`; comments and blank lines gone.

use crate::ir::*;
use crate::table::{FunctionTable, ParamEnc};
use crate::unit::{RefSlot, ScriptUnit};

/// Name lookups that need the rest of the load order.
pub trait Names {
    /// Name of variable `index` of the script attached to `form` (a quest, or a reference
    /// or base form with a script), as `unit`'s plugin sees it (its masters and itself:
    /// the names the source was compiled against). None when it cannot be resolved.
    fn remote_var(&self, unit: &ScriptUnit, form: u32, index: u16) -> Option<String>;
}

/// No lookups: other scripts' variables print as `<FormID>.<letter><index>`.
pub struct NoNames;

impl Names for NoNames {
    fn remote_var(&self, _unit: &ScriptUnit, _form: u32, _index: u16) -> Option<String> {
        None
    }
}

/// A number as the canonical form prints it: an integral value without a fraction,
/// anything else as the shortest decimal that reads back to the same f64, never with an
/// exponent.
pub fn number(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// A word (string parameter, enumerated name): lower-case; quoted when it is not made
/// only of ASCII letters, digits and underscores.
pub fn word(s: &str) -> String {
    let l = s.to_lowercase();
    if !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        l
    } else {
        format!("\"{l}\"")
    }
}

pub struct Decompiler<'a> {
    pub unit: &'a ScriptUnit,
    pub table: &'a FunctionTable,
    pub names: &'a dyn Names,
}

fn hex(b: &[u8]) -> String {
    fnv_util::sha256::to_hex(b)
}

impl Decompiler<'_> {
    fn local_name(&self, index: u32, letter: Option<VarLetter>) -> String {
        match self.unit.var(index) {
            Some(v) if !v.name.is_empty() => v.name.to_lowercase(),
            _ => {
                let l = letter.map(|l| l.byte() as char).unwrap_or('v');
                format!("{l}{index}")
            }
        }
    }

    fn form(f: u32) -> String {
        format!("0x{f:08X}")
    }

    /// A reference list entry.
    fn slot(&self, r: RefIndex) -> String {
        match self.unit.slot(r) {
            Some(RefSlot::Form(f)) => Self::form(f),
            Some(RefSlot::Local(i)) => self.local_name(i, None),
            Some(RefSlot::Unmapped(l)) => format!("?0x{l:08X}"),
            None => format!("#r{}", r.0),
        }
    }

    fn var(&self, v: &VarRef) -> String {
        match (v.on, v.var) {
            (None, VarSlot::Local { index, letter }) => self.local_name(index as u32, Some(letter)),
            (_, VarSlot::Global { global }) => self.slot(global),
            (Some(on), VarSlot::Local { index, letter }) => {
                let name = match self.unit.slot(on) {
                    Some(RefSlot::Form(f)) => self.names.remote_var(self.unit, f, index).map(|n| n.to_lowercase()),
                    _ => None,
                };
                let name = name.unwrap_or_else(|| format!("{}{index}", letter.byte() as char));
                format!("{}.{name}", self.slot(on))
            }
        }
    }

    fn enc_kind(&self, opcode: Option<u16>, pos: usize) -> Option<String> {
        match opcode.map(|o| self.table.param_enc(o, pos)) {
            Some(ParamEnc::Enum(k)) => Some(k),
            _ => None,
        }
    }

    fn arg(&self, a: &Arg, opcode: Option<u16>, pos: usize, out: &mut Vec<String>) {
        match a {
            Arg::Int(v) => out.push(v.to_string()),
            Arg::Float(v) => out.push(number(*v)),
            Arg::Ref(r) => out.push(self.slot(*r)),
            Arg::Var(v) => out.push(self.var(v)),
            Arg::Enum(code) => {
                let name = self.enc_kind(opcode, pos).and_then(|k| self.table.enum_name(&k, *code).map(word));
                out.push(name.unwrap_or_else(|| format!("#{code}")));
            }
            Arg::Axis(c) => out.push(c.to_ascii_lowercase().to_string()),
            Arg::String(s) => out.push(word(s)),
            Arg::FormatVars(vs) => {
                for v in vs {
                    self.arg(v, None, 0, out);
                }
            }
            // The message duration: written only when the source gives one (0 is what
            // the compiler stores otherwise).
            Arg::Int32(v) => {
                if *v != 0 {
                    out.push(v.to_string())
                }
            }
            Arg::Raw(b) => out.push(format!("?{}", hex(b))),
        }
    }

    fn args(&self, args: &[Arg], opcode: Option<u16>) -> Vec<String> {
        let mut out = Vec::new();
        for (i, a) in args.iter().enumerate() {
            self.arg(a, opcode, i, &mut out);
        }
        out
    }

    fn func_name(&self, opcode: u16) -> String {
        match self.table.get(opcode) {
            Some(f) => f.name.to_lowercase(),
            None => format!("#0x{opcode:04x}"),
        }
    }

    fn call_head(&self, c: &Call) -> String {
        match c.on {
            Some(r) => format!("{}.{}", self.slot(r), self.func_name(c.opcode)),
            None => self.func_name(c.opcode),
        }
    }

    /// A call statement: `[ref.]name arg arg`.
    pub fn call_stmt(&self, c: &Call) -> String {
        let mut s = self.call_head(c);
        for a in self.args(&c.args, Some(c.opcode)) {
            s.push(' ');
            s.push_str(&a);
        }
        s
    }

    /// A call inside an expression: `[ref.]name(arg arg)`.
    fn call_expr(&self, c: &Call) -> String {
        format!("{}({})", self.call_head(c), self.args(&c.args, Some(c.opcode)).join(" "))
    }

    pub fn expr(&self, e: &Expr) -> String {
        e.0.iter()
            .map(|t| match t {
                Token::Num { value, .. } => number(*value),
                Token::Var { var } => self.var(var),
                Token::Ref { r } => self.slot(*r),
                Token::Call { call } => self.call_expr(call),
                Token::Op { op } => op.symbol().to_string(),
                Token::X { e } => format!("eval {}", self.xexpr(e)),
                Token::Raw { bytes } => format!("?{}", hex(bytes)),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// An extended expression, every operation in parentheses:
    /// `(a := (b + 1))`, `getav(health)`, `(0x00000014.getav(luck) > 5)`.
    pub fn xexpr(&self, e: &XExpr) -> String {
        match e {
            XExpr::Num { value, .. } => number(*value),
            XExpr::Str { s } => format!("{s:?}"),
            XExpr::Var { var, .. } => self.var(var),
            XExpr::Ref { r } => self.slot(*r),
            XExpr::Enum { code } => format!("#{code}"),
            XExpr::Axis { c } => c.to_ascii_lowercase().to_string(),
            XExpr::Call { call } => {
                let head = match &call.func {
                    FuncId::Opcode(o) => self.table.get(*o).map_or_else(|| format!("#0x{o:04x}"), |f| f.name.to_ascii_lowercase()),
                    FuncId::Name(n) => n.to_ascii_lowercase(),
                };
                let on = call.on.as_ref().map(|o| format!("{}.", self.xexpr(o))).unwrap_or_default();
                format!("{on}{head}({})", call.args.iter().map(|a| self.xexpr(a)).collect::<Vec<_>>().join(" "))
            }
            XExpr::Unary { op, e } => {
                let sym = match op {
                    XUnary::Neg => "-",
                    XUnary::Stringize => "$",
                    XUnary::Numericize => "#",
                    XUnary::Unbox => "*",
                    XUnary::Box => "&",
                    XUnary::Not => "!",
                };
                format!("({sym}{})", self.xexpr(e))
            }
            XExpr::Binary { op: XOp::Subscript, l, r } => format!("{}[{}]", self.xexpr(l), self.xexpr(r)),
            XExpr::Binary { op, l, r } => format!("({} {} {})", self.xexpr(l), xop_symbol(*op), self.xexpr(r)),
            XExpr::Member { l, key } => format!("{}->{key}", self.xexpr(l)),
            XExpr::Assign { op, target, value } => format!("({} {} {})", self.xexpr(target), xassign_symbol(*op), self.xexpr(value)),
            XExpr::Lambda { params, body, value, .. } => {
                let ps: Vec<String> = params.iter().map(|p| self.var(p)).collect();
                match value {
                    Some(v) => format!("({{{}}} => {})", ps.join(" "), self.xexpr(v)),
                    None => {
                        let mut lines = Vec::new();
                        self.stmts(body, &mut lines);
                        format!("(begin function {{{}}} {} end)", ps.join(" "), lines.join("; "))
                    }
                }
            }
        }
    }

    fn with_expr(head: &str, e: String) -> String {
        if e.is_empty() {
            head.to_string()
        } else {
            format!("{head} {e}")
        }
    }

    /// The first line of statement `s` in canonical text (an `if`'s or a loop's head).
    pub fn head(&self, s: &Stmt) -> String {
        let mut out = Vec::new();
        self.stmts(std::slice::from_ref(s), &mut out);
        out.into_iter().next().unwrap_or_default().trim().to_string()
    }

    fn stmts(&self, s: &[Stmt], out: &mut Vec<String>) {
        for st in s {
            match st {
                Stmt::Set { target, expr } => out.push(format!("set {} to {}", self.var(target), self.expr(expr))),
                Stmt::If { arms, otherwise } => {
                    for (i, a) in arms.iter().enumerate() {
                        out.push(Self::with_expr(if i == 0 { "if" } else { "elseif" }, self.expr(&a.cond)));
                        self.stmts(&a.body, out);
                    }
                    if let Some(o) = otherwise {
                        out.push("else".into());
                        self.stmts(o, out);
                    }
                    out.push("endif".into());
                }
                Stmt::Return => out.push("return".into()),
                Stmt::Call { call } => out.push(self.call_stmt(call)),
                Stmt::Let { expr } => out.push(format!("let {}", self.xexpr(expr))),
                Stmt::Eval { expr } => out.push(format!("eval {}", self.xexpr(expr))),
                Stmt::While { cond, body } => {
                    out.push(format!("while {}", self.xexpr(cond)));
                    self.stmts(body, out);
                    out.push("loop".into());
                }
                Stmt::ForEach { head, body } => {
                    out.push(format!("foreach {} <- {}", self.xexpr(&head.var), self.xexpr(&head.source)));
                    self.stmts(body, out);
                    out.push("loop".into());
                }
                Stmt::Break => out.push("break".into()),
                Stmt::Label { n } => out.push(format!("label {n}")),
                Stmt::GoTo { n } => out.push(format!("goto {n}")),
                Stmt::Continue => out.push("continue".into()),
                Stmt::Unmatched { marker, cond, .. } => out.push(match marker {
                    Marker::Else => "else".into(),
                    Marker::EndIf => "endif".into(),
                    Marker::ElseIf => {
                        Self::with_expr("elseif", cond.as_ref().map(|c| self.expr(c)).unwrap_or_default())
                    }
                }),
                Stmt::Raw { opcode, body } => out.push(format!("?0x{opcode:04x} {}", hex(body)).trim_end().to_string()),
            }
        }
    }

    /// The canonical lines of a program.
    pub fn lines(&self, p: &Program) -> Vec<String> {
        let mut out = Vec::new();
        if p.script_name {
            out.push("scriptname".into());
        }
        self.stmts(&p.top, &mut out);
        for b in &p.blocks {
            let mut head = match self.table.block_name(b.event) {
                Some(n) => format!("begin {}", n.to_lowercase()),
                None if b.event == FUNCTION_BLOCK => "begin function".to_string(),
                None => format!("begin #{}", b.event),
            };
            for a in self.args(b.params.as_deref().unwrap_or(&[]), None) {
                head.push(' ');
                head.push_str(&a);
            }
            out.push(head);
            self.stmts(&b.body, &mut out);
            out.push("end".into());
        }
        out
    }
}

/// Canonical lines of `program` decoded from `unit`.
pub fn decompile(unit: &ScriptUnit, program: &Program, table: &FunctionTable, names: &dyn Names) -> Vec<String> {
    Decompiler { unit, table, names }.lines(program)
}

/// The source spelling of an extended binary operator.
pub fn xop_symbol(op: XOp) -> &'static str {
    match op {
        XOp::Or => "||",
        XOp::And => "&&",
        XOp::Slice => ":",
        XOp::Pair => "::",
        XOp::Eq => "==",
        XOp::Ne => "!=",
        XOp::Gt => ">",
        XOp::Lt => "<",
        XOp::Ge => ">=",
        XOp::Le => "<=",
        XOp::BitOr => "|",
        XOp::BitAnd => "&",
        XOp::Shl => "<<",
        XOp::Shr => ">>",
        XOp::Add => "+",
        XOp::Sub => "-",
        XOp::Mul => "*",
        XOp::Div => "/",
        XOp::Mod => "%",
        XOp::Pow => "^",
        XOp::Subscript => "[]",
    }
}

/// The source spelling of an assignment operator.
pub fn xassign_symbol(op: XAssign) -> &'static str {
    match op {
        XAssign::Set => ":=",
        XAssign::Add => "+=",
        XAssign::Sub => "-=",
        XAssign::Mul => "*=",
        XAssign::Div => "/=",
        XAssign::Pow => "^=",
        XAssign::BitOr => "|=",
        XAssign::BitAnd => "&=",
        XAssign::Mod => "%=",
    }
}
