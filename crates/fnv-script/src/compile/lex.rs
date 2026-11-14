//! Source text → lines of tokens (docs/spec-notes/scripts.md §3.4, script-compiler.md §1).
//!
//! A line ends at a line break. Comments run from `;` outside double quotes to the end of
//! the line. A line with no letter or digit left produces no statement and is dropped
//! here, as are blank lines [observed: scripts.md §3.4].

/// What a token is, as far as the lexer can tell without knowing names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Letters, digits, `_` and `.`: a name, a number, `Ref.Func`, `Quest.var`.
    Word,
    /// `"…"`: the text between the quotes (an unterminated string runs to the end of the
    /// line).
    Str,
    /// `==` `!=` `<=` `>=` `&&` `||` `<` `>` `+` `-` `*` `/` `%`.
    Op,
    Open,
    Close,
    /// Any other single character (`:`, `` ` ``, `=` …).
    Punct,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tok {
    pub kind: Kind,
    pub text: String,
}

impl Tok {
    pub fn is_word(&self, w: &str) -> bool {
        self.kind == Kind::Word && self.text.eq_ignore_ascii_case(w)
    }
    pub fn is_op(&self, o: &str) -> bool {
        self.kind == Kind::Op && self.text == o
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// 1-based line number in the source.
    pub no: usize,
    pub toks: Vec<Tok>,
    /// The line's text without its comment, for the extender's lexer ([`xtokens`]).
    pub text: String,
}

/// The longest source line the editor accepts [documented: Inline Expression; Lambda].
pub const MAX_LINE: usize = 512;

/// The text of `line` before its comment.
///
/// A `;` starts a comment even inside double quotes [observed: the GECK refused
/// `Sv_Split "a,b;c", ",;"` with "Mismatched quotes" (probe strings/03, ext-bytecode.md
/// §7); the golden's three pieces came from the line rewritten without `;`]. No shipped
/// string holds a `;`, so vanilla parity is the same either way.
pub fn strip_comment(line: &str) -> &str {
    line.find(';').map_or(line, |i| &line[..i])
}

/// Whether a line's double quotes do not pair up (an unclosed string).
pub fn unclosed_quote(line: &str) -> bool {
    line.matches('"').count() % 2 == 1
}

/// A number as the compiler writes it: digits with at most one `.`, at least one digit
/// [observed: scripts.md §2.3].
pub fn is_number(s: &str) -> bool {
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

fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '.'
}

/// Tokens of one line (comment already removed). Commas separate like spaces; a word
/// ending in `.` joins the next word (`SomeREF. Disable`) [observed: scripts.md §3.4].
pub fn tokens(line: &str) -> Vec<Tok> {
    let cs: Vec<char> = line.chars().collect();
    let mut out: Vec<Tok> = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() || c == ',' {
            i += 1;
            continue;
        }
        let two: String = cs[i..(i + 2).min(cs.len())].iter().collect();
        let (kind, len) = if c == '"' {
            let end = cs[i + 1..].iter().position(|&q| q == '"').map(|p| i + 1 + p);
            let text: String = cs[i + 1..end.unwrap_or(cs.len())].iter().collect();
            i = end.map(|e| e + 1).unwrap_or(cs.len());
            // `"Name".Function`: a quoted name right before a dot is the call's reference,
            // one word that keeps the quotes (`Gen::form` reads them) [observed: Veronica
            // Outfit Replacer.esp ships `"E32A9".GetItemCount …` with compiled bytes calling
            // it on 0x000E32A9, #2275].
            if end.is_some() && cs.get(i) == Some(&'.') && cs.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == '_') {
                let n = cs[i..].iter().take_while(|&&w| word_char(w)).count();
                let rest: String = cs[i..i + n].iter().collect();
                out.push(Tok { kind: Kind::Word, text: format!("\"{text}\"{rest}") });
                i += n;
                continue;
            }
            out.push(Tok { kind: Kind::Str, text });
            continue;
        } else if matches!(two.as_str(), "==" | "!=" | "<=" | ">=" | "&&" | "||") {
            (Kind::Op, 2)
        } else if "<>+-*/%".contains(c) {
            (Kind::Op, 1)
        } else if c == '(' {
            (Kind::Open, 1)
        } else if c == ')' {
            (Kind::Close, 1)
        } else if word_char(c) {
            let n = cs[i..].iter().take_while(|&&w| word_char(w)).count();
            let text: String = cs[i..i + n].iter().collect();
            if let Some(prev) = out.last_mut() {
                if prev.kind == Kind::Word && prev.text.ends_with('.') && !is_number(&prev.text) && (c.is_ascii_alphabetic() || c == '_') {
                    prev.text.push_str(&text);
                    i += n;
                    continue;
                }
            }
            (Kind::Word, n)
        } else {
            (Kind::Punct, 1)
        };
        out.push(Tok { kind, text: cs[i..i + len].iter().collect() });
        i += len;
    }
    out
}

