//! Shared helpers:
//! - The game's 8-bit text: [`text`] in the install's code page (Windows-1252 unless set
//!   otherwise, e.g. 1251 for a Russian install), [`cp1252`] always in Windows-1252, via
//!   `encoding_rs`
//! - SHA-256 for corpus fingerprints, via `sha2`
//! - `Cursor`, a bounds-checked little-endian byte reader

pub mod cp1252 {
    use encoding_rs::WINDOWS_1252;

    /// Decode. Undefined slots (0x81, 0x8D, 0x8F, 0x90, 0x9D) map to the C1 control
    /// of the same value (WHATWG "windows-1252"), so decoding is lossless.
    pub fn decode(bytes: &[u8]) -> String {
        WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned()
    }

    /// Encode; returns None if a character has no Windows-1252 representation.
    pub fn encode(s: &str) -> Option<Vec<u8>> {
        let (out, _, unmappable) = WINDOWS_1252.encode(s);
        (!unmappable).then(|| out.into_owned())
    }

    /// Decode a zero-terminated string: stops at the first NUL.
    pub fn decode_z(bytes: &[u8]) -> String {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        decode(&bytes[..end])
    }
}

/// The game's 8-bit text in the install's code page.
///
/// The original's plugins, string fields, script strings and font glyph tables carry text
/// as one byte a character. An English install's bytes are Windows-1252 [observed:
/// docs/spec-notes/esm.md, "Text"]; a localized install is
/// thought to keep the same one-byte format in its own code page, e.g. Windows-1251 for a
/// Russian one [hypothesis: no localized data looked at, docs/spec-notes/fonts.md §2.1].
///
/// The code page is one setting for the whole process ([`set_code_page`], read by
/// [`code_page`]), chosen once at the start before any text is read: NVForever's own
/// `NVForever.ini` (`[Text] CodePage=`) or `NVF_CODE_PAGE`. Windows-1252 is the default,
/// so an English install reads exactly as it did. Every code page here maps all 256
/// bytes to a character (the WHATWG tables: an undefined slot becomes the C1 control of
/// the same value), so decoding and encoding again gives the same bytes back.
pub mod text {
    use encoding_rs::{Encoding, WINDOWS_1250, WINDOWS_1251, WINDOWS_1252};
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::OnceLock;

