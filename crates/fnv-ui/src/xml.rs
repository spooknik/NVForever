//! A forgiving reader for the menu files' XML (docs/spec-notes/menu-xml.md §1).
//!
//! The game's own files are not well-formed XML [observed: 43 of the 126 files under
//! `menus\` fail a strict parser]: entities are used that no file declares (`&true;`,
//! `&-sInventoryItems;`), a file may hold several top-level elements (the prefabs are
//! fragments), end tags may not match, and one attribute is missing its opening quote.
//! So this reader never fails: it builds what it can and lists what it had to mend.

/// One piece of an element's content.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Element(Element),
    /// Text as written, with the predefined entities (`&amp;` …, `&#N;`) decoded.
    Text(String),
    /// A named entity the reader does not know, `&name;`, kept by name (without `&`
    /// and `;`). The menu files' constants (`&true;`, `&center;`) and game-setting
    /// strings (`&-sName;`) are these.
    Entity(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    /// The tag as written; compare with [`Element::is`].
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    /// The 1-based line the start tag is on.
    pub line: u32,
}

impl Element {
    /// Whether the tag is `name`, ignoring case.
    pub fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }

    /// An attribute's value, its name compared without case.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The element children.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }
}

/// What a file read into.
#[derive(Clone, Debug, Default)]
pub struct Doc {
    /// The top-level nodes (a menu file has one `<menu>`, a prefab several).
    pub nodes: Vec<Node>,
    /// What the reader mended, one line each, with the line number.
    pub warnings: Vec<String>,
}

/// Bytes of a menu file as text: UTF-8 when it is, else 8-bit text in this run's code
/// page (`fnv_util::text`, #1439) [hypothesis: the files are 8-bit Windows text in the
/// install's code page; every vanilla English one is ASCII].
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => fnv_util::text::decode(bytes),
    }
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

/// Reads `src`; never fails.
pub fn parse(src: &str) -> Doc {
    let chars: Vec<char> = src.chars().collect();
    let mut p = Parser { s: &chars, i: 0, line: 1, warnings: Vec::new() };
    // The open elements; the bottom one is the document.
    let mut stack: Vec<Element> = vec![Element::default()];
    while p.i < chars.len() {
        if p.peek_str("<!--") {
            let line = p.line;
            p.advance(4);
            if !p.skip_past("-->") {
                p.warn(line, "comment not closed");
            }
        } else if p.peek_str("<![CDATA[") {
            p.advance(9);
            let start = p.i;
            let line = p.line;
            let closed = p.skip_past("]]>");
            let end = if closed { p.i - 3 } else { p.i };
            if !closed {
                p.warn(line, "CDATA not closed");
            }
            let text: String = chars[start..end].iter().collect();
            push_text(&mut stack, text);
        } else if p.peek_str("<?") || p.peek_str("<!") {
            p.advance(2);
            p.skip_past(">");
        } else if p.peek_str("</") {
            let line = p.line;
            p.advance(2);
            let name = p.take_while(name_char);
            p.skip_past(">");
            close(&mut stack, &name, line, &mut p.warnings);
        } else if p.peek() == Some('<') && p.peek_at(1).is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
            let line = p.line;
            p.advance(1);
            let name = p.take_while(name_char);
            let (attrs, empty) = p.attributes(line);
            let el = Element { name, attrs, children: Vec::new(), line };
            if empty {
                stack.last_mut().unwrap().children.push(Node::Element(el));
            } else {
                stack.push(el);
            }
        } else {
            // Text up to the next tag; a `<` that starts no tag is text.
            let start = p.i;
            p.advance(1);
            while p.i < chars.len() {
                let c = chars[p.i];
                if c == '<' && p.peek_at(1).is_some_and(|n| n.is_ascii_alphabetic() || matches!(n, '_' | '/' | '!' | '?')) {
                    break;
                }
                p.advance(1);
            }
            let raw: String = chars[start..p.i].iter().collect();
            push_content(&mut stack, &raw);
        }
    }
    while stack.len() > 1 {
        let el = stack.pop().unwrap();
        p.warnings.push(format!("line {}: <{}> not closed at the end of the file", el.line, el.name));
        stack.last_mut().unwrap().children.push(Node::Element(el));
    }
    Doc { nodes: stack.pop().unwrap().children, warnings: p.warnings }
}