/// The line without `/* … */` comments, which may span lines (`in_block` carries one over)
/// [documented: xNVSE 6.1.7 comments]. Inside quotes they are text.
fn block_comments(line: &str, in_block: &mut bool) -> String {
    let cs: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut quoted = false;
    let mut i = 0;
    while i < cs.len() {
        if *in_block {
            if cs[i] == '*' && cs.get(i + 1) == Some(&'/') {
                *in_block = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        match cs[i] {
            '"' => quoted = !quoted,
            ';' => {
                out.extend(&cs[i..]);
                break;
            }
            '/' if !quoted && cs.get(i + 1) == Some(&'*') => {
                *in_block = true;
                i += 2;
                continue;
            }
            _ => {}
        }
        out.push(cs[i]);
        i += 1;
    }
    out
}

/// Every line that can produce a statement, with its tokens; `long` collects the numbers
/// of lines over [`MAX_LINE`] characters.
pub fn lines(src: &str, long: &mut Vec<usize>) -> Vec<Line> {
    let src = src.trim_end_matches('\0');
    let mut out = Vec::new();
    let mut in_block = false;
    for (n, raw) in src.split('\n').enumerate() {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let owned = block_comments(raw, &mut in_block);
        let raw = owned.as_str();
        if raw.chars().count() > MAX_LINE {
            long.push(n + 1);
        }
        let text = strip_comment(raw).trim();
        if !text.chars().any(|c| c.is_alphanumeric()) {
            continue;
        }
        let toks = tokens(text);
        if !toks.is_empty() {
            out.push(Line { no: n + 1, toks, text: text.to_string() });
        }
    }
    out
}

/// An `if`/`elseif` line whose parentheses are still open at its end takes the next lines
/// that start with `||`, `&&` or `)` until they close [observed: NpcCombatChem.esp ships
/// `If (( iConf == 4 && … )` with `|| ( iConf == 3 && … )` and more on the lines below, and
/// compiled bytes testing all of them in one condition, #2275; hypothesis: other lines do
/// not continue, and a continuation starts so].
pub fn join_open_conditions(lines: Vec<Line>) -> Vec<Line> {
    let depth = |t: &str| t.chars().fold(0i32, |d, c| match c {
        '(' => d + 1,
        ')' => d - 1,
        _ => d,
    });
    let mut out: Vec<Line> = Vec::new();
    let mut open = 0i32;
    for l in lines {
        let starts = |p: &str| l.text.starts_with(p);
        if open > 0 && (starts("||") || starts("&&") || starts(")")) {
            if let Some(prev) = out.last_mut() {
                prev.text = format!("{} {}", prev.text, l.text);
                prev.toks = tokens(&prev.text);
                open += depth(&l.text);
                continue;
            }
        }
        let head = l.toks.first().map(|t| t.text.to_ascii_lowercase());
        open = if matches!(head.as_deref(), Some("if" | "elseif")) { depth(&l.text) } else { 0 };
        out.push(l);
    }
    out
}

/// What a token of the extender's expressions is ([`xtokens`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XKind {
    Word,
    Num,
    Str,
    /// Every operator of NVSE Expressions, assignments included, and `=` (the Let macro's
    /// assignment, docs/spec-notes/ext-syntax.md).
    Op,
    Open,
    Close,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    /// Anything else.
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XTok {
    pub kind: XKind,
    pub text: String,
    /// A `-` with a space before it and a digit right after (`GetMaxOf -1 -5`): among
    /// arguments it starts the next one [observed: maxof.neg, aux.append.* in the
    /// original with JIP; `- 5` with spaces stays a subtraction, probe expressions/08].
    pub sign: bool,
}

/// `toks` without the one pair of parentheses that wraps all of it, if there is one
/// (`(a <- b)`; not `(a) <- (b)`).
pub fn unwrapped(toks: &[XTok]) -> &[XTok] {
    let wraps = toks.len() >= 2 && toks[0].kind == XKind::Open && toks[toks.len() - 1].kind == XKind::Close && {
        let mut depth = 0i32;
        toks[..toks.len() - 1].iter().all(|t| {
            match t.kind {
                XKind::Open => depth += 1,
                XKind::Close => depth -= 1,
                _ => {}
            }
            depth > 0
        })
    };
    if wraps {
        &toks[1..toks.len() - 1]
    } else {
        toks
    }
}

impl XTok {
    pub fn is(&self, kind: XKind, text: &str) -> bool {
        self.kind == kind && self.text.eq_ignore_ascii_case(text)
    }
}

/// Operators of the extender's expressions, longest first [documented: NVSE Expressions].
const XOPS: [&str; 33] = [
    "<-", "=>", ":=", "+=", "-=", "*=", "/=", "^=", "|=", "&=", "%=", "::", "->", "<<", ">>", "==", "!=", "<=", ">=", "&&", "||", ":", "|",
    "&", "+", "-", "*", "/", "%", "^", "<", ">", "=",
];

/// Tokens of a line in the extender's grammar. Words and numbers as in [`tokens`]; `!`,
/// `$` and `#` are operators too.
pub fn xtokens(line: &str) -> Vec<XTok> {
    let cs: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '"' {
            let end = cs[i + 1..].iter().position(|&q| q == '"').map(|p| i + 1 + p);
            out.push(XTok { kind: XKind::Str, text: cs[i + 1..end.unwrap_or(cs.len())].iter().collect(), sign: false });
            i = end.map(|e| e + 1).unwrap_or(cs.len());
            continue;
        }
        if word_char(c) {
            let n = cs[i..].iter().take_while(|&&w| word_char(w)).count();
            let text: String = cs[i..i + n].iter().collect();
            let kind = if is_number(&text) { XKind::Num } else { XKind::Word };
            out.push(XTok { kind, text, sign: false });
            i += n;
            continue;
        }
        let rest: String = cs[i..(i + 2).min(cs.len())].iter().collect();
        if let Some(op) = XOPS.iter().find(|o| rest.starts_with(**o)) {
            let spaced = i > 0 && cs[i - 1].is_whitespace();
            // `*` and `&` with a space before and a name or `(` right after are the unbox
            // and box prefixes (`Ar_Size *aKeybinds`, keybinder's ConstructModAddedJson.gek)
            // [hypothesis: the only reading in which the line works].
            let sign = (*op == "-" && spaced && cs.get(i + 1).is_some_and(|d| d.is_ascii_digit() || *d == '.'))
                || (matches!(*op, "*" | "&") && spaced && cs.get(i + 1).is_some_and(|d| d.is_alphabetic() || *d == '_' || *d == '('));
            out.push(XTok { kind: XKind::Op, text: op.to_string(), sign });
            i += op.chars().count();
            continue;
        }
        let kind = match c {
            '(' => XKind::Open,
            ')' => XKind::Close,
            '[' => XKind::LBracket,
            ']' => XKind::RBracket,
            '{' => XKind::LBrace,
            '}' => XKind::RBrace,
            ',' => XKind::Comma,
            '!' | '$' | '#' => XKind::Op,
            _ => XKind::Other,
        };
        out.push(XTok { kind, text: c.to_string(), sign: false });
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(l: &str) -> Vec<String> {
        tokens(l).into_iter().map(|t| t.text).collect()
    }

    #[test]
    fn splits_operators_words_and_strings() {
        assert_eq!(texts("if GetStage MQ01 >= 10 && x"), ["if", "GetStage", "MQ01", ">=", "10", "&&", "x"]);
        assert_eq!(texts("set x to (a+-1)*2"), ["set", "x", "to", "(", "a", "+", "-", "1", ")", "*", "2"]);
        let t = tokens("ModPCMiscStat \"Corpses Eaten\" 1");
        assert_eq!(t[1], Tok { kind: Kind::Str, text: "Corpses Eaten".into() });
    }

    #[test]
    fn commas_and_dot_space() {
        assert_eq!(texts("SomeREF. Disable"), ["SomeREF.Disable"]);
        assert_eq!(texts("AddItem Caps001, 10"), ["AddItem", "Caps001", "10"]);
        assert_eq!(texts("set x to 1. * 2"), ["set", "x", "to", "1.", "*", "2"]);
    }

    #[test]
    fn comments_and_empty_lines() {
        let mut long = Vec::new();
        let l = lines("scn Foo ; name\r\n\r\n; only a comment\n=====\nset s to \"a;b\" ; c\n", &mut long);
        assert_eq!(l.len(), 2);
        assert_eq!(l[0].no, 1);
        assert_eq!(l[1].no, 5);
        // A `;` starts a comment even inside quotes (the GECK, probe strings/03); the
        // vanilla line's unclosed string then runs to the end.
        assert_eq!(l[1].toks[3].text, "a");
        assert!(long.is_empty());
    }

    #[test]
    fn extender_tokens() {
        let t: Vec<String> = xtokens("let a[0] := b -> key + $c :: 2 != !d").into_iter().map(|t| t.text).collect();
        assert_eq!(t, ["let", "a", "[", "0", "]", ":=", "b", "->", "key", "+", "$", "c", "::", "2", "!=", "!", "d"]);
        let t = xtokens("x += 1.5, \"s\"");
        assert_eq!(t.iter().map(|t| t.kind).collect::<Vec<_>>(), [XKind::Word, XKind::Op, XKind::Num, XKind::Comma, XKind::Str]);
    }

    #[test]
    fn numbers() {
        assert!(is_number("19.50") && is_number(".5") && is_number("3") && is_number("1."));
        assert!(!is_number("1.2.3") && !is_number(".") && !is_number("0x10") && !is_number("a1"));
    }
}