    /// A one-byte code page of the game's text.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub enum CodePage {
        /// Western European; English installs [observed].
        #[default]
        Windows1252,
        /// Cyrillic; Russian installs [hypothesis].
        Windows1251,
        /// Central European (Polish, Czech); [hypothesis].
        Windows1250,
    }

    impl CodePage {
        pub const ALL: [CodePage; 3] = [CodePage::Windows1252, CodePage::Windows1251, CodePage::Windows1250];

        /// The code page's number: 1252, 1251, 1250.
        pub fn number(self) -> u16 {
            match self {
                CodePage::Windows1252 => 1252,
                CodePage::Windows1251 => 1251,
                CodePage::Windows1250 => 1250,
            }
        }

        /// The code page a setting names: its number, with or without `cp`, `windows-`
        /// or `windows` before it (case ignored).
        pub fn from_name(name: &str) -> Option<CodePage> {
            let n = name.trim().to_ascii_lowercase();
            let digits = ["windows-", "windows", "cp"].iter().find_map(|p| n.strip_prefix(p)).unwrap_or(&n);
            CodePage::ALL.into_iter().find(|c| digits == c.number().to_string())
        }

        fn encoding(self) -> &'static Encoding {
            match self {
                CodePage::Windows1252 => WINDOWS_1252,
                CodePage::Windows1251 => WINDOWS_1251,
                CodePage::Windows1250 => WINDOWS_1250,
            }
        }

        /// The characters of the codes 0x80 to 0xFF.
        fn high(self) -> &'static [char; 128] {
            static TABLES: [OnceLock<[char; 128]>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
            TABLES[self as usize].get_or_init(|| {
                let bytes: Vec<u8> = (0x80..=0xFF).collect();
                let s = self.decode(&bytes);
                let mut out = ['?'; 128];
                for (o, c) in out.iter_mut().zip(s.chars()) {
                    *o = c;
                }
                out
            })
        }

        /// Decode; never fails (module notes).
        pub fn decode(self, bytes: &[u8]) -> String {
            self.encoding().decode_without_bom_handling(bytes).0.into_owned()
        }

        /// Decode a zero-terminated string: stops at the first NUL.
        pub fn decode_z(self, bytes: &[u8]) -> String {
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            self.decode(&bytes[..end])
        }

        /// Encode; None if a character has no code in this code page.
        pub fn encode(self, s: &str) -> Option<Vec<u8>> {
            let (out, _, unmappable) = self.encoding().encode(s);
            (!unmappable).then(|| out.into_owned())
        }

        /// The character of one code.
        pub fn char_of(self, code: u8) -> char {
            if code < 0x80 {
                code as char
            } else {
                self.high()[code as usize - 0x80]
            }
        }

        /// The code of one character; None if it has none.
        pub fn code_of(self, c: char) -> Option<u8> {
            if (c as u32) < 0x80 {
                return Some(c as u8);
            }
            self.high().iter().position(|&h| h == c).map(|i| (i + 0x80) as u8)
        }

        /// Whether every character of `s` has a code.
        pub fn encodable(self, s: &str) -> bool {
            s.chars().all(|c| self.code_of(c).is_some())
        }
    }

    impl std::fmt::Display for CodePage {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "windows-{}", self.number())
        }
    }

    static CURRENT: AtomicU8 = AtomicU8::new(0);

    /// The environment variable that names the code page (over the INI).
    pub const ENV: &str = "NVF_CODE_PAGE";

    /// The code page a run is to use: `NVF_CODE_PAGE` if set, else `ini` (the
    /// `[Text] CodePage` of NVForever.ini) if set, else Windows-1252; Err names a value
    /// that is no code page of [`CodePage::ALL`].
    pub fn chosen(ini: Option<&str>) -> Result<CodePage, String> {
        let env = std::env::var(ENV).ok().filter(|v| !v.trim().is_empty());
        let (name, from) = match (&env, ini.filter(|v| !v.trim().is_empty())) {
            (Some(v), _) => (v.as_str(), ENV),
            (None, Some(v)) => (v, "[Text] CodePage"),
            (None, None) => return Ok(CodePage::default()),
        };
        CodePage::from_name(name).ok_or_else(|| format!("{from}={name} is not a code page NVForever reads (1252, 1251 or 1250)"))
    }

    /// The code page of this run's game text.
    pub fn code_page() -> CodePage {
        CodePage::ALL[CURRENT.load(Ordering::Relaxed) as usize % CodePage::ALL.len()]
    }

    /// Set the code page of this run's game text; done once at the start, before any
    /// text is read (module notes).
    pub fn set_code_page(c: CodePage) {
        CURRENT.store(c as u8, Ordering::Relaxed);
    }

    /// Decode in this run's code page.
    pub fn decode(bytes: &[u8]) -> String {
        code_page().decode(bytes)
    }

    /// Decode a zero-terminated string in this run's code page.
    pub fn decode_z(bytes: &[u8]) -> String {
        code_page().decode_z(bytes)
    }

    /// Encode in this run's code page; None if a character has no code in it.
    pub fn encode(s: &str) -> Option<Vec<u8>> {
        code_page().encode(s)
    }

    /// Whether every character of `s` has a code in this run's code page.
    pub fn encodable(s: &str) -> bool {
        code_page().encodable(s)
    }
}

pub mod sha256 {
    use sha2::{Digest, Sha256};

    pub fn digest(data: &[u8]) -> [u8; 32] {
        Sha256::digest(data).into()
    }

    pub fn hex(data: &[u8]) -> String {
        to_hex(&digest(data))
    }

    pub fn to_hex(b: &[u8]) -> String {
        hex::encode(b)
    }
}

/// Bounds-checked little-endian reader over a byte slice.
#[derive(Clone)]
pub struct Cursor<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("read of {wanted} bytes at offset {pos} exceeds length {len}")]
pub struct OutOfBounds {
    pub pos: usize,
    pub wanted: usize,
    pub len: usize,
}

