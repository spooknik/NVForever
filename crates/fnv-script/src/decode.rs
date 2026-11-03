//! Compiled script bytes (`SCDA`) → [`Program`]. The byte layout is in
//! docs/spec-notes/scripts.md §2. Everything the bytes contain ends up in the tree; what
//! the decoder cannot explain is kept as `Raw` and counted in the [`Report`].

use crate::ir::*;
use crate::table::{FunctionTable, ParamEnc, TrailerEnc};
use serde::Serialize;
use std::collections::HashSet;

/// Statement opcodes below 0x1000 (docs/spec-notes/scripts.md §2.2).
pub mod op {
    pub const BEGIN: u16 = 0x10;
    pub const END: u16 = 0x11;
    pub const SET: u16 = 0x15;
    pub const IF: u16 = 0x16;
    pub const ELSE: u16 = 0x17;
    pub const ELSEIF: u16 = 0x18;
    pub const ENDIF: u16 = 0x19;
    pub const REF_PREFIX: u16 = 0x1C;
    pub const SCRIPT_NAME: u16 = 0x1D;
    pub const RETURN: u16 = 0x1E;
    /// Opcodes from here on are function calls.
    pub const FIRST_FUNCTION: u16 = 0x1000;
    /// NVForever's own statement for `let` (docs/spec-notes/script-compiler.md §8): the
    /// body is the extended expression as JSON. Only our compiler writes it, never into a
    /// plugin.
    pub const X_LET: u16 = 0x00F0;
    /// NVForever's own expression token: u16 length + an extended expression as JSON.
    pub const X_TOKEN: u8 = 0xF0;
    /// NVForever's own loop statements (script-compiler.md §8): `while` (body: the
    /// condition as JSON), `foreach` (body: `XForEach` as JSON), `loop`, `break`,
    /// `continue` (empty bodies).
    pub const X_WHILE: u16 = 0x00F1;
    pub const X_FOREACH: u16 = 0x00F2;
    pub const X_LOOP: u16 = 0x00F3;
    pub const X_BREAK: u16 = 0x00F4;
    pub const X_CONTINUE: u16 = 0x00F5;
    /// NVForever's own expression statement (body: the expression as JSON).
    pub const X_EVAL: u16 = 0x00F6;
    /// NVForever's own `Label n` / `GoTo n` (body: n as i32).
    pub const X_LABEL: u16 = 0x00F7;
    pub const X_GOTO: u16 = 0x00F8;
}

/// Something the decoder noticed that is not an unknown encoding: a skip count or block
/// length that does not match the structure, a stray marker, an unknown opcode.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Issue {
    pub kind: &'static str,
    /// Statement index (0-based, a `0x001C`-prefixed call counts as one statement).
    pub statement: usize,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Report {
    /// Statements, as the compiler counts them for skip counts.
    pub statements: usize,
    /// Function calls (statements and calls inside expressions).
    pub calls: usize,
    /// `Arg::Raw` parameters.
    pub raw_args: usize,
    /// `Token::Raw` expression tokens.
    pub raw_tokens: usize,
    /// `Stmt::Raw` statements.
    pub raw_statements: usize,
    pub issues: Vec<Issue>,
}

