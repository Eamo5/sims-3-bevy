//! The scripts' saved object graph ("OBJS", 0x06B981ED in a world file): every gameplay object
//! of the town as the game's scripts serialized it, including the premade households and their
//! Sims. Layout:
//!
//! ```text
//! u16 version, u16 0, "OBJS", u32 class count, u32 object count,
//! u32 class table offset, u32 object offset table, u32 resource key table
//! objects ... | u32 offset per object | classes ... | u32 count, {u32 type, u32 group, u64 instance}
//! ```
//!
//! A class is a type descriptor (`flags`, name; generic types add their arguments; `0x40`
//! arrays and `0x02` wrappers nest one descriptor) followed by its serialized fields (name and
//! a type code); `0x7F` marks a class without a field list. An object is `0x10, u32 class`
//! and its fields, or `0x11, u32 count, element type` and references for arrays. Object ids in
//! references are 1-based indices into the offset table.

use s3pkg::ResourceKey;

pub const T_OBJS: u32 = 0x06B981ED;

#[derive(Clone, Debug)]
pub struct Class {
    pub name: String,
    pub fields: Vec<(String, u8)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f32),
    /// Object id (1-based; 0 = null).
    Ref(u32),
    /// Enum: its class and value.
    Enum(usize, u64),
    /// Index into the resource key table.
    Key(u32),
    Struct(usize, Vec<Value>),
}

impl Value {
    pub fn as_ref(&self) -> Option<u32> {
        match self {
            Value::Ref(r) if *r != 0 => Some(*r),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Enum(_, v) => Some(*v as i64),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Value::Float(v) => Some(*v),
            _ => None,
        }
    }
}

pub struct ObjStream<'a> {
    d: &'a [u8],
    pub classes: Vec<Class>,
    offsets: Vec<u32>,
    pub keys: Vec<ResourceKey>,
    end: usize,
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap()))
}

fn u64_at(d: &[u8], o: usize) -> Option<u64> {
    d.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap()))
}

/// A one- or two-byte length (high bit of the first byte set: its low 7 bits are the high
/// bits of a 14-bit value).
fn len_at(d: &[u8], o: usize) -> Option<(usize, usize)> {
    let b = *d.get(o)?;
    if b & 0x80 != 0 {
        Some(((((b & 0x7F) as usize) << 7) | *d.get(o + 1)? as usize, o + 2))
    } else {
        Some((b as usize, o + 1))
    }
}

fn str_at(d: &[u8], o: usize) -> Option<(String, usize)> {
    let (n, o) = len_at(d, o)?;
    Some((String::from_utf8_lossy(d.get(o..o + n)?).into_owned(), o + n))
}

fn type_desc(d: &[u8], o: usize) -> Option<(String, usize)> {
    let flag = *d.get(o)?;
    if flag & 0x20 != 0 {
        let (base, mut o) = type_desc(d, o + 1)?;
        let arity: usize = base.split('`').nth(1).and_then(|a| a.parse().ok()).unwrap_or(0);
        let mut args = Vec::new();
        for _ in 0..arity {
            let (a, o2) = type_desc(d, o)?;
            args.push(a);
            o = o2;
        }
        return Some((format!("{base}<{}>", args.join(",")), o));
    }
    if flag & 0x40 != 0 {
        let (t, o) = type_desc(d, o + 1)?;
        return Some((format!("{t}[]"), o));
    }
    if flag & 0x02 != 0 {
        let (t, o) = type_desc(d, o + 1)?;
        return Some((format!("{t}?"), o));
    }
    str_at(d, o + 1)
}

impl<'a> ObjStream<'a> {
    pub fn parse(d: &'a [u8]) -> Option<Self> {
        if d.get(4..8)? != b"OBJS" {
            return None;
        }
        let n_classes = u32_at(d, 8)? as usize;
        let n_objects = u32_at(d, 12)? as usize;
        let types_off = u32_at(d, 16)? as usize;
        let table_off = u32_at(d, 20)? as usize;
        let keys_off = u32_at(d, 24)? as usize;
        let mut classes = Vec::with_capacity(n_classes);
        let mut o = types_off;
        for _ in 0..n_classes {
            let (name, o2) = type_desc(d, o)?;
            o = o2;
            let n_fields = if *d.get(o)? == 0x7F {
                o += 1;
                0
            } else {
                let (n, o2) = len_at(d, o)?;
                o = o2;
                n
            };
            let mut fields = Vec::with_capacity(n_fields);
            for _ in 0..n_fields {
                let (f, o2) = str_at(d, o)?;
                fields.push((f, *d.get(o2)?));
                o = o2 + 1;
            }
            classes.push(Class { name, fields });
        }
        let offsets = (0..n_objects).map(|i| u32_at(d, table_off + 4 * i)).collect::<Option<Vec<_>>>()?;
        let n_keys = u32_at(d, keys_off)? as usize;
        let mut keys = Vec::with_capacity(n_keys);
        for i in 0..n_keys {
            let p = keys_off + 4 + 16 * i;
            keys.push(ResourceKey::new(u32_at(d, p)?, u32_at(d, p + 4)?, u64_at(d, p + 8)?));
        }
        Some(Self { d, classes, offsets, keys, end: table_off })
    }

    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    fn range(&self, id: u32) -> Option<(usize, usize)> {
        let i = (id as usize).checked_sub(1)?;
        let a = *self.offsets.get(i)? as usize;
        let b = self.offsets.get(i + 1).map_or(self.end, |&b| b as usize);
        Some((a, b))
    }

