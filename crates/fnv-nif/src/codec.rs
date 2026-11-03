//! Schema interpreter: decode block bytes into a [`Struct`] and encode it back.
//!
//! Encoding exists to prove the decoder: a block that decodes, consumes exactly its
//! stored size and re-encodes to the same bytes is understood field by field.

use crate::expr::Scope;
use crate::header::{RawString, V20_1_0_3};
use crate::schema::{Field, Prim, Schema, TypeRef};
use crate::value::{FieldScope, Struct, Value};
use fnv_util::Cursor;

/// Error while decoding or encoding, with the field path and (for decoding) the offset
/// within the block.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{path}: {msg}{}", .offset.map(|o| format!(" (block offset {o})")).unwrap_or_default())]
pub struct CodecError {
    pub path: String,
    pub msg: String,
    pub offset: Option<usize>,
}

/// Arrays longer than this are rejected as corrupt counts.
const MAX_ELEMENTS: i64 = 1 << 26;

/// Elements (and 2D rows) that take no bytes one array may hold: a structure whose
/// fields are all left out in this version, a row of width 0. Every other element takes
/// at least a byte, so an array is at most the bytes left plus this many long; a broken
/// count past that would build millions of empty values from a few bytes.
const MAX_EMPTY: usize = 1 << 16;

/// Everything about the file that decoding depends on.
#[derive(Clone, Copy)]
pub struct FileCtx<'a> {
    pub version: u32,
    pub user_version: u32,
    pub bs_version: u32,
    pub strings: Option<&'a [RawString]>,
}

pub struct Codec<'a> {
    pub schema: &'a Schema,
    pub file: FileCtx<'a>,
    /// Concrete block type and its ancestors (for `only` / `exclude`).
    pub lineage: &'a [String],
}