impl Report {
    /// Undecoded pieces of any kind.
    pub fn raw_total(&self) -> usize {
        self.raw_args + self.raw_tokens + self.raw_statements
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decoded {
    pub program: Program,
    pub report: Report,
}

/// Little-endian reader over a byte slice that never panics.
struct Rd<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Rd<'a> {
    fn new(b: &'a [u8]) -> Self {
        Rd { b, i: 0 }
    }
    fn left(&self) -> usize {
        self.b.len() - self.i
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn peek_at(&self, k: usize) -> Option<u8> {
        self.b.get(self.i + k).copied()
    }
    fn u8(&mut self) -> Option<u8> {
        let v = self.peek()?;
        self.i += 1;
        Some(v)
    }
    fn u16(&mut self) -> Option<u16> {
        let s = self.b.get(self.i..self.i + 2)?;
        self.i += 2;
        Some(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        let s = self.b.get(self.i..self.i + 4)?;
        self.i += 4;
        Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    fn f64(&mut self) -> Option<f64> {
        let s = self.b.get(self.i..self.i + 8)?;
        self.i += 8;
        Some(f64::from_le_bytes(s.try_into().ok()?))
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.i..self.i + n)?;
        self.i += n;
        Some(s)
    }
    fn rest(&mut self) -> &'a [u8] {
        let s = &self.b[self.i..];
        self.i = self.b.len();
        s
    }
}

/// One statement of the flat stream.
struct Flat<'a> {
    /// Byte offset just past the statement.
    end: usize,
    opcode: u16,
    on: Option<RefIndex>,
    body: &'a [u8],
}

/// Split the bytes into statements: opcode u16, length u16, body; `0x001C` + u16
/// reference index prefixes the statement that follows it. A truncated tail becomes one
/// statement with opcode 0xFFFF holding the remaining bytes.
fn split(bytes: &[u8]) -> Vec<Flat<'_>> {
    let mut out = Vec::new();
    let mut r = Rd::new(bytes);
    while r.left() > 0 {
        let start = r.i;
        let mut on = None;
        let header = |r: &mut Rd<'_>| -> Option<(u16, u16)> { Some((r.u16()?, r.u16()?)) };
        let Some((mut opcode, mut len)) = header(&mut r) else {
            r.i = start;
            out.push(Flat { end: bytes.len(), opcode: 0xFFFF, on: None, body: r.rest() });
            break;
        };
        if opcode == op::REF_PREFIX {
            on = Some(RefIndex(len));
            match header(&mut r) {
                Some((o, l)) => {
                    opcode = o;
                    len = l;
                }
                None => {
                    r.i = start;
                    out.push(Flat { end: bytes.len(), opcode: 0xFFFF, on: None, body: r.rest() });
                    break;
                }
            }
        }
        match r.take(len as usize) {
            Some(body) => out.push(Flat { end: r.i, opcode, on, body }),
            None => {
                r.i = start;
                out.push(Flat { end: bytes.len(), opcode: 0xFFFF, on: None, body: r.rest() });
                break;
            }
        }
    }
    out
}

/// A statement decoded on its own, before nesting is rebuilt.
enum Piece {
    Begin { event: u16, len: u32, params: Option<Vec<Arg>> },
    End,
    ScriptName,
    If { skip: u16, cond: Expr },
    ElseIf { skip: u16, cond: Expr },
    Else { skip: u16 },
    EndIf,
    While(XExpr),
    ForEach(XForEach),
    Loop,
    Stmt(Stmt),
}

/// An open loop while nesting is rebuilt: its head, the `If` frames open outside it, and
/// its body so far.
struct LoopFrame {
    head: Piece,
    ifs: Vec<Frame>,
    body: Vec<Stmt>,
}

impl LoopFrame {
    fn finish(self) -> Stmt {
        match self.head {
            Piece::While(cond) => Stmt::While { cond, body: self.body },
            Piece::ForEach(head) => Stmt::ForEach { head, body: self.body },
            _ => Stmt::Raw { opcode: op::X_LOOP, body: vec![] },
        }
    }
}

struct Ctx<'t> {
    table: &'t FunctionTable,
    report: Report,
    stmt: usize,
}

impl Ctx<'_> {
    fn issue(&mut self, kind: &'static str, detail: String) {
        let statement = self.stmt;
        self.report.issues.push(Issue { kind, statement, detail });
    }
}

/// A tagged value (`ParamEnc::Value`). With `allow_refvar`, `r` + u16 followed by `s`/`f`
/// is read as one remote variable; otherwise `r` + u16 alone is a reference.
fn tagged_value(r: &mut Rd<'_>, allow_refvar: bool) -> Option<Arg> {
    let t = r.peek()?;
    let save = r.i;
    let v = (|| {
        Some(match r.u8()? {
            b'n' => Arg::Int(r.i32()?),
            b'z' => Arg::Float(r.f64()?),
            b's' | b'f' => Arg::Var(VarRef {
                on: None,
                var: VarSlot::Local { index: r.u16()?, letter: VarLetter::from_byte(t)? },
            }),
            b'G' => Arg::Var(VarRef { on: None, var: VarSlot::Global { global: RefIndex(r.u16()?) } }),
            b'r' => {
                let idx = RefIndex(r.u16()?);
                if allow_refvar {
                    let letter = VarLetter::from_byte(r.u8()?)?;
                    Arg::Var(VarRef { on: Some(idx), var: VarSlot::Local { index: r.u16()?, letter } })
                } else {
                    Arg::Ref(idx)
                }
            }
            _ => return None,
        })
    })();
    if v.is_none() {
        r.i = save;
    }
    v
}