macro_rules! rd {
    ($name:ident, $t:ty, $n:expr) => {
        pub fn $name(&mut self) -> Result<$t, OutOfBounds> {
            let b = self.bytes($n)?;
            Ok(<$t>::from_le_bytes(b.try_into().unwrap()))
        }
    };
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], OutOfBounds> {
        if self.pos.checked_add(n).is_none_or(|e| e > self.data.len()) {
            return Err(OutOfBounds { pos: self.pos, wanted: n, len: self.data.len() });
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn tag(&mut self) -> Result<[u8; 4], OutOfBounds> {
        Ok(self.bytes(4)?.try_into().unwrap())
    }
    rd!(u8, u8, 1);
    rd!(i8, i8, 1);
    rd!(u16, u16, 2);
    rd!(i16, i16, 2);
    rd!(u32, u32, 4);
    rd!(i32, i32, 4);
    rd!(u64, u64, 8);
    rd!(i64, i64, 8);
    rd!(f32, f32, 4);
    rd!(f64, f64, 8);
}

/// Printable form of a 4-byte signature; non-printable bytes are hex-escaped.
pub fn tag_str(t: &[u8; 4]) -> String {
    let mut s = String::new();
    for &b in t {
        if (0x20..0x7F).contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("\\x{b:02x}"));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_vectors() {
        // FIPS 180-4 / NIST example vectors.
        assert_eq!(sha256::hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256::hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256::hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256::hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn cp1252_roundtrip() {
        let all: Vec<u8> = (0u8..=255).collect();
        let s = cp1252::decode(&all);
        assert_eq!(cp1252::encode(&s).unwrap(), all);
        assert_eq!(cp1252::decode(&[0x80, 0x92, 0xE9]), "\u{20AC}\u{2019}\u{E9}");
        assert_eq!(cp1252::decode_z(b"abc\0def"), "abc");
    }

    #[test]
    fn every_code_page_roundtrips_all_bytes() {
        let all: Vec<u8> = (0u8..=255).collect();
        for cp in text::CodePage::ALL {
            let s = cp.decode(&all);
            assert_eq!(s.chars().count(), 256, "{cp}");
            assert_eq!(cp.encode(&s).unwrap(), all, "{cp}");
            for b in 0u8..=255 {
                let c = cp.char_of(b);
                assert_eq!(cp.code_of(c), Some(b), "{cp} {b:#04x}");
                assert_eq!(cp.decode(&[b]), c.to_string(), "{cp} {b:#04x}");
            }
        }
    }

    #[test]
    fn code_pages_place_their_letters() {
        use text::CodePage::*;
        // Windows-1251: А..я at 0xC0..0xFF, Ё 0xA8, ё 0xB8.
        assert_eq!(Windows1251.decode(&[0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2]), "Привет");
        assert_eq!(Windows1251.encode("Ёё").unwrap(), [0xA8, 0xB8]);
        assert_eq!(Windows1251.code_of('Я'), Some(0xDF));
        // Windows-1250: Ł 0xA3, ą 0xB9, ř 0xF8, Š 0x8A.
        assert_eq!(Windows1250.decode(&[0xA3, 0xB9, 0xF8, 0x8A]), "ŁąřŠ");
        assert_eq!(Windows1250.code_of('ł'), Some(0xB3));
        // Windows-1252 unchanged: the curly apostrophe and é.
        assert_eq!(Windows1252.decode(b"Ain\x92t caf\xe9"), "Ain\u{2019}t caf\u{e9}");
        // A letter of another code page has no code.
        assert_eq!(Windows1252.code_of('Я'), None);
        assert_eq!(Windows1251.code_of('é'), None);
        assert!(!Windows1252.encodable("Привет") && Windows1251.encodable("Привет, Doc!"));
        assert_eq!(Windows1250.code_of('\u{4E2D}'), None);
    }

    #[test]
    fn code_page_names() {
        use text::CodePage::*;
        for (name, cp) in [("1251", Windows1251), ("cp1250", Windows1250), ("Windows-1252", Windows1252), ("windows1251", Windows1251), (" CP1251 ", Windows1251)] {
            assert_eq!(text::CodePage::from_name(name), Some(cp), "{name}");
        }
        assert_eq!(text::CodePage::from_name("utf-8"), None);
        assert_eq!(text::CodePage::from_name("125"), None);
        assert_eq!(Windows1251.to_string(), "windows-1251");
    }

    #[test]
    fn the_run_code_page_defaults_to_1252_and_can_be_set() {
        // The only test that changes the process's code page; it puts it back.
        assert_eq!(text::code_page(), text::CodePage::Windows1252);
        text::set_code_page(text::CodePage::Windows1251);
        let s = text::decode_z(&[0xC4, 0xEE, 0xEA, 0]);
        let back = text::encode("Док");
        text::set_code_page(text::CodePage::Windows1252);
        assert_eq!(s, "Док");
        assert_eq!(back.unwrap(), [0xC4, 0xEE, 0xEA]);
        assert_eq!(text::decode(&[0xC4]), "\u{C4}");
    }

    proptest::proptest! {
        #[test]
        fn cp1252_bytes_roundtrip(b in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..300)) {
            proptest::prop_assert_eq!(cp1252::encode(&cp1252::decode(&b)), Some(b));
        }

        #[test]
        fn cp1252_text_roundtrip(s in "\\PC{0,50}") {
            if let Some(b) = cp1252::encode(&s) {
                proptest::prop_assert_eq!(cp1252::decode(&b), s);
            }
        }
    }

    #[test]
    fn cursor_bounds() {
        let mut c = Cursor::new(&[1, 0, 0, 0, 2]);
        assert_eq!(c.u32().unwrap(), 1);
        assert!(c.u32().is_err());
        assert_eq!(c.u8().unwrap(), 2);
    }
}