impl<'a> Codec<'a> {
    fn scope<'s>(&self, fields: &'s [(std::sync::Arc<str>, Value)], arg: i64) -> FieldScope<'s> {
        FieldScope {
            fields,
            version: self.file.version,
            user_version: self.file.user_version,
            bs_version: self.file.bs_version,
            arg,
        }
    }

    fn present(&self, f: &Field, scope: &dyn Scope, path: &str) -> Result<bool, CodecError> {
        let v = self.file.version;
        if f.since.is_some_and(|s| v < s) || f.until.is_some_and(|u| v > u) {
            return Ok(false);
        }
        if !f.only.is_empty() && !f.only.iter().any(|t| self.lineage.contains(t)) {
            return Ok(false);
        }
        if f.exclude.iter().any(|t| self.lineage.contains(t)) {
            return Ok(false);
        }
        match &f.cond {
            None => Ok(true),
            Some(e) => e
                .eval(scope)
                .map(|x| x != 0)
                .map_err(|e| err(path, format!("cond: {e}"), None)),
        }
    }

    fn resolve<'t>(&self, ty: &'t TypeRef, template: Option<&'t TypeRef>, path: &str) -> Result<&'t TypeRef, CodecError> {
        match ty {
            TypeRef::Template => template.ok_or_else(|| err(path, "T used without a template".into(), None)),
            t => Ok(t),
        }
    }

    // ---------------------------------------------------------------- decoding

    pub fn decode_fields(
        &self,
        fields: &[Field],
        c: &mut Cursor<'_>,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
    ) -> Result<Struct, CodecError> {
        let mut st = Struct::default();
        for f in fields {
            let fpath = format!("{path}.{}", f.name);
            let scope = self.scope(&st.fields, arg);
            if !self.present(f, &scope, &fpath)? {
                continue;
            }
            let ty = self.resolve(&f.ty, template, &fpath)?;
            let sub_template = match &f.template {
                Some(TypeRef::Template) => template,
                Some(t) => Some(t),
                None => None,
            };
            let sub_arg = match &f.arg {
                Some(e) => e.eval(&scope).map_err(|e| err(&fpath, format!("arg: {e}"), Some(c.pos)))?,
                None => 0,
            };
            let v = match &f.length {
                None => self.read_one(ty, c, sub_arg, sub_template, &fpath)?,
                Some(len) => {
                    let n = len.eval(&scope).map_err(|e| err(&fpath, format!("length: {e}"), Some(c.pos)))?;
                    match &f.width {
                        None => self.read_array(ty, n, c, sub_arg, sub_template, &fpath)?,
                        Some(w) => {
                            let widths = self.widths(w, n, &scope, &fpath, c.pos, c.remaining())?;
                            let mut rows = Vec::with_capacity(widths.len());
                            for (i, w) in widths.into_iter().enumerate() {
                                rows.push(self.read_array(ty, w, c, sub_arg, sub_template, &format!("{fpath}[{i}]"))?);
                            }
                            Value::List(rows)
                        }
                    }
                }
            };
            st.fields.push((f.name.clone(), v));
        }
        Ok(st)
    }

    /// Row widths of a 2D array: one per element of a named integer array, or the same
    /// scalar for every row.
    fn widths(
        &self,
        w: &crate::expr::Expr,
        rows: i64,
        scope: &FieldScope<'_>,
        path: &str,
        pos: usize,
        remaining: usize,
    ) -> Result<Vec<i64>, CodecError> {
        check_count(rows, path, pos)?;
        if rows as usize > remaining.saturating_add(MAX_EMPTY) {
            return Err(err(path, format!("{rows} rows exceed the {remaining} bytes left"), Some(pos)));
        }
        if let Some(name) = w.as_ident() {
            if let Some((_, Value::IntList(v))) = scope.fields.iter().rev().find(|(n, _)| &**n == name) {
                if v.len() as i64 != rows {
                    return Err(err(path, format!("{} row widths for {rows} rows", v.len()), Some(pos)));
                }
                return Ok(v.clone());
            }
        }
        let n = w.eval(scope).map_err(|e| err(path, format!("width: {e}"), Some(pos)))?;
        Ok(vec![n; rows as usize])
    }

    fn read_array(
        &self,
        ty: &TypeRef,
        n: i64,
        c: &mut Cursor<'_>,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
    ) -> Result<Value, CodecError> {
        check_count(n, path, c.pos)?;
        let n = n as usize;
        let min = match ty {
            TypeRef::Prim(p) | TypeRef::Enum { storage: p, .. } => p.min_size(),
            _ => 0,
        };
        if min > 0 && n.saturating_mul(min) > c.remaining() {
            return Err(err(path, format!("{n} elements of at least {min} bytes exceed the {} bytes left", c.remaining()), Some(c.pos)));
        }
        let at = c.pos;
        let oob = |e: fnv_util::OutOfBounds| err(path, e.to_string(), Some(at));
        match ty {
            TypeRef::Prim(Prim::Bytes) => Ok(Value::Bytes(c.bytes(n).map_err(oob)?.to_vec())),
            TypeRef::Prim(p) if p.floats().is_some() => {
                let dim = p.floats().unwrap();
                let mut data = Vec::with_capacity(n * dim);
                for _ in 0..n * dim {
                    data.push(c.f32().map_err(oob)?);
                }
                Ok(Value::FloatList { dim: dim as u8, data })
            }
            TypeRef::Prim(Prim::Ref | Prim::Ptr) => {
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(c.i32().map_err(oob)?);
                }
                Ok(Value::RefList(v))
            }
            TypeRef::Prim(p @ (Prim::Triangle | Prim::ByteColor4)) => {
                let mut v = Vec::with_capacity(n * 4);
                for _ in 0..n {
                    push_ints(*p, c, &mut v).map_err(oob)?;
                }
                Ok(Value::IntList(v))
            }
            TypeRef::Prim(p) | TypeRef::Enum { storage: p, .. } if p.int_size().is_some() => {
                let mut v = Vec::with_capacity(n);
                for _ in 0..n {
                    v.push(read_int(*p, c).map_err(oob)?);
                }
                Ok(Value::IntList(v))
            }
            _ => {
                if n > c.remaining().saturating_add(MAX_EMPTY) {
                    return Err(err(path, format!("{n} elements exceed the {} bytes left", c.remaining()), Some(c.pos)));
                }
                let mut v = Vec::with_capacity(n.min(4096));
                let mut empty = 0;
                for i in 0..n {
                    let before = c.pos;
                    v.push(self.read_one(ty, c, arg, template, &format!("{path}[{i}]"))?);
                    if c.pos == before {
                        empty += 1;
                        if empty > MAX_EMPTY {
                            return Err(err(path, format!("more than {MAX_EMPTY} elements of no bytes"), Some(c.pos)));
                        }
                    }
                }
                Ok(Value::List(v))
            }
        }
    }

    fn read_one(
        &self,
        ty: &TypeRef,
        c: &mut Cursor<'_>,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
    ) -> Result<Value, CodecError> {
        let at = c.pos;
        let oob = |e: fnv_util::OutOfBounds| err(path, e.to_string(), Some(at));
        Ok(match ty {
            TypeRef::Struct(name) => {
                let def = self.schema.structs.get(name).ok_or_else(|| err(path, format!("unknown struct {name}"), None))?;
                Value::Struct(self.decode_fields(&def.fields, c, arg, template, path)?)
            }
            TypeRef::Template => return Err(err(path, "unresolved template".into(), None)),
            TypeRef::Enum { storage, .. } => Value::Int(read_int(*storage, c).map_err(oob)?),
            TypeRef::Prim(p) => match p {
                Prim::Bool => {
                    let b = c.u8().map_err(oob)?;
                    if b <= 1 {
                        Value::Bool(b == 1)
                    } else {
                        Value::Int(b as i64)
                    }
                }
                Prim::Ref => Value::Ref(c.i32().map_err(oob)?),
                Prim::Ptr => Value::Ptr(c.i32().map_err(oob)?),
                Prim::F32 => Value::Float(c.f32().map_err(oob)?),
                Prim::String if self.file.version >= V20_1_0_3 => {
                    let idx = c.u32().map_err(oob)?;
                    let text = self.file.strings.and_then(|s| s.get(idx as usize)).map(RawString::text);
                    Value::Str { index: Some(idx), text }
                }
                Prim::String | Prim::SizedString => {
                    let n = c.u32().map_err(oob)? as usize;
                    let b = c.bytes(n).map_err(oob)?;
                    Value::Str { index: None, text: Some(fnv_util::text::decode(b)) }
                }
                Prim::Bytes => return Err(err(path, "bytes without length".into(), None)),
                Prim::Triangle | Prim::ByteColor4 => {
                    let mut v = Vec::with_capacity(4);
                    push_ints(*p, c, &mut v).map_err(oob)?;
                    Value::IntList(v)
                }
                p if p.floats().is_some() => {
                    let mut v = Vec::with_capacity(p.floats().unwrap());
                    for _ in 0..p.floats().unwrap() {
                        v.push(c.f32().map_err(oob)?);
                    }
                    Value::Floats(v)
                }
                p => Value::Int(read_int(*p, c).map_err(oob)?),
            },
        })
    }

    // ---------------------------------------------------------------- encoding

    pub fn encode_fields(
        &self,
        fields: &[Field],
        st: &Struct,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
        out: &mut Vec<u8>,
    ) -> Result<(), CodecError> {
        let mut idx = 0;
        for f in fields {
            let fpath = format!("{path}.{}", f.name);
            let scope = self.scope(&st.fields[..idx], arg);
            if !self.present(f, &scope, &fpath)? {
                continue;
            }
            let Some((name, v)) = st.fields.get(idx) else {
                return Err(err(&fpath, "missing from the value".into(), None));
            };
            if **name != *f.name {
                return Err(err(&fpath, format!("value has {name} here"), None));
            }
            let ty = self.resolve(&f.ty, template, &fpath)?;
            let sub_template = match &f.template {
                Some(TypeRef::Template) => template,
                Some(t) => Some(t),
                None => None,
            };
            let sub_arg = match &f.arg {
                Some(e) => e.eval(&scope).map_err(|e| err(&fpath, format!("arg: {e}"), None))?,
                None => 0,
            };
            match (&f.length, &f.width) {
                (None, _) => self.write_one(ty, v, sub_arg, sub_template, &fpath, out)?,
                (Some(_), None) => self.write_array(ty, v, sub_arg, sub_template, &fpath, out)?,
                (Some(_), Some(_)) => {
                    let Value::List(rows) = v else { return Err(err(&fpath, "2D array is not a list".into(), None)) };
                    for (i, r) in rows.iter().enumerate() {
                        self.write_array(ty, r, sub_arg, sub_template, &format!("{fpath}[{i}]"), out)?;
                    }
                }
            }
            idx += 1;
        }
        if idx != st.fields.len() {
            return Err(err(path, format!("{} extra fields in the value", st.fields.len() - idx), None));
        }
        Ok(())
    }

    fn write_array(
        &self,
        ty: &TypeRef,
        v: &Value,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
        out: &mut Vec<u8>,
    ) -> Result<(), CodecError> {
        match (ty, v) {
            (TypeRef::Prim(Prim::Bytes), Value::Bytes(b)) => out.extend_from_slice(b),
            (TypeRef::Prim(p), Value::FloatList { data, .. }) if p.floats().is_some() => {
                for f in data {
                    out.extend_from_slice(&f.to_le_bytes());
                }
            }
            (TypeRef::Prim(Prim::Ref | Prim::Ptr), Value::RefList(r)) => {
                for x in r {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
            (TypeRef::Prim(Prim::Triangle), Value::IntList(xs)) => {
                for x in xs {
                    out.extend_from_slice(&(*x as u16).to_le_bytes());
                }
            }
            (TypeRef::Prim(Prim::ByteColor4), Value::IntList(xs)) => {
                for x in xs {
                    out.push(*x as u8);
                }
            }
            (TypeRef::Prim(p) | TypeRef::Enum { storage: p, .. }, Value::IntList(xs)) if p.int_size().is_some() => {
                for x in xs {
                    write_int(*p, *x, out);
                }
            }
            (_, Value::List(items)) => {
                for (i, it) in items.iter().enumerate() {
                    self.write_one(ty, it, arg, template, &format!("{path}[{i}]"), out)?;
                }
            }
            _ => return Err(err(path, format!("array value {v:?} does not fit {ty:?}"), None)),
        }
        Ok(())
    }

    fn write_one(
        &self,
        ty: &TypeRef,
        v: &Value,
        arg: i64,
        template: Option<&TypeRef>,
        path: &str,
        out: &mut Vec<u8>,
    ) -> Result<(), CodecError> {
        let mismatch = || err(path, format!("value {v:?} does not fit {ty:?}"), None);
        match ty {
            TypeRef::Struct(name) => {
                let def = self.schema.structs.get(name).ok_or_else(|| err(path, format!("unknown struct {name}"), None))?;
                let Value::Struct(s) = v else { return Err(mismatch()) };
                self.encode_fields(&def.fields, s, arg, template, path, out)?;
            }
            TypeRef::Template => return Err(err(path, "unresolved template".into(), None)),
            TypeRef::Enum { storage, .. } => write_int(*storage, v.as_int().ok_or_else(mismatch)?, out),
            TypeRef::Prim(p) => match (p, v) {
                (Prim::F32, Value::Float(f)) => out.extend_from_slice(&f.to_le_bytes()),
                (Prim::Ref | Prim::Ptr, Value::Ref(r) | Value::Ptr(r)) => out.extend_from_slice(&r.to_le_bytes()),
                (Prim::String, Value::Str { index: Some(i), .. }) if self.file.version >= V20_1_0_3 => {
                    out.extend_from_slice(&i.to_le_bytes())
                }
                (Prim::String | Prim::SizedString, Value::Str { index: None, text: Some(t) }) => {
                    let b = fnv_util::text::encode(t).ok_or_else(|| err(path, "text not encodable".into(), None))?;
                    out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                    out.extend_from_slice(&b);
                }
                (Prim::Triangle, Value::IntList(xs)) => {
                    for x in xs {
                        out.extend_from_slice(&(*x as u16).to_le_bytes());
                    }
                }
                (Prim::ByteColor4, Value::IntList(xs)) => {
                    for x in xs {
                        out.push(*x as u8);
                    }
                }
                (p, Value::Floats(fs)) if p.floats().is_some() => {
                    for f in fs {
                        out.extend_from_slice(&f.to_le_bytes());
                    }
                }
                (p, v) if p.int_size().is_some() => write_int(*p, v.as_int().ok_or_else(mismatch)?, out),
                _ => return Err(mismatch()),
            },
        }
        Ok(())
    }
}

fn err(path: &str, msg: String, offset: Option<usize>) -> CodecError {
    CodecError { path: path.to_string(), msg, offset }
}

fn check_count(n: i64, path: &str, pos: usize) -> Result<(), CodecError> {
    if !(0..=MAX_ELEMENTS).contains(&n) {
        return Err(err(path, format!("implausible element count {n}"), Some(pos)));
    }
    Ok(())
}

fn read_int(p: Prim, c: &mut Cursor<'_>) -> Result<i64, fnv_util::OutOfBounds> {
    Ok(match p {
        Prim::U8 | Prim::Bool => c.u8()? as i64,
        Prim::I8 => c.i8()? as i64,
        Prim::U16 => c.u16()? as i64,
        Prim::I16 => c.i16()? as i64,
        Prim::U32 => c.u32()? as i64,
        Prim::I32 | Prim::Ref | Prim::Ptr => c.i32()? as i64,
        Prim::U64 => c.u64()? as i64,
        _ => unreachable!("not an integer primitive"),
    })
}

fn push_ints(p: Prim, c: &mut Cursor<'_>, v: &mut Vec<i64>) -> Result<(), fnv_util::OutOfBounds> {
    match p {
        Prim::Triangle => {
            for _ in 0..3 {
                v.push(c.u16()? as i64);
            }
        }
        Prim::ByteColor4 => {
            for _ in 0..4 {
                v.push(c.u8()? as i64);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn write_int(p: Prim, x: i64, out: &mut Vec<u8>) {
    match p {
        Prim::U8 | Prim::I8 | Prim::Bool => out.push(x as u8),
        Prim::U16 | Prim::I16 => out.extend_from_slice(&(x as u16).to_le_bytes()),
        Prim::U32 | Prim::I32 | Prim::Ref | Prim::Ptr => out.extend_from_slice(&(x as u32).to_le_bytes()),
        Prim::U64 => out.extend_from_slice(&(x as u64).to_le_bytes()),
        _ => unreachable!("not an integer primitive"),
    }
}