fn one_param(r: &mut Rd<'_>, enc: &ParamEnc, refvar: bool) -> Option<Arg> {
    let save = r.i;
    let v = match enc {
        ParamEnc::Value => tagged_value(r, refvar),
        ParamEnc::Enum(_) => r.u16().map(Arg::Enum),
        ParamEnc::Axis => match r.u8() {
            Some(c @ (b'X' | b'Y' | b'Z')) => Some(Arg::Axis(c as char)),
            _ => None,
        },
        ParamEnc::String => {
            let n = r.u16()?;
            r.take(n as usize).map(|s| Arg::String(fnv_util::text::decode(s)))
        }
    };
    if v.is_none() {
        r.i = save;
    }
    v
}

fn trailer_items(r: &mut Rd<'_>, trailer: &[TrailerEnc], out: &mut Vec<Arg>) -> bool {
    for t in trailer {
        match t {
            TrailerEnc::Int32 => match r.i32() {
                Some(v) => out.push(Arg::Int32(v)),
                None => return false,
            },
            TrailerEnc::FormatVars => {
                let Some(n) = r.u16() else { return false };
                let mut vars = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    // A format variable is a value; `r` + u16 + `s`/`f` is a remote
                    // variable, `r` + u16 alone a reference.
                    let v = tagged_value(r, true).or_else(|| tagged_value(r, false));
                    match v {
                        Some(v) => vars.push(v),
                        None => return false,
                    }
                }
                out.push(Arg::FormatVars(vars));
            }
        }
    }
    true
}

/// Parameters `k..count` from `r`, then the trailer; true when every byte is used.
/// `r` + u16 + `s`/`f` in a value position is either one remote variable or a reference
/// followed by a local variable; both readings are tried (remote variable first) and the
/// one that uses the bytes exactly with the stored parameter count wins.
///
/// Whether parameters `k..count` can use exactly the bytes from `r.i` to the end depends
/// only on `(r.i, k)`, so `failed` remembers the states that cannot: without it a block
/// with many ambiguous spots that never comes out exact would try every combination
/// (2^spots); with it the work is bounded by block length × count.
fn params_from(
    r: &mut Rd<'_>,
    table: &FunctionTable,
    opcode: Option<u16>,
    k: usize,
    count: usize,
    out: &mut Vec<Arg>,
    failed: &mut HashSet<(usize, usize)>,
) -> bool {
    if failed.contains(&(r.i, k)) {
        return false;
    }
    if k == count {
        let save = (r.i, out.len());
        let trailer = opcode.map(|o| table.trailer(o)).unwrap_or(&[]);
        if trailer_items(r, trailer, out) && r.left() == 0 {
            return true;
        }
        r.i = save.0;
        out.truncate(save.1);
        return false;
    }
    let enc = opcode.map(|o| table.param_enc(o, k)).unwrap_or(ParamEnc::Value);
    let tries: &[bool] = if enc == ParamEnc::Value && r.peek() == Some(b'r') && matches!(r.peek_at(3), Some(b's' | b'f'))
    {
        &[true, false]
    } else {
        &[false]
    };
    let at = r.i;
    for &refvar in tries {
        let save = (r.i, out.len());
        if let Some(a) = one_param(r, &enc, refvar) {
            out.push(a);
            if params_from(r, table, opcode, k + 1, count, out, failed) {
                return true;
            }
        }
        r.i = save.0;
        out.truncate(save.1);
    }
    failed.insert((at, k));
    false
}

/// A parameter block: empty (no parameters at all) or u16 count + parameters (+ trailer).
/// `opcode` None = event parameters of a Begin block (every one a tagged value).
fn params(c: &mut Ctx<'_>, blob: &[u8], opcode: Option<u16>) -> Option<Vec<Arg>> {
    if blob.is_empty() {
        return None;
    }
    let mut r = Rd::new(blob);
    let Some(count) = r.u16() else {
        c.report.raw_args += 1;
        c.issue("raw_params", format!("parameter block of {} byte(s) too short for a count", blob.len()));
        return Some(vec![Arg::Raw(blob.to_vec())]);
    };
    let mut out = Vec::new();
    if params_from(&mut r, c.table, opcode, 0, count as usize, &mut out, &mut HashSet::new()) {
        return Some(out);
    }
    // No reading uses the bytes exactly: keep what reads greedily, then the rest raw.
    let mut r = Rd::new(blob);
    r.i = 2;
    let mut out = Vec::new();
    for k in 0..count as usize {
        let enc = opcode.map(|o| c.table.param_enc(o, k)).unwrap_or(ParamEnc::Value);
        match one_param(&mut r, &enc, false) {
            Some(a) => out.push(a),
            None => break,
        }
    }
    let rest = r.rest();
    let name = opcode.map(|o| format!("0x{o:04X}")).unwrap_or_else(|| "event".into());
    c.issue("raw_params", format!("{name}: {count} parameter(s), undecoded tail {}", fnv_util::sha256::to_hex(rest)));
    c.report.raw_args += 1;
    out.push(Arg::Raw(rest.to_vec()));
    Some(out)
}