    /// The serialized bytes of an object.
    pub fn raw(&self, id: u32) -> &[u8] {
        self.range(id).and_then(|(a, b)| self.d.get(a..b)).unwrap_or_default()
    }

    /// Class of an instance object.
    pub fn class(&self, id: u32) -> Option<usize> {
        let (a, _) = self.range(id)?;
        (*self.d.get(a)? == 0x10).then(|| u32_at(self.d, a + 1)).flatten().map(|c| c as usize)
    }

    pub fn class_name(&self, id: u32) -> Option<&str> {
        self.classes.get(self.class(id)?).map(|c| c.name.as_str())
    }

    /// Ids of every object of a class.
    pub fn objects_of(&self, class_name: &str) -> Vec<u32> {
        let Some(c) = self.classes.iter().position(|c| c.name == class_name) else { return vec![] };
        (1..=self.offsets.len() as u32).filter(|&id| self.class(id) == Some(c)).collect()
    }

    fn value(&self, t: u8, o: usize) -> Option<(Value, usize)> {
        let d = self.d;
        Some(match t {
            0x01 => (Value::Ref(u32_at(d, o)?), o + 4),
            0x02 => (Value::Bool(*d.get(o)? != 0), o + 1),
            0x03 | 0x04 => (Value::Int(*d.get(o)? as i64), o + 1),
            0x06..=0x08 => (Value::Int(u16::from_le_bytes([*d.get(o)?, *d.get(o + 1)?]) as i64), o + 2),
            0x0b | 0x0d => (Value::Int(u64_at(d, o)? as i64), o + 8),
            0x05 | 0x0a | 0x0f => (Value::Int(u64_at(d, o)? as i64), o + 8),
            0x09 => (Value::Int(u32_at(d, o)? as i32 as i64), o + 4),
            0x0e => (Value::Int(u32_at(d, o)? as i64), o + 4),
            0x0c => (Value::Float(f32::from_bits(u32_at(d, o)?)), o + 4),
            0x19 => (Value::Key(u32_at(d, o)?), o + 4),
            0x10 => {
                let c = u32_at(d, o)? as usize;
                let (vals, o) = self.fields_at(c, o + 4)?;
                (Value::Struct(c, vals), o)
            }
            0x17 => {
                let (c, o) = len_at(d, o)?;
                let vt = *d.get(o)?;
                let (v, o) = self.value(vt, o + 1)?;
                (Value::Enum(c, v.as_int().unwrap_or(0) as u64), o)
            }
            _ => return None,
        })
    }

    fn fields_at(&self, class: usize, mut o: usize) -> Option<(Vec<Value>, usize)> {
        let c = self.classes.get(class)?;
        let mut out = Vec::with_capacity(c.fields.len());
        for (_, t) in &c.fields {
            let (v, o2) = self.value(*t, o)?;
            out.push(v);
            o = o2;
        }
        Some((out, o))
    }

    /// The fields of an instance object, by name.
    pub fn fields(&self, id: u32) -> Option<Vec<(&str, Value)>> {
        let (a, _) = self.range(id)?;
        let c = self.class(id)?;
        let (vals, _) = self.fields_at(c, a + 5)?;
        Some(self.classes[c].fields.iter().map(|f| f.0.as_str()).zip(vals).collect())
    }

    pub fn field(&self, id: u32, name: &str) -> Option<Value> {
        self.fields(id)?.into_iter().find(|f| f.0 == name).map(|f| f.1)
    }

    pub fn field_ref(&self, id: u32, name: &str) -> Option<u32> {
        self.field(id, name)?.as_ref()
    }

    pub fn string(&self, id: u32) -> Option<String> {
        if self.class_name(id)? != "System.String" {
            return None;
        }
        let (a, _) = self.range(id)?;
        Some(str_at(self.d, a + 5)?.0)
    }

    /// Element references of an array object (`0x11`), or of a `List` (whose only datum is its
    /// backing array).
    pub fn elements(&self, id: u32) -> Vec<u32> {
        let Some((a, b)) = self.range(id) else { return vec![] };
        let d = self.d;
        match d.get(a) {
            Some(0x11) => {
                let n = u32_at(d, a + 1).unwrap_or(0) as usize;
                // Element type: 0x10 + u32 class.
                let start = a + 5 + if d.get(a + 5) == Some(&0x10) { 5 } else { 0 };
                (0..n).filter_map(|i| u32_at(d, start + 4 * i).filter(|_| start + 4 * i + 4 <= b)).collect()
            }
            Some(0x10) if self.class_name(id).is_some_and(|n| n.starts_with("System.Collections.Generic.List`1")) => {
                u32_at(d, a + 5).map(|arr| self.elements(arr)).unwrap_or_default()
            }
            _ => vec![],
        }
    }

    /// Entries of a `Dictionary` with fixed-size keys and object values: (key, value id).
    pub fn dictionary(&self, id: u32, key_size: usize) -> Vec<(u64, u32)> {
        let Some((a, b)) = self.range(id) else { return vec![] };
        let Some((n, mut o)) = len_at(self.d, a + 5) else { return vec![] };
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            if o + key_size + 4 > b {
                break;
            }
            let key = match key_size {
                8 => u64_at(self.d, o).unwrap_or(0),
                _ => u32_at(self.d, o).unwrap_or(0) as u64,
            };
            out.push((key, u32_at(self.d, o + key_size).unwrap_or(0)));
            o += key_size + 4;
        }
        out
    }
}
