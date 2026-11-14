//! NVSEScript, the extender's second script language (xNVSE 6.3.6 on) [documented:
//! NVSEScript; Lambda; User Defined Function]: `name X;`, C-style calls `F(a, b);`, blocks
//! in braces, `fn (int a) { … }` functions, `for`/`for … in`/`while`/`match`, array and
//! map literals and `${…}` in strings (script-compiler.md §8.2).
//!
//! The editor compiles it to bytes of its own that NVForever does not read (§8: the engine
//! writes the extender's statements in a form of its own). So the unit is written out in
//! the extender's first grammar, which the compiler builds, and compiled from that. Every
//! line written keeps the number of the source line it came from, so a diagnostic names
//! the source's line.

use super::Diag;

/// A unit's source in the first grammar, and the source line of each of its lines.
pub(super) struct Translated {
    pub text: String,
    pub lines: Vec<usize>,
    pub diags: Vec<Diag>,
}

/// Whether `src` is NVSEScript: its first statement is `name X;` [documented: NVSEScript,
/// "Beginning a script": the `name` statement tells the editor which compiler to use].
pub(super) fn is_nvsescript(src: &str) -> bool {
    for line in src.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with("//") {
            continue;
        }
        let mut w = l.split_whitespace();
        return w.next().is_some_and(|k| k.eq_ignore_ascii_case("name")) && w.next().is_some_and(|n| n.ends_with(';') || l.contains(';'));
    }
    false
}

/// The unit written in the first grammar, or the first error.
pub(super) fn translate(src: &str) -> Result<Translated, Diag> {
    let toks = lex(src, 1)?;
    let mut p = Parser { t: toks, i: 0, pattern: false };
    let items = p.root()?;
    let mut o = Out::default();
    o.root(&items);
    Ok(o.finish())
}

fn err(line: usize, msg: impl Into<String>) -> Diag {
    Diag { line, kind: "syntax", error: true, detail: msg.into() }
}

// ---------------------------------------------------------------- tokens

#[derive(Debug, Clone, PartialEq)]
enum T {
    Word(String),
    Num(String),
    Str(Vec<SPart>),
    Op(String),
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBrack,
    RBrack,
    Semi,
    Comma,
}

#[derive(Debug, Clone, PartialEq)]
enum SPart {
    Lit(String),
    /// `${expr}` [documented: NVSEScript, Interpolation].
    Expr(Vec<Tk>),
}

#[derive(Debug, Clone, PartialEq)]
struct Tk {
    t: T,
    line: usize,
}

const OPS: [&str; 39] = [
    "::", "->", "==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "+=", "-=", "*=", "/=", "%=", "^=", "|=", "&=", "++", "--", ":=", "+", "-", "*", "/", "%", "^", "<",
    ">", "=", "!", "$", "#", "&", "|", ":", "?", "~", ".",
];

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn lex(src: &str, first_line: usize) -> Result<Vec<Tk>, Diag> {
    let c: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = first_line;
    while i < c.len() {
        let ch = c[i];
        if ch == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        // `//` to the end of the line [documented: NVSEScript, Comments].
        if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        let push = |out: &mut Vec<Tk>, t: T| out.push(Tk { t, line });
        match ch {
            '(' => push(&mut out, T::LParen),
            ')' => push(&mut out, T::RParen),
            '{' => push(&mut out, T::LBrace),
            '}' => push(&mut out, T::RBrace),
            '[' => push(&mut out, T::LBrack),
            ']' => push(&mut out, T::RBrack),
            ';' => push(&mut out, T::Semi),
            ',' => push(&mut out, T::Comma),
            '"' => {
                let start = line;
                let mut parts = Vec::new();
                let mut lit = String::new();
                i += 1;
                loop {
                    let Some(&d) = c.get(i) else { return Err(err(start, "string without its closing quote")) };
                    match d {
                        '"' => break,
                        '\n' => return Err(err(start, "string without its closing quote")),
                        // Escapes [hypothesis: the C ones; the page shows `\n` in no example,
                        // the mod lists' scripts write it].
                        '\\' => {
                            let e = c.get(i + 1).copied().unwrap_or('\\');
                            lit.push(match e {
                                'n' => '\n',
                                't' => '\t',
                                'r' => '\r',
                                other => other,
                            });
                            i += 2;
                            continue;
                        }
                        '$' if c.get(i + 1) == Some(&'{') => {
                            let mut depth = 0;
                            let mut j = i + 1;
                            loop {
                                match c.get(j) {
                                    None => return Err(err(start, "`${` without its `}`")),
                                    Some('{') => depth += 1,
                                    Some('}') => {
                                        depth -= 1;
                                        if depth == 0 {
                                            break;
                                        }
                                    }
                                    _ => {}
                                }
                                j += 1;
                            }
                            let inner: String = c[i + 2..j].iter().collect();
                            if !lit.is_empty() {
                                parts.push(SPart::Lit(std::mem::take(&mut lit)));
                            }
                            parts.push(SPart::Expr(lex(&inner, line)?));
                            i = j + 1;
                            continue;
                        }
                        other => lit.push(other),
                    }
                    i += 1;
                }
                if !lit.is_empty() || parts.is_empty() {
                    parts.push(SPart::Lit(lit));
                }
                push(&mut out, T::Str(parts));
            }
            _ if word_char(ch) => {
                let mut j = i;
                // Words take `.` between their parts (`player.GetItemCount`, `Quest.var`,
                // `1.5`), as the first grammar's do.
                while j < c.len() && (word_char(c[j]) || c[j] == '.' && c.get(j + 1).is_some_and(|n| word_char(*n))) {
                    j += 1;
                }
                let w: String = c[i..j].iter().collect();
                let num = w.chars().all(|x| x.is_ascii_digit() || x == '.') || w.len() > 2 && w[..2].eq_ignore_ascii_case("0x") && w[2..].chars().all(|x| x.is_ascii_hexdigit());
                push(&mut out, if num { T::Num(w) } else { T::Word(w) });
                i = j;
                continue;
            }
            _ => {
                let rest: String = c[i..(i + 2).min(c.len())].iter().collect();
                let Some(op) = OPS.iter().find(|o| rest.starts_with(*o)) else { return Err(err(line, format!("unexpected {ch}"))) };
                push(&mut out, T::Op(op.to_string()));
                i += op.chars().count();
                continue;
            }
        }
        i += 1;
    }
    Ok(out)
}