fn call(c: &mut Ctx<'_>, on: Option<RefIndex>, opcode: u16, blob: &[u8]) -> Call {
    c.report.calls += 1;
    if c.table.get(opcode).is_none() {
        c.issue("unknown_opcode", format!("0x{opcode:04X}"));
    }
    let args = params(c, blob, Some(opcode));
    Call { on, opcode, has_params: args.is_some(), args: args.unwrap_or_default() }
}

/// Postfix expression: each token is preceded by a space (0x20).
fn expr(c: &mut Ctx<'_>, bytes: &[u8]) -> Expr {
    let mut out = Vec::new();
    let mut r = Rd::new(bytes);
    while r.left() > 0 {
        let start = r.i;
        let tok = (|| -> Option<Token> {
            if r.u8()? != b' ' {
                return None;
            }
            let mut on = None;
            if r.peek()? == b'r' {
                r.u8();
                on = Some(RefIndex(r.u16()?));
            }
            let t = r.peek()?;
            Some(match t {
                b's' | b'f' => {
                    r.u8();
                    let index = r.u16()?;
                    Token::Var { var: VarRef { on, var: VarSlot::Local { index, letter: VarLetter::from_byte(t)? } } }
                }
                b'G' if on.is_none() => {
                    r.u8();
                    Token::Var { var: VarRef { on: None, var: VarSlot::Global { global: RefIndex(r.u16()?) } } }
                }
                b'Z' if on.is_none() => {
                    r.u8();
                    Token::Ref { r: RefIndex(r.u16()?) }
                }
                op::X_TOKEN if on.is_none() => {
                    r.u8();
                    let n = r.u16()? as usize;
                    let e = serde_json::from_slice(r.take(n)?).ok()?;
                    Token::X { e }
                }
                b'X' => {
                    r.u8();
                    let opcode = r.u16()?;
                    let len = r.u16()?;
                    let blob = r.take(len as usize)?;
                    Token::Call { call: call(c, on, opcode, blob) }
                }
                _ if on.is_none() => {
                    let s = r.i;
                    while r.peek().is_some_and(|b| b != b' ') {
                        r.i += 1;
                    }
                    let text = std::str::from_utf8(&r.b[s..r.i]).ok()?;
                    if let Some(op) = Op::parse(text) {
                        Token::Op { op }
                    } else if is_number(text) {
                        Token::Num { value: text.parse().ok()?, text: text.to_string() }
                    } else {
                        return None;
                    }
                }
                _ => return None,
            })
        })();
        match tok {
            Some(t) => out.push(t),
            None => {
                r.i = start;
                let rest = r.rest();
                c.report.raw_tokens += 1;
                c.issue("raw_token", fnv_util::sha256::to_hex(rest));
                out.push(Token::Raw { bytes: rest.to_vec() });
            }
        }
    }
    Expr(out)
}

/// ASCII number literal as the compiler stores it: digits with at most one '.'.
fn is_number(s: &str) -> bool {
    let mut dot = false;
    let mut digit = false;
    for ch in s.chars() {
        match ch {
            '0'..='9' => digit = true,
            '.' if !dot => dot = true,
            _ => return false,
        }
    }
    digit
}

/// `Set` target: `s`/`f` + u16, `G` + u16, or `r` + u16 + `s`/`f` + u16.
fn set_target(r: &mut Rd<'_>) -> Option<VarRef> {
    let mut on = None;
    if r.peek()? == b'r' {
        r.u8();
        on = Some(RefIndex(r.u16()?));
    }
    let t = r.u8()?;
    Some(match t {
        b's' | b'f' => VarRef { on, var: VarSlot::Local { index: r.u16()?, letter: VarLetter::from_byte(t)? } },
        b'G' if on.is_none() => VarRef { on: None, var: VarSlot::Global { global: RefIndex(r.u16()?) } },
        _ => return None,
    })
}