/// An end tag closes the innermost open element of the name it gives (and those opened
/// inside it); when none of that name is open, it closes the innermost open element
/// whatever its name; with nothing open it is dropped [hypothesis, from what works in
/// the original: MCM runs with both `<_KeyInut>` … `</_KeyInput>` and a `<_target>` whose
/// `<onlyif>` is never closed (ModListRect.xml); the first needs the second rule, the
/// second the first. Observer check: menu-xml.md §8].
fn close(stack: &mut Vec<Element>, name: &str, line: u32, warnings: &mut Vec<String>) {
    if stack.len() == 1 {
        warnings.push(format!("line {line}: </{name}> closes nothing open; dropped"));
        return;
    }
    let at = (1..stack.len()).rev().find(|&k| stack[k].name.eq_ignore_ascii_case(name)).unwrap_or(stack.len() - 1);
    while stack.len() > at {
        let el = stack.pop().unwrap();
        if !el.name.eq_ignore_ascii_case(name) {
            warnings.push(format!("line {line}: <{}> (line {}) closed by </{name}>", el.name, el.line));
        }
        stack.last_mut().unwrap().children.push(Node::Element(el));
    }
}

fn push_text(stack: &mut [Element], text: String) {
    let top = stack.last_mut().unwrap();
    if let Some(Node::Text(t)) = top.children.last_mut() {
        t.push_str(&text);
    } else {
        top.children.push(Node::Text(text));
    }
}

/// Text with entities: the predefined ones decoded into the text, the others kept as
/// [`Node::Entity`].
fn push_content(stack: &mut [Element], raw: &str) {
    let mut text = String::new();
    let mut rest = raw;
    while let Some(k) = rest.find('&') {
        text.push_str(&rest[..k]);
        let after = &rest[k + 1..];
        let len = after.find(|c: char| !(name_char(c) || c == '#')).unwrap_or(after.len());
        if len > 0 && after[len..].starts_with(';') {
            let name = &after[..len];
            match predefined(name) {
                Some(c) => text.push(c),
                None => {
                    if !text.is_empty() {
                        push_text(stack, std::mem::take(&mut text));
                    }
                    stack.last_mut().unwrap().children.push(Node::Entity(name.to_string()));
                }
            }
            rest = &after[len + 1..];
        } else {
            text.push('&');
            rest = after;
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        push_text(stack, text);
    }
}

fn predefined(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let n = name.strip_prefix('#')?;
            let v = match n.strip_prefix(['x', 'X']) {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => n.parse().ok()?,
            };
            char::from_u32(v)?
        }
    })
}