// ---------------------------------------------------------------- tree

#[derive(Debug, Clone)]
enum E {
    Num(String),
    Str(Vec<SP>),
    Name(String),
    /// `callee(args)`; `on` for `expr.F(args)` on a value that is not a name.
    Call { name: String, on: Option<Box<E>>, args: Vec<E> },
    Index(Box<E>, Box<E>),
    Member(Box<E>, String),
    Unary(String, Box<E>),
    Postfix(String, Box<E>),
    Bin(String, Box<E>, Box<E>),
    Assign(String, Box<E>, Box<E>),
    List(Vec<E>),
    Map(Vec<E>),
    Lambda { params: Vec<(String, String)>, body: LBody },
}

#[derive(Debug, Clone)]
enum SP {
    Lit(String),
    E(E),
}

#[derive(Debug, Clone)]
enum LBody {
    Block(Vec<S>),
    Expr(Box<E>),
}

#[derive(Debug, Clone)]
struct S {
    line: usize,
    k: SK,
}

#[derive(Debug, Clone)]
enum SK {
    Decl(String, Vars),
    Expr(E),
    If(Vec<(E, Vec<S>)>, Option<Vec<S>>),
    While(E, Vec<S>),
    For(Option<Box<S>>, Option<E>, Option<E>, Vec<S>),
    /// `for (array a in e)`, or the destructure form `[T k, T v]` / `[_, T v]` / `[T v]`.
    ForIn(ForVar, E, Vec<S>),
    Match(E, Vec<(Option<E>, Vec<S>)>),
    Return(Option<E>),
    Break,
    Continue,
    Block(Vec<S>),
}

#[derive(Debug, Clone)]
enum ForVar {
    Plain(String, String),
    Destructure(Vec<Option<(String, String)>>),
}

/// Declared names and their values.
type Vars = Vec<(String, Option<E>)>;

enum Item {
    Name(String),
    Decl(usize, String, Vars),
    Fn(usize, Vec<(String, String)>, Vec<S>),
    Event(usize, String, Option<String>, Vec<S>),
}

/// The variable types [documented: NVSEScript, Variable types; `short`/`long` as in the
/// first grammar, hypothesis].
const TYPES: [&str; 8] = ["int", "float", "double", "ref", "string", "array", "short", "long"];

fn is_type(w: &str) -> bool {
    TYPES.iter().any(|t| t.eq_ignore_ascii_case(w))
}

struct Parser {
    t: Vec<Tk>,
    i: usize,
    /// Parsing a match arm's pattern, where `->` ends it.
    pattern: bool,
}