fn piece(c: &mut Ctx<'_>, f: &Flat<'_>) -> Piece {
    let raw = |c: &mut Ctx<'_>, why: &str| {
        c.report.raw_statements += 1;
        c.issue("raw_statement", format!("0x{:04X}: {why}", f.opcode));
        Piece::Stmt(Stmt::Raw { opcode: f.opcode, body: f.body.to_vec() })
    };
    if f.opcode >= op::FIRST_FUNCTION && f.opcode != 0xFFFF {
        return Piece::Stmt(Stmt::Call { call: call(c, f.on, f.opcode, f.body) });
    }
    if f.on.is_some() {
        return raw(c, "reference prefix before a statement that is not a call");
    }
    let mut r = Rd::new(f.body);
    let empty = f.body.is_empty();
    match f.opcode {
        op::BEGIN => {
            let (Some(event), Some(len)) = (r.u16(), r.u32()) else { return raw(c, "short Begin") };
            let rest = r.rest();
            let params = params(c, rest, None);
            Piece::Begin { event, len, params }
        }
        op::END if empty => Piece::End,
        op::SCRIPT_NAME if empty => Piece::ScriptName,
        op::RETURN if empty => Piece::Stmt(Stmt::Return),
        op::X_LET => match serde_json::from_slice(f.body) {
            Ok(expr) => Piece::Stmt(Stmt::Let { expr }),
            Err(_) => raw(c, "unreadable extended statement"),
        },
        op::X_WHILE => match serde_json::from_slice(f.body) {
            Ok(e) => Piece::While(e),
            Err(_) => raw(c, "unreadable extended statement"),
        },
        op::X_FOREACH => match serde_json::from_slice(f.body) {
            Ok(h) => Piece::ForEach(h),
            Err(_) => raw(c, "unreadable extended statement"),
        },
        op::X_EVAL => match serde_json::from_slice(f.body) {
            Ok(expr) => Piece::Stmt(Stmt::Eval { expr }),
            Err(_) => raw(c, "unreadable extended statement"),
        },
        op::X_LABEL | op::X_GOTO if f.body.len() == 4 => {
            let n = i32::from_le_bytes([f.body[0], f.body[1], f.body[2], f.body[3]]);
            Piece::Stmt(if f.opcode == op::X_LABEL { Stmt::Label { n } } else { Stmt::GoTo { n } })
        }
        op::X_LOOP if empty => Piece::Loop,
        op::X_BREAK if empty => Piece::Stmt(Stmt::Break),
        op::X_CONTINUE if empty => Piece::Stmt(Stmt::Continue),
        op::ENDIF if empty => Piece::EndIf,
        op::ELSE if f.body.len() == 2 => Piece::Else { skip: r.u16().unwrap_or_default() },
        op::IF | op::ELSEIF => {
            let (Some(skip), Some(len)) = (r.u16(), r.u16()) else { return raw(c, "short If") };
            if r.left() != len as usize {
                return raw(c, "expression length does not match the body");
            }
            let cond = expr(c, r.rest());
            if f.opcode == op::IF {
                Piece::If { skip, cond }
            } else {
                Piece::ElseIf { skip, cond }
            }
        }
        op::SET => {
            let Some(target) = set_target(&mut r) else { return raw(c, "unknown Set target") };
            let Some(len) = r.u16() else { return raw(c, "short Set") };
            if r.left() != len as usize {
                return raw(c, "expression length does not match the body");
            }
            let e = expr(c, r.rest());
            Piece::Stmt(Stmt::Set { target, expr: e })
        }
        0xFFFF => raw(c, "truncated statement"),
        _ => raw(c, "unknown statement opcode or body"),
    }
}

/// An open `If` while nesting is rebuilt.
struct Frame {
    arms: Vec<Arm>,
    otherwise: Option<Vec<Stmt>>,
    /// Statement index and stored skip count of the last marker (If/ElseIf/Else).
    marker: (usize, u16),
}

impl Frame {
    fn current(&mut self) -> &mut Vec<Stmt> {
        match &mut self.otherwise {
            Some(o) => o,
            None => &mut self.arms.last_mut().expect("frame has an arm").body,
        }
    }
    fn finish(self) -> Stmt {
        Stmt::If { arms: self.arms, otherwise: self.otherwise }
    }
}

