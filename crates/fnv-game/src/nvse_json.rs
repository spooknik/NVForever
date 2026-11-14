//! SUP's JSON commands (#2326, batch 2; docs/spec-notes/ext-unlisted-rest.md §6): a JSON
//! file under the game folder, read whole into a cache or changed on disk, by top-level
//! key. Files a mod writes go to NVForever's own layer, never the game folder
//! (`crate::mod_files`, install-layout.md). Behaviour from probes in the original (sets
//! json1, json2 and json2r0-r5 of tools/capture/unlisted_rest_probes.py, run 2026-11-13;
//! every comment cites the spec note's rows).

use crate::game::Game;
use fnv_script::vm::{ArrayKind, XKey, XVal};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// The cached files, by lower-case path.
#[derive(Debug, Default)]
pub struct JsonState {
    pub cache: BTreeMap<String, Value>,
}

/// No such file, or (with the cache chosen) a file not cached [observed: json1.cache.missing,
/// json2.uncached.set.cache].
const NO_FILE: f64 = -4.0;
/// Text that is not JSON [observed: json2.fromstring with escaped quotes; hypothesis: the
/// code means a parse error].
const BAD_JSON: f64 = -3.0;
/// A key the object does not hold [observed: json1.type.nokey].
const NO_KEY: f64 = -2.0;

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        Some(XVal::Num(n)) => format!("{n}"),
        _ => String::new(),
    }
}

fn num(args: &[XVal], k: usize) -> f64 {
    match args.get(k) {
        Some(XVal::Num(n)) => *n,
        _ => 0.0,
    }
}

fn key_of(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}

/// The file's text as written: indented by four spaces, keys in order [observed: the
/// probe file after the setters with 0].
fn pretty(v: &Value) -> String {
    let mut out = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
    if serde::Serialize::serialize(v, &mut ser).is_err() {
        return String::new();
    }
    String::from_utf8(out).unwrap_or_default()
}

/// The value with every object's keys in order, as the original keeps them [observed:
/// json1.tostring, keys sorted whatever the file's order].
fn sorted(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut pairs: Vec<(String, Value)> = m.into_iter().map(|(k, x)| (k, sorted(x))).collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(pairs.into_iter().collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        x => x,
    }
}

/// The type code of a value [observed: json1.type.*: object 1, array 2, string 3, number
/// 5; hypothesis: a boolean 4, null 0].
fn type_code(v: &Value) -> f64 {
    match v {
        Value::Object(_) => 1.0,
        Value::Array(_) => 2.0,
        Value::String(_) => 3.0,
        Value::Bool(_) => 4.0,
        Value::Number(_) => 5.0,
        Value::Null => 0.0,
    }
}

fn to_xval(g: &mut Game, v: &Value) -> XVal {
    match v {
        Value::Number(n) => XVal::Num(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => XVal::Str(s.clone()),
        Value::Bool(b) => XVal::Num(f64::from(u8::from(*b))),
        Value::Null => XVal::Num(0.0),
        Value::Array(a) => {
            let entries = a.iter().enumerate().map(|(i, x)| (XKey::Num(i as f64), to_xval(g, x))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::List, entries))
        }
        Value::Object(m) => {
            let entries = m.iter().map(|(k, x)| (XKey::Str(k.clone()), to_xval(g, x))).collect();
            XVal::Array(g.ext.new_array(ArrayKind::StringMap, entries))
        }
    }
}

/// The document a command works on: with `cached` the cache's copy (none when the file is
/// not cached), else the file as it is now.
fn document(g: &Game, path: &str, cached: bool) -> Result<Value, f64> {
    if cached {
        return g.other.json.cache.get(&key_of(path)).cloned().ok_or(NO_FILE);
    }
    let t = g.mod_files.read_root_text(path).ok_or(NO_FILE)?;
    serde_json::from_str(&t).map(sorted).map_err(|_| BAD_JSON)
}

/// Keep a changed document: in the cache, or (not `cached`) written to the file.
fn store(g: &mut Game, path: &str, cached: bool, v: Value) -> f64 {
    let v = sorted(v);
    if cached {
        g.other.json.cache.insert(key_of(path), v);
        return 1.0;
    }
    if g.mod_files.write_root_text(path, pretty(&v)) { 1.0 } else { NO_FILE }
}

/// Change the top-level object of a document; 1 when kept.
fn change(g: &mut Game, path: &str, cached: bool, f: impl FnOnce(&mut Map<String, Value>)) -> f64 {
    let mut doc = match document(g, path, cached) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let Value::Object(m) = &mut doc else { return BAD_JSON };
    f(m);
    store(g, path, cached, doc)
}

