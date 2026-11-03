//! Game settings (GMST): named engine constants authored in the plugins.
//!
//! GMST DATA is a union decided by the first letter of the Editor ID: `f` → f32,
//! `i` → i32, `s` → NUL-terminated string [documented: xEdit wbDefinitionsFNV.pas GMST;
//! observed: all 459 `f` and 61 `i` settings in the masters are 4 bytes]. The schema
//! keeps DATA as bytes (schema v1 has no unions) and this module interprets them.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Setting {
    Float(f32),
    Int(i32),
    Str(String),
    /// A prefix or size the rule above does not cover (reported by the loader).
    Raw(Vec<u8>),
}

impl Setting {
    /// Interpret DATA bytes for a setting with this Editor ID.
    pub fn from_bytes(edid: &str, data: &[u8]) -> Setting {
        match (edid.as_bytes().first(), data.len()) {
            (Some(b'f'), 4) => Setting::Float(f32::from_le_bytes(data.try_into().expect("4 bytes"))),
            (Some(b'i'), 4) => Setting::Int(i32::from_le_bytes(data.try_into().expect("4 bytes"))),
            (Some(b's'), _) => Setting::Str(fnv_util::text::decode_z(data)),
            _ => Setting::Raw(data.to_vec()),
        }
    }

    pub fn as_f32(&self) -> Option<f32> {
        match *self {
            Setting::Float(v) => Some(v),
            Setting::Int(v) => Some(v as f32),
            _ => None,
        }
    }
}

/// Parse the lower-case hex the schema decoder prints for `bytes` fields.
pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_by_prefix() {
        assert_eq!(Setting::from_bytes("fAVDCarryWeightsBase", &150f32.to_le_bytes()), Setting::Float(150.0));
        assert_eq!(Setting::from_bytes("iLevelCap", &30i32.to_le_bytes()), Setting::Int(30));
        assert_eq!(Setting::from_bytes("sOk", b"Ok\0"), Setting::Str("Ok".into()));
        assert_eq!(Setting::from_bytes("bX", &[1]), Setting::Raw(vec![1]));
        assert_eq!(Setting::from_bytes("fShort", &[0, 0]), Setting::Raw(vec![0, 0]));
        assert_eq!(unhex("00001643"), Some(vec![0, 0, 0x16, 0x43]));
        assert_eq!(unhex("0"), None);
        assert_eq!(unhex("zz"), None);
        assert_eq!(Setting::Int(3).as_f32(), Some(3.0));
    }
}