/// Decode compiled script bytes.
pub fn decode(bytes: &[u8], table: &FunctionTable) -> Decoded {
    let flats = split(bytes);
    let mut c = Ctx { table, report: Report::default(), stmt: 0 };
    let mut prog = Program::default();
    let mut block: Option<(Block, usize, u32, usize)> = None; // block, start stmt, stored len, byte offset after Begin
    let mut stack: Vec<Frame> = Vec::new();
    let mut loops: Vec<LoopFrame> = Vec::new();

    // Close every open If into the current sink (end of a block or of the unit).
    fn close_ifs(stack: &mut Vec<Frame>, sink: &mut Vec<Stmt>) {
        while let Some(f) = stack.pop() {
            let s = f.finish();
            match stack.last_mut() {
                Some(p) => p.current().push(s),
                None => sink.push(s),
            }
        }
    }
    fn push(stack: &mut [Frame], loops: &mut [LoopFrame], block: &mut Option<(Block, usize, u32, usize)>, top: &mut Vec<Stmt>, s: Stmt) {
        match (stack.last_mut(), loops.last_mut()) {
            (Some(f), _) => f.current().push(s),
            (None, Some(l)) => l.body.push(s),
            (None, None) => match block {
                Some(b) => b.0.body.push(s),
                None => top.push(s),
            },
        }
    }
    // Close every open loop (a block or the unit ends inside one) into where it stands.
    fn close_loops(stack: &mut Vec<Frame>, loops: &mut Vec<LoopFrame>, sink: &mut Vec<Stmt>) {
        while let Some(mut l) = loops.pop() {
            close_ifs(stack, &mut l.body);
            *stack = std::mem::take(&mut l.ifs);
            let s = l.finish();
            match (stack.last_mut(), loops.last_mut()) {
                (Some(f), _) => f.current().push(s),
                (None, Some(o)) => o.body.push(s),
                (None, None) => sink.push(s),
            }
        }
    }
    let check_skip = |c: &mut Ctx<'_>, marker: (usize, u16), at: usize| {
        let expected = at - marker.0 - 1;
        if marker.1 as usize != expected {
            c.report.issues.push(Issue {
                kind: "skip_mismatch",
                statement: marker.0,
                detail: format!("stored {} statement(s), the next marker is {} statement(s) on", marker.1, expected),
            });
        }
    };

    for (i, f) in flats.iter().enumerate() {
        c.stmt = i;
        let p = piece(&mut c, f);
        match p {
            Piece::ScriptName => {
                if i != 0 || prog.script_name {
                    c.issue("misplaced_scriptname", String::new());
                    push(&mut stack, &mut loops, &mut block, &mut prog.top, Stmt::Raw { opcode: op::SCRIPT_NAME, body: vec![] });
                    c.report.raw_statements += 1;
                } else {
                    prog.script_name = true;
                }
            }
            Piece::Begin { event, len, params } => {
                if let Some((b, ..)) = block.take() {
                    c.issue("begin_inside_block", String::new());
                    let mut b = b;
                    close_loops(&mut stack, &mut loops, &mut b.body);
                    close_ifs(&mut stack, &mut b.body);
                    prog.blocks.push(b);
                } else if !stack.is_empty() {
                    c.issue("if_open_at_begin", String::new());
                    close_ifs(&mut stack, &mut prog.top);
                }
                if !prog.top.is_empty() {
                    c.issue("statements_outside_blocks", format!("{} statement(s) before this block", prog.top.len()));
                }
                block = Some((Block { event, params, body: Vec::new() }, i, len, f.end));
            }
            Piece::End => match block.take() {
                Some((mut b, _start, len, after_begin)) => {
                    if !loops.is_empty() {
                        c.issue("unterminated_loop", String::new());
                        close_loops(&mut stack, &mut loops, &mut b.body);
                    }
                    if !stack.is_empty() {
                        c.issue("if_open_at_end", String::new());
                        close_ifs(&mut stack, &mut b.body);
                    }
                    let actual = f.end - after_begin;
                    if actual != len as usize {
                        c.issue("block_length_mismatch", format!("stored {len}, actual {actual}"));
                    }
                    prog.blocks.push(b);
                }
                None => {
                    c.issue("end_without_begin", String::new());
                    c.report.raw_statements += 1;
                    push(&mut stack, &mut loops, &mut block, &mut prog.top, Stmt::Raw { opcode: op::END, body: vec![] });
                }
            },
            Piece::If { skip, cond } => {
                stack.push(Frame { arms: vec![Arm { cond, body: Vec::new() }], otherwise: None, marker: (i, skip) });
            }
            Piece::ElseIf { skip, cond } => match stack.last_mut() {
                Some(fr) if fr.otherwise.is_none() => {
                    let m = fr.marker;
                    fr.arms.push(Arm { cond, body: Vec::new() });
                    fr.marker = (i, skip);
                    check_skip(&mut c, m, i);
                }
                // After the chain's Else: the compiler still chains skip counts through
                // it; it stays in the Else body as a continuation marker.
                Some(fr) => {
                    let m = fr.marker;
                    fr.marker = (i, skip);
                    fr.current().push(Stmt::Unmatched { marker: Marker::ElseIf, skip: Some(skip), cond: Some(cond) });
                    check_skip(&mut c, m, i);
                    c.issue("chain_after_else", "ElseIf".into());
                }
                None => {
                    c.issue("unmatched_elseif", String::new());
                    let s = Stmt::Unmatched { marker: Marker::ElseIf, skip: Some(skip), cond: Some(cond) };
                    push(&mut stack, &mut loops, &mut block, &mut prog.top, s);
                }
            },
            Piece::Else { skip } => match stack.last_mut() {
                Some(fr) if fr.otherwise.is_none() => {
                    let m = fr.marker;
                    fr.otherwise = Some(Vec::new());
                    fr.marker = (i, skip);
                    check_skip(&mut c, m, i);
                }
                Some(fr) => {
                    let m = fr.marker;
                    fr.marker = (i, skip);
                    fr.current().push(Stmt::Unmatched { marker: Marker::Else, skip: Some(skip), cond: None });
                    check_skip(&mut c, m, i);
                    c.issue("chain_after_else", "Else".into());
                }
                None => {
                    c.issue("unmatched_else", String::new());
                    let s = Stmt::Unmatched { marker: Marker::Else, skip: Some(skip), cond: None };
                    push(&mut stack, &mut loops, &mut block, &mut prog.top, s);
                }
            },
            Piece::EndIf => match stack.pop() {
                Some(fr) => {
                    check_skip(&mut c, fr.marker, i);
                    let s = fr.finish();
                    push(&mut stack, &mut loops, &mut block, &mut prog.top, s);
                }
                None => {
                    c.issue("unmatched_endif", String::new());
                    let s = Stmt::Unmatched { marker: Marker::EndIf, skip: None, cond: None };
                    push(&mut stack, &mut loops, &mut block, &mut prog.top, s);
                }
            },
            Piece::While(_) | Piece::ForEach(_) => {
                loops.push(LoopFrame { head: p, ifs: std::mem::take(&mut stack), body: Vec::new() });
            }
            Piece::Loop => match loops.pop() {
                Some(mut l) => {
                    if !stack.is_empty() {
                        c.issue("if_open_at_loop", String::new());
                        close_ifs(&mut stack, &mut l.body);
                    }
                    stack = std::mem::take(&mut l.ifs);
                    let s = l.finish();
                    push(&mut stack, &mut loops, &mut block, &mut prog.top, s);
                }
                None => {
                    c.issue("loop_without_while", String::new());
                    c.report.raw_statements += 1;
                    push(&mut stack, &mut loops, &mut block, &mut prog.top, Stmt::Raw { opcode: op::X_LOOP, body: vec![] });
                }
            },
            Piece::Stmt(s) => push(&mut stack, &mut loops, &mut block, &mut prog.top, s),
        }
    }
    c.stmt = flats.len();
    match block.take() {
        Some((mut b, ..)) => {
            c.issue("unterminated_block", String::new());
            close_loops(&mut stack, &mut loops, &mut b.body);
            close_ifs(&mut stack, &mut b.body);
            prog.blocks.push(b);
        }
        None => {
            if !loops.is_empty() {
                c.issue("unterminated_loop", String::new());
            }
            close_loops(&mut stack, &mut loops, &mut prog.top);
            if !stack.is_empty() {
                c.issue("unterminated_if", String::new());
            }
            close_ifs(&mut stack, &mut prog.top);
        }
    }
    c.report.statements = flats.len();
    Decoded { program: prog, report: c.report }
}