/// The functions of this file; `None` for any other name.
pub(crate) fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let path = text(args, 0);
    let v = match name {
        // `path flag`: the file read into the cache, 0; -4 for no such file
        // [observed: json1.cache, cache.data ("Data\\…" under the game folder), cache.missing].
        "jsoncachefile" => {
            let code = match document(g, &path, false) {
                Ok(doc) => {
                    g.other.json.cache.insert(key_of(&path), doc);
                    0.0
                }
                Err(code) => code,
            };
            XVal::Num(code)
        }
        // `text name`: the text cached as if it were that file; -3 for text that is not
        // JSON [observed: json2.fromstring, fromstring.swap -3; hypothesis: the order].
        "jsoncachefilefromstring" => XVal::Num(match serde_json::from_str::<Value>(&path) {
            Ok(doc) => {
                g.other.json.cache.insert(key_of(&text(args, 1)), sorted(doc));
                0.0
            }
            Err(_) => BAD_JSON,
        }),
        "jsonisfilecached" => XVal::Num(f64::from(u8::from(g.other.json.cache.contains_key(&key_of(&path))))),
        // The cache's copy in one line; "" when not cached [observed: json1.tostring,
        // json2.json2r5.after after a close].
        "jsontostring" => XVal::Str(g.other.json.cache.get(&key_of(&path)).map(|v| v.to_string()).unwrap_or_default()),
        // `path flag`: 1 drops the cache's copy and gives 0; 0 gives -5 and keeps it
        // [observed: json2.close.1, close.0].
        "jsonclosefile" => XVal::Num(if num(args, 1) != 0.0 {
            g.other.json.cache.remove(&key_of(&path));
            0.0
        } else {
            -5.0
        }),
        // `path key flag`: the type code of a top-level key; nested paths are not keys
        // [observed: json1.type.*, json2.type.*: "obj/inner", "obj.inner", "/obj/inner"
        // all -2].
        "jsongetvaluetype" => XVal::Num(match document(g, &path, num(args, 2) != 0.0) {
            Ok(Value::Object(m)) => m.get(&text(args, 1)).map_or(NO_KEY, type_code),
            Ok(_) => NO_KEY,
            Err(code) => code,
        }),
        // `path key flag`: the key's array as an array [observed: json1.arr.get, an array
        // of 3; hypothesis: its elements, which read back neither as numbers nor as
        // strings in the original, and an object's as a string map].
        "jsongetvaluearray" => match document(g, &path, num(args, 2) != 0.0) {
            Ok(Value::Object(m)) => match m.get(&text(args, 1)).cloned() {
                Some(v @ (Value::Array(_) | Value::Object(_))) => to_xval(g, &v),
                _ => XVal::Array(0),
            },
            _ => XVal::Array(0),
        },
        // `path key value flag`: 0 the file on disk, 1 the cache's copy; 1 when kept
        // [observed: json1.setfloat with 0 changed the file and not the cache,
        // setfloat.flag1 the cache; json2.uncached.set.cache -4]. A float is written as
        // the number [decision: the original wrote `true` for some (json2.set.cache 2)].
        "jsonsetvaluefloat" | "jsonsetvaluestring" | "jsonappendvaluefloat" | "jsonappendvaluestring" => {
            let key = text(args, 1);
            let value = if name.ends_with("float") { serde_json::json!(num(args, 2)) } else { Value::String(text(args, 2)) };
            let append = name.starts_with("jsonappend");
            XVal::Num(change(g, &path, num(args, 3) != 0.0, |m| {
                if append {
                    // Onto the key's array [observed: json1, json2 after.append].
                    match m.get_mut(&key) {
                        Some(Value::Array(a)) => a.push(value),
                        _ => {
                            m.insert(key, Value::Array(vec![value]));
                        }
                    }
                } else {
                    m.insert(key, value);
                }
            }))
        }
        // `path key index flag` [observed: json2.erase.elem.cache, index 0 went].
        "jsonerasearrayelement" => {
            let (key, at) = (text(args, 1), num(args, 2));
            XVal::Num(change(g, &path, num(args, 3) != 0.0, |m| {
                if let Some(Value::Array(a)) = m.get_mut(&key) {
                    if at >= 0.0 && (at as usize) < a.len() {
                        a.remove(at as usize);
                    }
                }
            }))
        }
        // `path key flag`: the key's object or array emptied [observed: json1 and json2,
        // "obj" became {}].
        "jsonclearkey" => {
            let key = text(args, 1);
            XVal::Num(change(g, &path, num(args, 2) != 0.0, |m| match m.get_mut(&key) {
                Some(Value::Object(o)) => o.clear(),
                Some(Value::Array(a)) => a.clear(),
                _ => {}
            }))
        }
        // `path key flag`: the key taken out [decision: the original crashed both ways,
        // json2r0 and json2r1].
        "jsonerasekey" => {
            let key = text(args, 1);
            XVal::Num(change(g, &path, num(args, 2) != 0.0, |m| {
                m.remove(&key);
            }))
        }
        _ => return None,
    };
    Some(Ok(v))
}