struct Parser<'a> {
    s: &'a [char],
    i: usize,
    line: u32,
    warnings: Vec<String>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn peek_at(&self, k: usize) -> Option<char> {
        self.s.get(self.i + k).copied()
    }

    fn peek_str(&self, t: &str) -> bool {
        t.chars().enumerate().all(|(k, c)| self.peek_at(k) == Some(c))
    }

    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            if self.peek() == Some('\n') {
                self.line += 1;
            }
            if self.i < self.s.len() {
                self.i += 1;
            }
        }
    }

    /// Moves past the next `t`; false (at the end) when there is none.
    fn skip_past(&mut self, t: &str) -> bool {
        while self.i < self.s.len() {
            if self.peek_str(t) {
                self.advance(t.chars().count());
                return true;
            }
            self.advance(1);
        }
        false
    }

    fn take_while(&mut self, f: impl Fn(char) -> bool) -> String {
        let start = self.i;
        while self.peek().is_some_and(&f) {
            self.advance(1);
        }
        self.s[start..self.i].iter().collect()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.advance(1);
        }
    }

    fn warn(&mut self, line: u32, what: &str) {
        self.warnings.push(format!("line {line}: {what}"));
    }

    /// The attributes up to the tag's `>`; true when it ends `/>`.
    fn attributes(&mut self, line: u32) -> (Vec<(String, String)>, bool) {
        let mut attrs = Vec::new();
        loop {
            self.skip_space();
            match self.peek() {
                None => {
                    self.warn(line, "tag not closed at the end of the file");
                    return (attrs, true);
                }
                Some('>') => {
                    self.advance(1);
                    return (attrs, false);
                }
                Some('/') if self.peek_at(1) == Some('>') => {
                    self.advance(2);
                    return (attrs, true);
                }
                // A start tag cut short by the next tag (`<systemcolor</systemcolor>` in
                // two vanilla files [observed]): ends there, open, so that the end tag
                // after it closes it.
                Some('<') => {
                    self.warn(line, "tag cut short by the next one");
                    return (attrs, false);
                }
                Some(c) if name_char(c) => {
                    let name = self.take_while(name_char);
                    self.skip_space();
                    let value = if self.peek() == Some('=') {
                        self.advance(1);
                        self.skip_space();
                        self.attr_value(line, &name)
                    } else {
                        String::new()
                    };
                    attrs.push((name, value));
                }
                Some(_) => self.advance(1),
            }
        }
    }

    fn attr_value(&mut self, line: u32, name: &str) -> String {
        match self.peek() {
            Some(q @ ('"' | '\'')) => {
                self.advance(1);
                let v = self.take_while(|c| c != q);
                self.advance(1);
                v
            }
            _ => {
                // Unquoted (or missing its opening quote): up to a space or the tag's end,
                // a stray closing quote dropped.
                let v = self.take_while(|c| !c.is_whitespace() && c != '>' && c != '/');
                self.warn(line, &format!("attribute {name} not quoted"));
                v.trim_matches(['"', '\'']).to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(n: &Node) -> &Element {
        match n {
            Node::Element(e) => e,
            other => panic!("not an element: {other:?}"),
        }
    }

    #[test]
    fn fragments_entities_and_comments() {
        let d = parse("<!-- head -->\n<a> 1 </a>\n<b x=\"p()\">&true; and &amp; &-sName;</b>");
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
        let els: Vec<_> = d.nodes.iter().filter(|n| matches!(n, Node::Element(_))).map(el).collect();
        assert_eq!(els.len(), 2);
        assert_eq!(els[0].children, vec![Node::Text(" 1 ".into())]);
        assert_eq!(els[1].attr("X"), Some("p()"));
        assert_eq!(
            els[1].children,
            vec![Node::Entity("true".into()), Node::Text(" and & ".into()), Node::Entity("-sName".into())]
        );
        assert_eq!(els[1].line, 3);
    }

    #[test]
    fn mends_stray_and_mismatched_end_tags() {
        // `</Bx>` (no such open) closes `<b>`; `</a>` closes the unclosed `<onlyif>` and
        // the `<a>` it is in; `</x>` closes nothing.
        let d = parse("<rect><b>1</Bx><a><onlyif>2</a><c/></rect></x><text>");
        let r = el(&d.nodes[0]);
        assert!(r.is("RECT"));
        let names: Vec<_> = r.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["b", "a", "c"]);
        assert!(el(&el(&r.children[1]).children[0]).is("onlyif"));
        assert_eq!(d.warnings.len(), 4, "{:?}", d.warnings);
        assert!(el(&d.nodes[1]).is("text"));
    }

    #[test]
    fn attribute_missing_its_opening_quote() {
        let d = parse("<rect name=item_parent\"><copy src='me()' trait=\"x\"/></rect>");
        let r = el(&d.nodes[0]);
        assert_eq!(r.attr("name"), Some("item_parent"));
        let c = el(&r.children[0]);
        assert_eq!(c.attr("src"), Some("me()"));
        assert_eq!(d.warnings.len(), 1);
    }

    #[test]
    fn a_start_tag_cut_short() {
        let d = parse("<image><systemcolor</systemcolor><locus>1</locus></image>");
        let i = el(&d.nodes[0]);
        let names: Vec<_> = i.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["systemcolor", "locus"]);
        assert_eq!(d.warnings.len(), 1, "{:?}", d.warnings);
    }

    #[test]
    fn a_lone_less_than_is_text() {
        let d = parse("<a> 1 < 2 &unknown </a>");
        assert_eq!(el(&d.nodes[0]).children, vec![Node::Text(" 1 < 2 &unknown ".into())]);
    }

    #[test]
    fn latin1_fallback() {
        assert_eq!(decode(&[b'a', 0xE9]), "a\u{e9}");
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'x']), "x");
    }
}
