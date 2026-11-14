//! Lines of tokens → statements of the vanilla grammar (docs/spec-notes/script-language.md
//! §4–5). Expressions and arguments stay token lists here: how far a function's
//! arguments reach depends on its parameter count, which code generation knows.

use super::lex::{xtokens, Kind, Line, Tok, XKind, XTok};

/// Declared type of a local variable [documented: Scripting for Beginners].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclType {
    /// `short`, `int`, `long`: an `Integer` slot.
    Int,
    /// `float`: a `FloatOrRef` slot.
    Float,
    /// `ref`, `reference`: a `FloatOrRef` slot.
    Ref,
    /// `string_var` [documented: String Variable].
    String,
    /// `array_var` [documented: Array Variable].
    Array,
}

impl DeclType {
    pub fn from_word(w: &str) -> Option<DeclType> {
        Some(match w.to_ascii_lowercase().as_str() {
            "short" | "int" | "long" => DeclType::Int,
            "float" => DeclType::Float,
            "ref" | "reference" => DeclType::Ref,
            "string_var" => DeclType::String,
            "array_var" => DeclType::Array,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kw {
    ScriptName(Option<String>),
    /// A declaration produces no statement [observed: scripts.md §3.4]; with `= value`
    /// (`int i = 5`, the extender's inline declaration) it also assigns.
    Decl { ty: DeclType, names: Vec<String>, init: Option<Vec<XTok>> },
    Begin { block: String, args: Vec<Tok> },
    End,
    If(Vec<Tok>),
    ElseIf(Vec<Tok>),
    Else,
    EndIf,
    Return,
    /// `on`: the reference written before `.Set`, kept as the statement's prefix.
    Set { target: Vec<Tok>, expr: Vec<Tok>, on: Option<String> },
    /// `[Ref.]Function args…`, or a line the code generator reports as unknown.
    Call(Vec<Tok>),
    /// `let <assignment>`, or the Let macro (`x = 1`, `x += 1`), in the extender's tokens
    /// [documented: Let; Let Macro].
    Let(Vec<XTok>),
    /// `if eval <expression>` / `elseif eval …` [documented: Eval].
    IfEval(Vec<XTok>),
    ElseIfEval(Vec<XTok>),
    /// `eval <expression>` on a line of its own: the tokens after `eval`. The expression
    /// is run for what it does, an assignment included (`eval iTemp = iTemp * (1 - …)`,
    /// `eval Quest.rRef.Activate PlayerRef 1`) [documented: Eval, an NVSE-aware command
    /// taking an expression; observed: JustAssortedMods.esp 4.6 ships 32 such lines with
    /// compiled bytes, two of them assignments].
    Eval(Vec<XTok>),
    /// `while <condition>` [documented: While].
    While(Vec<XTok>),
    /// `foreach <var> <- <source>` [documented: ForEach]: the tokens after `foreach`.
    ForEach(Vec<XTok>),
    /// `loop`, closing a `while` or `foreach`.
    Loop,
    Break,
    Continue,
    /// `Label n` / `GoTo n` [documented: Label; GoTo], n 0 when left out.
    Label(i32),
    GoTo(i32),
}

/// The assignments the Let macro takes without `let` [documented: Let Macro: `=`, `+=`,
/// `-=`, `*=`; observed: `/=`, JustAssortedMods.esp 4.6 ships `fAP /= GetNumericGameSetting
/// …` (JBTOnFireWeaponUDF) with compiled bytes, so the original's compiler takes it; `:=`,
/// thirteen units of the owner's Blue Moon list (TLD_Travelers.esm `doOnce := 1`,
/// DryWellsReloaded.esm `Quest.iObjectivePoint := 1`, DefenderRifle.esp `rContainer :=
/// GetContainer`, …) ship compiled bytes where the line is the same Let as with `let`
/// (#2275)].
const MACRO_OPS: [&str; 6] = ["=", ":=", "+=", "-=", "*=", "/="];

/// A line of the extender's grammar, by its first words.
fn extender(l: &Line) -> Option<Kw> {
    let head = l.toks[0].text.to_ascii_lowercase();
    let second = l.toks.get(1).map(|t| t.text.to_ascii_lowercase());
    match (head.as_str(), second.as_deref()) {
        ("let", _) => Some(Kw::Let(spaced_assign(xtokens(&l.text)[1..].to_vec()))),
        ("if", Some("eval")) => Some(Kw::IfEval(xtokens(&l.text)[2..].to_vec())),
        ("elseif", Some("eval")) => Some(Kw::ElseIfEval(xtokens(&l.text)[2..].to_vec())),
        ("eval", Some(_)) => Some(Kw::Eval(xtokens(&l.text)[1..].to_vec())),
        ("while", _) =>Some(Kw::While(xtokens(&l.text)[1..].to_vec())),
        ("foreach", _) => Some(Kw::ForEach(xtokens(&l.text)[1..].to_vec())),
        ("loop", None) => Some(Kw::Loop),
        ("break", None) => Some(Kw::Break),
        ("label", _) => Some(Kw::Label(second.as_deref().and_then(|n| n.parse().ok()).unwrap_or(0))),
        ("goto", _) => Some(Kw::GoTo(second.as_deref().and_then(|n| n.parse().ok()).unwrap_or(0))),
        ("continue", None) => Some(Kw::Continue),
        _ => {
            let x = xtokens(&l.text);
            // The target may be subscripted (`aEvent["Count"] = …`, `Q.aItem[i] = 0`)
            // [observed: JustAssortedMods.esp's event handlers compile so in the original].
            let mut depth = 0i32;
            let mut at = None;
            for (i, t) in x.iter().enumerate() {
                match t.kind {
                    XKind::LBracket => depth += 1,
                    XKind::RBracket => depth -= 1,
                    XKind::Op if depth == 0 && MACRO_OPS.contains(&t.text.as_str()) => {
                        at = Some(i);
                        break;
                    }
                    XKind::Word if depth == 0 && i == 0 => {}
                    _ if depth > 0 => {}
                    _ => break,
                }
            }
            let assigns = x.len() > 2 && x[0].kind == XKind::Word && at.is_some_and(|i| i + 1 < x.len());
            assigns.then_some(Kw::Let(x))
        }
    }
}

/// A `let`'s `:` and `=` written apart (`Let r : = …`) are its `:=` [observed: Hymn of
/// The New West.esp ships `Let rVaultTerminal : = (GetFormFromMod …)` with compiled bytes
/// where the line is the same Let as its neighbours' `:=`, #2275].
fn spaced_assign(mut x: Vec<XTok>) -> Vec<XTok> {
    if let Some(i) = (0..x.len().saturating_sub(1)).find(|&i| x[i].is(XKind::Op, ":") && x[i + 1].is(XKind::Op, "=")) {
        x[i].text = ":=".into();
        x.remove(i + 1);
    }
    x
}

#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub line: usize,
    pub kw: Kw,
    /// The line's text without its comment.
    pub text: String,
    /// The variables the line declares inline ([`inline_decls`]), taken out of `kw`.
    pub decls: Vec<(DeclType, String)>,
}

impl Kw {
    /// The extender tokens of a `let`, `if eval`, `elseif eval`, `while` or `foreach` line.
    fn xtoks_mut(&mut self) -> Option<&mut Vec<XTok>> {
        match self {
            Kw::Let(t) | Kw::IfEval(t) | Kw::ElseIfEval(t) | Kw::Eval(t) | Kw::While(t) | Kw::ForEach(t) => Some(t),
            _ => None,
        }
    }

    /// The statement is the extender's grammar (the unit is then compiled with its rules,
    /// script-compiler.md §8).
    pub fn extended(&self) -> bool {
        match self {
            Kw::Let(_) | Kw::IfEval(_) | Kw::ElseIfEval(_) | Kw::Eval(_) | Kw::While(_) | Kw::ForEach(_) | Kw::Loop | Kw::Break | Kw::Continue | Kw::Label(_) | Kw::GoTo(_) => true,
            Kw::Decl { ty, init, .. } => matches!(ty, DeclType::String | DeclType::Array) || init.is_some(),
            // `Begin _GameMode`: a compiler-override block [documented: Script Compiler Override].
            Kw::Begin { block, .. } => block.starts_with('_') || block.eq_ignore_ascii_case("function"),
            _ => false,
        }
    }
}

/// A line that does not fit the grammar.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

pub fn statement(l: &Line) -> Result<Statement, ParseError> {
    let toks = &l.toks;
    let head = &toks[0];
    let rest = toks[1..].to_vec();
    let err = |msg: &str| ParseError { line: l.no, msg: msg.to_string() };
    if head.kind == Kind::Word {
        if let Some(mut kw) = extender(l) {
            let decls = kw.xtoks_mut().map(inline_decls).unwrap_or_default();
            return Ok(Statement { line: l.no, kw, text: l.text.clone(), decls });
        }
    }
    let kw = if head.kind != Kind::Word {
        Kw::Call(toks.clone())
    } else {
        let word = head.text.to_ascii_lowercase();
        // `Ref.Set x to v`: a set of the script's own `x` with the reference as its
        // calling-reference prefix [observed: Fallout3.esm and RAD.esp ship
        // `MS14ArmorVoiceRef.Set SayTimer to GameDaysPassed + 0.1` with compiled bytes
        // `1C 00 <ref> 15 00 …` whose target is the local SayTimer, #2335].
        let (word, on) = match word.strip_suffix(".set") {
            Some(r) if !r.is_empty() && !r.contains('.') => ("set", Some(head.text[..r.len()].to_string())),
            _ => (word.as_str(), None),
        };
        match word {
            "scn" | "scriptname" => Kw::ScriptName(rest.first().map(|t| t.text.clone())),
            "begin" => {
                let Some(b) = rest.first().filter(|t| t.kind == Kind::Word) else { return Err(err("Begin without a block name")) };
                Kw::Begin { block: b.text.clone(), args: rest[1..].to_vec() }
            }
            "end" => Kw::End,
            "if" => Kw::If(rest),
            "elseif" => Kw::ElseIf(rest),
            "else" => Kw::Else,
            "endif" => Kw::EndIf,
            "return" => Kw::Return,
            "set" => {
                let Some(k) = rest.iter().position(|t| t.is_word("to")) else { return Err(err("Set without To")) };
                if k == 0 {
                    return Err(err("Set without a variable"));
                }
                Kw::Set { target: rest[..k].to_vec(), expr: rest[k + 1..].to_vec(), on }
            }
            w => match DeclType::from_word(w) {
                Some(ty) => {
                    let x = xtokens(&l.text);
                    // A compound assignment declares and assigns too (`float fChance +=
                    // rand fMin, fMax`) [observed: Vegas Elysium.esp ships that line with
                    // compiled bytes adding Rand's value to the one variable, #2275; read
                    // as a plain declaration it declared `rand` and the bounds as floats].
                    match x.get(2) {
                        Some(eq) if eq.kind == XKind::Op && (MACRO_OPS.contains(&eq.text.as_str()) || ["^=", "|=", "&=", "%="].contains(&eq.text.as_str())) => {
                            Kw::Decl { ty, names: vec![x[1].text.clone()], init: Some(x[1..].to_vec()) }
                        }
                        _ => Kw::Decl { ty, names: rest.iter().filter(|t| t.kind == Kind::Word).map(|t| t.text.clone()).collect(), init: None },
                    }
                }
                None => Kw::Call(toks.clone()),
            },
        }
    };
    // A call's inline expressions may declare (`Fn (float f)`); the code generator reads
    // the line again and drops the type word there.
    // So may a declaration's value (`int n = Ar_Size (array_var a = GetFiles …)`): the
    // inline one is declared with the line's, so that a lambda's later lines find it
    // [observed: B42 Interact's ln_B42Interact.txt runs in the original with such a line
    // in an event handler's lambda and `xkR = ar_null` after it].
    let decls = match &kw {
        Kw::Call(_) if toks.iter().any(|t| t.kind == Kind::Open) => inline_decls(&mut xtokens(&l.text)),
        Kw::Decl { init: Some(x), .. } => inline_decls(&mut x[1..].to_vec()),
        _ => Vec::new(),
    };
    Ok(Statement { line: l.no, kw, text: l.text.clone(), decls })
}

/// The inline declarations of an extender line, taken out of its tokens: a type word right
/// before a name that is assigned (`=`, `:=`) or iterated (`<-`) declares it there
/// [documented: Inline variable declaration, `let int i := 10`, `foreach array_var it <-
/// arr`, `if eval ref r = GetCrosshairRef && IsReference r`; observed: the ext-compile
/// README, inline `int` declarations with `let` compiled]. An inline expression is an
/// NVSE expression, so it declares too (`SetUIFloatAlt "…" (int i = GetPCDetectionState)`)
/// [documented: Inline variable declaration, "NVSE expressions can now declare variables";
/// Inline Expression], and so does a type and a name alone in parentheses, where a
/// function writes its result into the variable (`GetNifRotAlt "Node" (float fX) (float
/// fY) (float fZ)`) [hypothesis: the owner's mods' Script Runner files are written so,
/// #1470; the pages show only `=`, `:=` and `<-`]. A lambda's typed parameters
/// (`{int a, ref b} =>`) are followed by `,` or `}`, so they stay.
pub fn inline_decls(x: &mut Vec<XTok>) -> Vec<(DeclType, String)> {
    let mut decls = Vec::new();
    let mut i = 0;
    while i + 2 < x.len() {
        let assigned = x[i + 2].kind == XKind::Op && [":=", "=", "<-"].contains(&x[i + 2].text.as_str())
            || i > 0 && x[i - 1].kind == XKind::Open && x[i + 2].kind == XKind::Close;
        match DeclType::from_word(&x[i].text.to_ascii_lowercase()) {
            Some(ty) if x[i].kind == XKind::Word && x[i + 1].kind == XKind::Word && assigned => {
                decls.push((ty, x[i + 1].text.clone()));
                x.remove(i);
            }
            _ => {}
        }
        i += 1;
    }
    decls
}

#[cfg(test)]
mod tests {
    use super::super::lex::lines;
    use super::*;

    fn parse(src: &str) -> Vec<Kw> {
        lines(src, &mut Vec::new()).iter().map(|l| statement(l).unwrap().kw).collect()
    }

    #[test]
    fn classifies_lines() {
        let k = parse("scn Foo\nshort a\nref r\nbegin OnTrigger player\nset a to 1 + 2\nif a\nelse\nendif\nr.Disable\nend\n");
        assert_eq!(k[0], Kw::ScriptName(Some("Foo".into())));
        assert_eq!(k[1], Kw::Decl { ty: DeclType::Int, names: vec!["a".into()], init: None });
        assert_eq!(k[2], Kw::Decl { ty: DeclType::Ref, names: vec!["r".into()], init: None });
        assert!(matches!(&k[3], Kw::Begin { block, args } if block == "OnTrigger" && args.len() == 1));
        assert!(matches!(&k[4], Kw::Set { target, expr, .. } if target.len() == 1 && expr.len() == 3));
        assert!(matches!(&k[5], Kw::If(e) if e.len() == 1));
        assert_eq!(k[6], Kw::Else);
        assert_eq!(k[7], Kw::EndIf);
        assert!(matches!(&k[8], Kw::Call(t) if t[0].text == "r.Disable"));
        assert_eq!(k[9], Kw::End);
    }

    #[test]
    fn set_needs_to() {
        let l = lines("set a 5", &mut Vec::new());
        assert!(statement(&l[0]).is_err());
    }
}