impl Parser {
    fn peek(&self) -> Option<&T> {
        self.t.get(self.i).map(|t| &t.t)
    }
    fn peek_at(&self, k: usize) -> Option<&T> {
        self.t.get(self.i + k).map(|t| &t.t)
    }
    fn line(&self) -> usize {
        self.t.get(self.i).or(self.t.last()).map_or(1, |t| t.line)
    }
    fn next(&mut self) -> Option<T> {
        let t = self.t.get(self.i).map(|t| t.t.clone());
        self.i += 1;
        t
    }
    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(T::Word(x)) if x.eq_ignore_ascii_case(w))
    }
    fn is_op(&self, o: &str) -> bool {
        matches!(self.peek(), Some(T::Op(x)) if x == o)
    }
    fn expect(&mut self, t: T, what: &str) -> Result<(), Diag> {
        if self.peek() == Some(&t) {
            self.i += 1;
            Ok(())
        } else {
            Err(err(self.line(), format!("expected {what}")))
        }
    }
    fn word(&mut self) -> Result<String, Diag> {
        match self.next() {
            Some(T::Word(w)) => Ok(w),
            _ => {
                self.i -= 1;
                Err(err(self.line(), "expected a name"))
            }
        }
    }

    fn root(&mut self) -> Result<Vec<Item>, Diag> {
        let mut items = Vec::new();
        while self.peek().is_some() {
            let line = self.line();
            if self.is_word("name") && matches!(self.peek_at(2), Some(T::Semi)) {
                self.i += 1;
                items.push(Item::Name(self.word()?));
                self.expect(T::Semi, ";")?;
            } else if self.is_word("fn") {
                self.i += 1;
                let params = self.params()?;
                items.push(Item::Fn(line, params, self.block()?));
            } else if matches!(self.peek(), Some(T::Word(w)) if is_type(w)) {
                let (ty, vars) = self.decl()?;
                self.expect(T::Semi, ";")?;
                items.push(Item::Decl(line, ty, vars));
            } else if matches!(self.peek(), Some(T::Word(_))) {
                // `GameMode { … }`, `OnEquip::Player { … }` [documented: NVSEScript, Event
                // blocks].
                let name = self.word()?;
                let arg = if self.is_op("::") {
                    self.i += 1;
                    match self.next() {
                        Some(T::Word(w) | T::Num(w)) => Some(w),
                        _ => return Err(err(line, "expected an event block's argument")),
                    }
                } else {
                    None
                };
                items.push(Item::Event(line, name, arg, self.block()?));
            } else {
                return Err(err(line, "expected a declaration, a function or an event block"));
            }
        }
        Ok(items)
    }

    /// `(int a, ref b)`.
    fn params(&mut self) -> Result<Vec<(String, String)>, Diag> {
        self.expect(T::LParen, "(")?;
        let mut ps = Vec::new();
        while self.peek() != Some(&T::RParen) {
            let ty = self.word()?;
            if !is_type(&ty) {
                return Err(err(self.line(), format!("{ty} is not a variable type")));
            }
            ps.push((ty, self.word()?));
            if self.peek() == Some(&T::Comma) {
                self.i += 1;
            }
        }
        self.i += 1;
        Ok(ps)
    }

    fn decl(&mut self) -> Result<(String, Vars), Diag> {
        let ty = self.word()?;
        let mut vars = Vec::new();
        loop {
            let n = self.word()?;
            let init = if self.is_op("=") {
                self.i += 1;
                Some(self.expr(0)?)
            } else {
                None
            };
            vars.push((n, init));
            if self.peek() == Some(&T::Comma) {
                self.i += 1;
            } else {
                break;
            }
        }
        Ok((ty, vars))
    }

    fn block(&mut self) -> Result<Vec<S>, Diag> {
        self.expect(T::LBrace, "{")?;
        let mut v = Vec::new();
        while self.peek() != Some(&T::RBrace) {
            if self.peek().is_none() {
                return Err(err(self.line(), "block without its }"));
            }
            v.push(self.stmt()?);
        }
        self.i += 1;
        Ok(v)
    }

    /// A block, or one statement.
    fn body(&mut self) -> Result<Vec<S>, Diag> {
        if self.peek() == Some(&T::LBrace) {
            self.block()
        } else {
            Ok(vec![self.stmt()?])
        }
    }

    fn paren_expr(&mut self) -> Result<E, Diag> {
        self.expect(T::LParen, "(")?;
        let e = self.expr(0)?;
        self.expect(T::RParen, ")")?;
        Ok(e)
    }

    fn stmt(&mut self) -> Result<S, Diag> {
        let line = self.line();
        let k = if self.peek() == Some(&T::LBrace) {
            SK::Block(self.block()?)
        } else if self.peek() == Some(&T::Semi) {
            self.i += 1;
            SK::Block(Vec::new())
        } else if matches!(self.peek(), Some(T::Word(w)) if is_type(w)) && matches!(self.peek_at(1), Some(T::Word(_))) {
            let (ty, vars) = self.decl()?;
            self.expect(T::Semi, ";")?;
            SK::Decl(ty, vars)
        } else if self.is_word("if") {
            self.i += 1;
            let mut arms = vec![(self.paren_expr()?, self.body()?)];
            let mut els = None;
            while self.is_word("else") {
                self.i += 1;
                if self.is_word("if") {
                    self.i += 1;
                    arms.push((self.paren_expr()?, self.body()?));
                } else {
                    els = Some(self.body()?);
                    break;
                }
            }
            SK::If(arms, els)
        } else if self.is_word("while") {
            self.i += 1;
            let c = self.paren_expr()?;
            SK::While(c, self.body()?)
        } else if self.is_word("for") {
            self.i += 1;
            self.for_stmt()?
        } else if self.is_word("match") {
            self.i += 1;
            let subj = self.paren_expr()?;
            self.expect(T::LBrace, "{")?;
            let mut arms = Vec::new();
            while self.peek() != Some(&T::RBrace) {
                let pat = if self.is_word("_") {
                    self.i += 1;
                    None
                } else {
                    self.pattern = true;
                    let e = self.expr(0);
                    self.pattern = false;
                    Some(e?)
                };
                if !self.is_op("->") {
                    return Err(err(self.line(), "expected -> in a match arm"));
                }
                self.i += 1;
                arms.push((pat, self.body()?));
                if self.peek() == Some(&T::Comma) {
                    self.i += 1;
                }
            }
            self.i += 1;
            SK::Match(subj, arms)
        } else if self.is_word("return") {
            self.i += 1;
            let e = if self.peek() == Some(&T::Semi) { None } else { Some(self.expr(0)?) };
            self.expect(T::Semi, ";")?;
            SK::Return(e)
        } else if self.is_word("break") || self.is_word("continue") {
            let b = self.is_word("break");
            self.i += 1;
            self.expect(T::Semi, ";")?;
            if b {
                SK::Break
            } else {
                SK::Continue
            }
        } else {
            let e = self.expr(0)?;
            self.expect(T::Semi, ";")?;
            SK::Expr(e)
        };
        Ok(S { line, k })
    }

    fn for_stmt(&mut self) -> Result<SK, Diag> {
        self.expect(T::LParen, "(")?;
        // `for ([int e] in arr)`, `for ([string k, int v] in m)`, `for ([_, int v] in m)`.
        if self.peek() == Some(&T::LBrack) {
            self.i += 1;
            let mut vars = Vec::new();
            while self.peek() != Some(&T::RBrack) {
                if self.is_word("_") {
                    self.i += 1;
                    vars.push(None);
                } else {
                    let ty = self.word()?;
                    vars.push(Some((ty, self.word()?)));
                }
                if self.peek() == Some(&T::Comma) {
                    self.i += 1;
                }
            }
            self.i += 1;
            if !self.is_word("in") {
                return Err(err(self.line(), "expected in"));
            }
            self.i += 1;
            let src = self.expr(0)?;
            self.expect(T::RParen, ")")?;
            return Ok(SK::ForIn(ForVar::Destructure(vars), src, self.body()?));
        }
        if matches!(self.peek(), Some(T::Word(w)) if is_type(w)) && matches!(self.peek_at(2), Some(T::Word(w)) if w.eq_ignore_ascii_case("in")) {
            let ty = self.word()?;
            let n = self.word()?;
            self.i += 1;
            let src = self.expr(0)?;
            self.expect(T::RParen, ")")?;
            return Ok(SK::ForIn(ForVar::Plain(ty, n), src, self.body()?));
        }
        let line = self.line();
        let init = if self.peek() == Some(&T::Semi) {
            None
        } else if matches!(self.peek(), Some(T::Word(w)) if is_type(w)) {
            let (ty, vars) = self.decl()?;
            Some(Box::new(S { line, k: SK::Decl(ty, vars) }))
        } else {
            Some(Box::new(S { line, k: SK::Expr(self.expr(0)?) }))
        };
        self.expect(T::Semi, ";")?;
        let cond = if self.peek() == Some(&T::Semi) { None } else { Some(self.expr(0)?) };
        self.expect(T::Semi, ";")?;
        let inc = if self.peek() == Some(&T::RParen) { None } else { Some(self.expr(0)?) };
        self.expect(T::RParen, ")")?;
        Ok(SK::For(init, cond, inc, self.body()?))
    }

    /// Precedence climbing over the operators of NVSE Expressions, as `gen/x.rs`.
    fn expr(&mut self, min: u8) -> Result<E, Diag> {
        let mut left = self.unary()?;
        while let Some(T::Op(op)) = self.peek().cloned() {
            let Some((lb, rb)) = binding(&op) else { break };
            if lb < min {
                break;
            }
            self.i += 1;
            let right = self.expr(rb)?;
            left = if matches!(op.as_str(), "=" | ":=" | "+=" | "-=" | "*=" | "/=" | "%=" | "^=" | "|=" | "&=") {
                E::Assign(if op == "=" { ":=".into() } else { op }, Box::new(left), Box::new(right))
            } else {
                E::Bin(op, Box::new(left), Box::new(right))
            };
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<E, Diag> {
        if let Some(T::Op(op)) = self.peek().cloned() {
            if matches!(op.as_str(), "-" | "!" | "$" | "#" | "*" | "&") {
                self.i += 1;
                let bp = if op == "!" { 27 } else { 25 };
                let e = self.expr(bp)?;
                return Ok(E::Unary(op, Box::new(e)));
            }
            if op == "++" || op == "--" {
                self.i += 1;
                let e = self.expr(25)?;
                return Ok(E::Assign(if op == "++" { "+=".into() } else { "-=".into() }, Box::new(e), Box::new(E::Num("1".into()))));
            }
        }
        let mut e = self.primary()?;
        loop {
            match self.peek().cloned() {
                Some(T::LBrack) => {
                    self.i += 1;
                    let inner = self.expr(0)?;
                    self.expect(T::RBrack, "]")?;
                    e = E::Index(Box::new(e), Box::new(inner));
                }
                Some(T::LParen) => {
                    let args = self.args()?;
                    e = match e {
                        E::Name(name) => E::Call { name, on: None, args },
                        E::Member(on, name) => E::Call { name, on: Some(on), args },
                        other => E::Call { name: String::new(), on: Some(Box::new(other)), args },
                    };
                }
                Some(T::Op(o)) if o == "." => {
                    self.i += 1;
                    let n = self.word()?;
                    e = E::Member(Box::new(e), n);
                }
                Some(T::Op(o)) if o == "->" && !self.pattern => {
                    self.i += 1;
                    let k = match self.next() {
                        Some(T::Word(w)) => w,
                        Some(T::Str(p)) => match p.as_slice() {
                            [SPart::Lit(s)] => s.clone(),
                            _ => return Err(err(self.line(), "-> with an interpolated key")),
                        },
                        _ => return Err(err(self.line(), "-> without a key")),
                    };
                    e = E::Member(Box::new(e), format!("->{k}"));
                }
                Some(T::Op(o)) if o == "++" || o == "--" => {
                    self.i += 1;
                    e = E::Postfix(o, Box::new(e));
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn args(&mut self) -> Result<Vec<E>, Diag> {
        self.expect(T::LParen, "(")?;
        let mut v = Vec::new();
        while self.peek() != Some(&T::RParen) {
            v.push(self.expr(0)?);
            match self.peek() {
                Some(T::Comma) => self.i += 1,
                Some(T::RParen) => {}
                _ => return Err(err(self.line(), "expected , or ) after an argument")),
            }
        }
        self.i += 1;
        Ok(v)
    }

    fn primary(&mut self) -> Result<E, Diag> {
        let line = self.line();
        match self.next() {
            Some(T::Num(n)) => Ok(E::Num(n)),
            Some(T::Str(parts)) => {
                let mut v = Vec::new();
                for p in parts {
                    match p {
                        SPart::Lit(s) => v.push(SP::Lit(s)),
                        SPart::Expr(toks) => {
                            let mut sub = Parser { t: toks, i: 0, pattern: false };
                            let e = sub.expr(0)?;
                            if sub.peek().is_some() {
                                return Err(err(line, "unexpected text in ${…}"));
                            }
                            v.push(SP::E(e));
                        }
                    }
                }
                Ok(E::Str(v))
            }
            Some(T::LParen) => {
                let e = self.expr(0)?;
                self.expect(T::RParen, ")")?;
                Ok(e)
            }
            // `[a, b]` and `{k::v, …}` [documented: NVSEScript, Array/Map literals].
            Some(T::LBrack) => Ok(E::List(self.list(T::RBrack)?)),
            Some(T::LBrace) => Ok(E::Map(self.list(T::RBrace)?)),
            // `fn (params) { … }` and `fn (params) -> expr` [documented: Lambda].
            Some(T::Word(w)) if w.eq_ignore_ascii_case("fn") && self.peek() == Some(&T::LParen) => {
                let params = self.params()?;
                if self.is_op("->") {
                    self.i += 1;
                    let e = self.expr(0)?;
                    Ok(E::Lambda { params, body: LBody::Expr(Box::new(e)) })
                } else {
                    Ok(E::Lambda { params, body: LBody::Block(self.block()?) })
                }
            }
            Some(T::Word(w)) => Ok(E::Name(w)),
            _ => Err(err(line, "expected an expression")),
        }
    }

    fn list(&mut self, close: T) -> Result<Vec<E>, Diag> {
        let mut v = Vec::new();
        while self.peek() != Some(&close) {
            v.push(self.expr(0)?);
            if self.peek() == Some(&T::Comma) {
                self.i += 1;
            } else if self.peek() != Some(&close) {
                return Err(err(self.line(), "expected , in a literal"));
            }
        }
        self.i += 1;
        Ok(v)
    }
}

/// Left and right binding power, as `gen/x.rs` `infix`.
fn binding(op: &str) -> Option<(u8, u8)> {
    let l = |p: u8| (p * 2 + 1, p * 2 + 2);
    let r = |p: u8| (p * 2 + 1, p * 2 + 1);
    Some(match op {
        "=" | ":=" => r(0),
        "||" => l(1),
        "&&" => l(2),
        "+=" | "-=" | "*=" | "/=" | "^=" | "|=" | "&=" | "%=" => r(2),
        ":" | "::" => l(3),
        "==" | "!=" => l(4),
        "<" | ">" | "<=" | ">=" => l(5),
        "|" => l(6),
        "&" => l(7),
        "<<" | ">>" => l(8),
        "+" | "-" => l(9),
        "*" | "/" | "%" => l(10),
        "^" => l(11),
        _ => return None,
    })
}

// ---------------------------------------------------------------- the first grammar

#[derive(Default)]
struct Out {
    name: String,
    decls: Vec<(String, String)>,
    body: Vec<(String, usize)>,
    diags: Vec<Diag>,
    tmp: usize,
    /// Per loop open: a `for` loop's increment, run before `continue` and at the end.
    loops: Vec<Option<E>>,
}

/// The first grammar's type word for a variable type [documented: NVSEScript, Variable
/// types: array = array_var, string = string_var, double = float].
fn ty_word(t: &str) -> &'static str {
    match t.to_ascii_lowercase().as_str() {
        "int" => "int",
        "short" => "short",
        "long" => "long",
        "ref" => "ref",
        "string" => "string_var",
        "array" => "array_var",
        _ => "float",
    }
}

impl Out {
    fn line(&mut self, text: String, src: usize) {
        for l in text.split('\n') {
            self.body.push((l.to_string(), src));
        }
    }

    fn declare(&mut self, ty: &str, name: &str) {
        if !self.decls.iter().any(|(_, n)| n.eq_ignore_ascii_case(name)) {
            self.decls.push((ty_word(ty).to_string(), name.to_string()));
        }
    }

    fn is_var(&self, name: &str) -> bool {
        self.decls.iter().any(|(_, n)| n.eq_ignore_ascii_case(name))
    }

    fn root(&mut self, items: &[Item]) {
        // Declarations first: every variable of the unit is the script's [documented:
        // Lambda, "They can access and modify all variables defined in the parent script"].
        for it in items {
            if let Item::Decl(line, ty, vars) = it {
                for (n, init) in vars {
                    self.declare(ty, n);
                    if init.is_some() {
                        // A value given where no block runs: nothing sets it [hypothesis: the
                        // editor's new compiler runs it on the script's first run].
                        self.diags.push(Diag { line: *line, kind: "nvsescript_initializer_dropped", error: false, detail: n.clone() });
                    }
                }
            }
        }
        for it in items {
            match it {
                Item::Name(n) => self.name = n.clone(),
                Item::Decl(..) => {}
                // A function at the root is the unit's user-defined function [documented:
                // Lambda, the `name OnKeyDownUDF; fn (int iKeyID) { … }` example].
                Item::Fn(line, params, body) => {
                    for (t, n) in params {
                        self.declare(t, n);
                    }
                    let names: Vec<&str> = params.iter().map(|(_, n)| n.as_str()).collect();
                    self.line(format!("begin function {{{}}}", names.join(", ")), *line);
                    self.stmts(body);
                    self.line("end".into(), *line);
                }
                Item::Event(line, name, arg, body) => {
                    self.line(format!("begin {name}{}", arg.as_ref().map_or(String::new(), |a| format!(" {a}"))), *line);
                    self.stmts(body);
                    self.line("end".into(), *line);
                }
            }
        }
    }

    fn finish(self) -> Translated {
        let mut lines = Vec::new();
        let mut text = String::new();
        let first = self.body.first().map_or(1, |b| b.1);
        text.push_str(&format!("scn {}\n", self.name));
        lines.push(1);
        for (t, n) in &self.decls {
            text.push_str(&format!("{t} {n}\n"));
            lines.push(first);
        }
        for (l, n) in &self.body {
            text.push_str(l);
            text.push('\n');
            lines.push(*n);
        }
        Translated { text, lines, diags: self.diags }
    }

    fn stmts(&mut self, v: &[S]) {
        for s in v {
            self.stmt(s);
        }
    }

    fn stmt(&mut self, s: &S) {
        let at = s.line;
        match &s.k {
            SK::Decl(ty, vars) => {
                for (n, init) in vars {
                    self.declare(ty, n);
                    if let Some(e) = init {
                        let v = self.e(e);
                        self.line(format!("let {n} := {v}"), at);
                    }
                }
            }
            SK::Expr(e) => {
                let t = self.stmt_expr(e);
                self.line(t, at);
            }
            SK::Block(v) => self.stmts(v),
            SK::If(arms, els) => {
                for (k, (c, body)) in arms.iter().enumerate() {
                    let c = self.e(c);
                    self.line(format!("{} eval {c}", if k == 0 { "if" } else { "elseif" }), at);
                    self.stmts(body);
                }
                if let Some(b) = els {
                    self.line("else".into(), at);
                    self.stmts(b);
                }
                self.line("endif".into(), at);
            }
            SK::While(c, body) => {
                let c = self.e(c);
                self.line(format!("while {c}"), at);
                self.loops.push(None);
                self.stmts(body);
                self.loops.pop();
                self.line("loop".into(), at);
            }
            // `for (init; cond; inc)`: the init, a while over the condition, the body and
            // the increment [documented: NVSEScript, For loops].
            SK::For(init, cond, inc, body) => {
                if let Some(i) = init {
                    self.stmt(i);
                }
                let c = cond.as_ref().map_or("1".to_string(), |c| self.e(c));
                self.line(format!("while {c}"), at);
                self.loops.push(inc.clone());
                self.stmts(body);
                self.loops.pop();
                if let Some(i) = inc {
                    let t = self.stmt_expr(i);
                    self.line(t, at);
                }
                self.line("loop".into(), at);
            }
            SK::ForIn(var, src, body) => {
                let src = self.e(src);
                match var {
                    ForVar::Plain(ty, n) => {
                        self.declare(ty, n);
                        self.line(format!("foreach {n} <- {src}"), at);
                    }
                    // The element's key and value taken out of the iterator [documented:
                    // NVSEScript, For-each loops; ForEach, the iterator's "key" and "value"].
                    // One variable takes the value [documented for arrays; the page gives a
                    // map's key there, decision: the value, as the list form, until a map
                    // case is met].
                    ForVar::Destructure(vars) => {
                        self.tmp += 1;
                        let it = format!("nvfIter{}", self.tmp);
                        self.declare("array", &it);
                        self.line(format!("foreach {it} <- {src}"), at);
                        let keys: &[&str] = if vars.len() == 1 { &["value"] } else { &["key", "value"] };
                        for (v, k) in vars.iter().zip(keys) {
                            if let Some((ty, n)) = v {
                                self.declare(ty, n);
                                self.line(format!("let {n} := {it}[\"{k}\"]"), at);
                            }
                        }
                    }
                }
                self.loops.push(None);
                self.stmts(body);
                self.loops.pop();
                self.line("loop".into(), at);
            }
            // `match`: the arms in order, each compared with `==`, `_` the rest
            // [documented: NVSEScript, Match statements; decision: the subject is read
            // again for each arm].
            SK::Match(subj, arms) => {
                let sv = self.e(subj);
                let mut first = true;
                let mut wild = None;
                for (pat, body) in arms {
                    match pat {
                        Some(p) => {
                            let p = self.e(p);
                            self.line(format!("{} eval ({sv} == {p})", if first { "if" } else { "elseif" }), at);
                            first = false;
                            self.stmts(body);
                        }
                        None => wild = Some(body),
                    }
                }
                if let Some(b) = wild {
                    if first {
                        self.stmts(b);
                        return;
                    }
                    self.line("else".into(), at);
                    self.stmts(b);
                }
                if !first {
                    self.line("endif".into(), at);
                }
            }
            // `return x;`: SetFunctionValue, then return [documented: User Defined
            // Function, SetFunctionValue gives Call's result].
            SK::Return(e) => {
                if let Some(e) = e {
                    let v = self.e(e);
                    self.line(format!("SetFunctionValue {v}"), at);
                }
                self.line("return".into(), at);
            }
            SK::Break => self.line("break".into(), at),
            SK::Continue => {
                if let Some(Some(inc)) = self.loops.last().cloned() {
                    let t = self.stmt_expr(&inc);
                    self.line(t, at);
                }
                self.line("continue".into(), at);
            }
        }
    }

    /// An expression on a line of its own: an assignment is a `let`, anything else an
    /// `eval`.
    fn stmt_expr(&mut self, e: &E) -> String {
        match e {
            E::Assign(op, l, r) => {
                let (l, r) = (self.e(l), self.e(r));
                format!("let {l} {op} {r}")
            }
            E::Postfix(op, x) => {
                let x = self.e(x);
                format!("let {x} {}= 1", &op[..1])
            }
            other => {
                let v = self.e(other);
                format!("eval {v}")
            }
        }
    }

    /// An expression in the first grammar, parenthesised wherever the first grammar's own
    /// precedence or argument rules could read it otherwise.
    fn e(&mut self, e: &E) -> String {
        match e {
            E::Num(n) => n.clone(),
            E::Name(n) => n.clone(),
            E::Str(parts) => {
                let mut v = Vec::new();
                for p in parts {
                    match p {
                        SP::Lit(s) => v.push(lit(s)),
                        SP::E(x) => {
                            let x = self.e(x);
                            v.push(format!("$({x})"));
                        }
                    }
                }
                if v.len() == 1 && matches!(parts.as_slice(), [SP::Lit(_)]) {
                    v.remove(0)
                } else {
                    format!("({})", v.join(" + "))
                }
            }
            E::Call { name, on, args } => {
                let a: Vec<String> = args.iter().map(|x| self.e(x)).collect();
                let a = if a.is_empty() { String::new() } else { format!(" {}", a.join(", ")) };
                match on {
                    // A variable holding a function, called: `Call` [documented: Lambda,
                    // `rCallback(10)` is `Call rCallback 10`].
                    None if self.is_var(name) => format!("(Call {name}{a})"),
                    None => format!("({name}{a})"),
                    Some(o) => {
                        let o = match &**o {
                            E::Name(n) => n.clone(),
                            other => self.e(other),
                        };
                        if name.is_empty() {
                            format!("(Call {o}{a})")
                        } else {
                            format!("({o}.{name}{a})")
                        }
                    }
                }
            }
            E::Member(o, k) => {
                let o2 = match &**o {
                    E::Name(n) => n.clone(),
                    other => self.e(other),
                };
                match k.strip_prefix("->") {
                    Some(key) => format!("({o2}->{key})"),
                    // `Quest.var` or `ref.Function` with no arguments.
                    None => format!("{o2}.{k}"),
                }
            }
            E::Index(b, i) => {
                let (b, i) = (self.e(b), self.e(i));
                format!("{b}[{i}]")
            }
            E::Unary(op, x) => {
                let x = self.e(x);
                format!("({op}{x})")
            }
            // `x++` inside an expression gives the new value here [hypothesis: the page
            // names the operator only in examples; a postfix one gives the old value in
            // C, which no script on the lists relies on].
            E::Postfix(op, x) => {
                let x = self.e(x);
                format!("({x} {}= 1)", &op[..1])
            }
            E::Bin(op, l, r) => {
                let (l, r) = (self.e(l), self.e(r));
                format!("({l} {op} {r})")
            }
            E::Assign(op, l, r) => {
                let (l, r) = (self.e(l), self.e(r));
                format!("({l} {op} {r})")
            }
            E::List(v) if v.is_empty() => "(Ar_Construct \"array\")".into(),
            E::List(v) => {
                let a: Vec<String> = v.iter().map(|x| self.e(x)).collect();
                format!("(Ar_List {})", a.join(", "))
            }
            E::Map(v) if v.is_empty() => "(Ar_Construct \"stringmap\")".into(),
            E::Map(v) => {
                let a: Vec<String> = v.iter().map(|x| self.e(x)).collect();
                format!("(Ar_Map {})", a.join(", "))
            }
            E::Lambda { params, body } => {
                for (t, n) in params {
                    self.declare(t, n);
                }
                let names: Vec<&str> = params.iter().map(|(_, n)| n.as_str()).collect();
                match body {
                    LBody::Expr(x) => {
                        let x = self.e(x);
                        format!("({{{}}} => {x})", names.join(", "))
                    }
                    LBody::Block(b) => {
                        // The body's lines between `(begin function {…}` and `end)`; their
                        // source lines become the statement's.
                        let saved = std::mem::take(&mut self.body);
                        let loops = std::mem::take(&mut self.loops);
                        self.stmts(b);
                        let inner = std::mem::replace(&mut self.body, saved);
                        self.loops = loops;
                        let lines: Vec<String> = inner.into_iter().map(|(l, _)| l).collect();
                        format!("(begin function {{{}}}\n{}\nend)", names.join(", "), lines.join("\n"))
                    }
                }
            }
        }
    }
}

/// A string literal in the first grammar. A `;` there starts a comment and a line break
/// ends the line (script-compiler.md §1), and a `"` ends the string, so those characters
/// are written as their codes [documented: AsciiToChar].
fn lit(s: &str) -> String {
    let mut parts = Vec::new();
    let mut cur = String::new();
    for ch in s.chars() {
        let code = match ch {
            ';' => Some(59),
            '\n' => Some(10),
            '\r' => Some(13),
            '"' => Some(34),
            _ => None,
        };
        match code {
            Some(c) => {
                if !cur.is_empty() {
                    parts.push(format!("\"{}\"", std::mem::take(&mut cur)));
                }
                parts.push(format!("(AsciiToChar {c})"));
            }
            None => cur.push(ch),
        }
    }
    if !cur.is_empty() || parts.is_empty() {
        parts.push(format!("\"{cur}\""));
    }
    if parts.len() == 1 {
        parts.remove(0)
    } else {
        format!("({})", parts.join(" + "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tr(src: &str) -> String {
        translate(src).unwrap_or_else(|d| panic!("{d:?}")).text
    }

    #[test]
    fn a_function_unit() {
        let t = tr("name Fn;\n\nfn (int iMenuId) {\n    SetGameMainLoopCallback(Other, 1, 5, 2);\n}\n");
        assert_eq!(t, "scn Fn\nint iMenuId\nbegin function {iMenuId}\neval (SetGameMainLoopCallback Other, 1, 5, 2)\nend\n");
    }

    #[test]
    fn loops_and_strings() {
        let t = tr("name L;\nfn () {\n  string s = \"\";\n  for ([ref r] in GetList(106)) {\n    s += \"(${player.GetItemCount(r)}) ${r}\\n\";\n  }\n  for (int i = 0; i < 3; i++) { if (i == 1) { continue; } }\n}\n");
        assert!(t.contains("foreach nvfIter1 <- (GetList 106)"), "{t}");
        assert!(t.contains("let r := nvfIter1[\"value\"]"), "{t}");
        assert!(t.contains("let s += (\"(\" + $((player.GetItemCount r)) + \") \" + $(r) + (AsciiToChar 10))"), "{t}");
        assert!(t.contains("while (i < 3)\nif eval (i == 1)\nlet i += 1\ncontinue\nendif\nlet i += 1\nloop"), "{t}");
    }

    #[test]
    fn lines_map_back() {
        let r = translate("name A;\n\nGameMode {\n  int x = 1;\n  x++;\n}\n").unwrap();
        let at = |needle: &str| r.text.lines().position(|l| l == needle).map(|i| r.lines[i]);
        assert_eq!(at("begin GameMode"), Some(3));
        assert_eq!(at("let x := 1"), Some(4));
        assert_eq!(at("let x += 1"), Some(5));
    }

    #[test]
    fn detection() {
        assert!(is_nvsescript("// c\nname MyScript;\n"));
        assert!(!is_nvsescript("scn MyScript\n"));
        assert!(!is_nvsescript("name = 5\n"));
    }
}
