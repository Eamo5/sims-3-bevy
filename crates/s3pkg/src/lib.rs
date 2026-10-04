//! Reader for Sims 3 DBPF 2.0 `.package` / `.world` files.

pub mod install;
pub mod refpack;
pub mod types;

use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceKey {
    pub t: u32,
    pub g: u32,
    pub i: u64,
}

impl ResourceKey {
    pub const fn new(t: u32, g: u32, i: u64) -> Self {
        Self { t, g, i }
    }
}

impl fmt::Debug for ResourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08X}:{:08X}:{:016X}", self.t, self.g, self.i)
    }
}

impl fmt::Display for ResourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IndexEntry {
    pub key: ResourceKey,
    pub offset: u64,
    pub file_size: u32,
    pub mem_size: u32,
    pub compressed: u16,
}

impl IndexEntry {
    pub fn is_deleted(&self) -> bool {
        self.compressed == 0xFFE0
    }
}

pub struct Package {
    pub path: PathBuf,
    pub entries: Vec<IndexEntry>,
    file: Mutex<File>,
}

fn rd_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl Package {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;
        let mut hdr = [0u8; 96];
        file.read_exact(&mut hdr)?;
        if &hdr[0..4] != b"DBPF" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not a DBPF file"));
        }
        let major = rd_u32(&hdr, 4);
        if major != 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unsupported DBPF major version {major}"),
            ));
        }
        let count = rd_u32(&hdr, 0x24) as usize;
        let short_off = rd_u32(&hdr, 0x28) as u64;
        let size = rd_u32(&hdr, 0x2C) as usize;
        let long_off = rd_u32(&hdr, 0x40) as u64;
        let index_off = if long_off != 0 { long_off } else { short_off };

        let mut entries = Vec::with_capacity(count);
        if count > 0 {
            file.seek(SeekFrom::Start(index_off))?;
            let mut idx = vec![0u8; size];
            file.read_exact(&mut idx)?;
            let flags = rd_u32(&idx, 0);
            let mut p = 4;
            let take = |p: &mut usize| {
                let v = rd_u32(&idx, *p);
                *p += 4;
                v
            };
            let c_type = (flags & 1 != 0).then(|| take(&mut p));
            let c_group = (flags & 2 != 0).then(|| take(&mut p));
            let c_ihi = (flags & 4 != 0).then(|| take(&mut p));
            for _ in 0..count {
                let t = c_type.unwrap_or_else(|| take(&mut p));
                let g = c_group.unwrap_or_else(|| take(&mut p));
                let ihi = c_ihi.unwrap_or_else(|| take(&mut p));
                let ilo = take(&mut p);
                let offset = take(&mut p) as u64;
                let fsize = take(&mut p) & 0x7FFF_FFFF;
                let msize = take(&mut p);
                let comp = (take(&mut p) & 0xFFFF) as u16;
                entries.push(IndexEntry {
                    key: ResourceKey::new(t, g, ((ihi as u64) << 32) | ilo as u64),
                    offset,
                    file_size: fsize,
                    mem_size: msize,
                    compressed: comp,
                });
            }
        }
        Ok(Self {
            path,
            entries,
            file: Mutex::new(file),
        })
    }

    pub fn read_raw(&self, e: &IndexEntry) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; e.file_size as usize];
        let mut f = self.file.lock().unwrap();
        f.seek(SeekFrom::Start(e.offset))?;
        f.read_exact(&mut buf)?;
        Ok(buf)
    }

    pub fn read(&self, e: &IndexEntry) -> io::Result<Vec<u8>> {
        let raw = self.read_raw(e)?;
        if e.compressed == 0xFFFF && raw.len() >= 2 && raw[1] == 0xFB {
            refpack::decompress(&raw).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
        } else {
            Ok(raw)
        }
    }

    pub fn find(&self, key: &ResourceKey) -> Option<&IndexEntry> {
        self.entries.iter().find(|e| e.key == *key && !e.is_deleted())
    }

    pub fn of_type(&self, t: u32) -> impl Iterator<Item = &IndexEntry> {
        self.entries.iter().filter(move |e| e.key.t == t && !e.is_deleted())
    }
}

/// A stack of packages searched in priority order (later-added packages override earlier ones).
#[derive(Default)]
pub struct PackageSet {
    pub packages: Vec<Package>,
    index: HashMap<ResourceKey, (usize, usize)>,
    by_ti: HashMap<(u32, u64), ResourceKey>,
}

impl PackageSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a package; its resources override any already present with the same key.
    pub fn add(&mut self, pkg: Package) {
        let pi = self.packages.len();
        for (ei, e) in pkg.entries.iter().enumerate() {
            if e.is_deleted() {
                self.index.remove(&e.key);
                self.by_ti.remove(&(e.key.t, e.key.i));
            } else {
                self.index.insert(e.key, (pi, ei));
                self.by_ti.insert((e.key.t, e.key.i), e.key);
            }
        }
        self.packages.push(pkg);
    }

    pub fn get_entry(&self, key: &ResourceKey) -> Option<(&Package, &IndexEntry)> {
        let &(pi, ei) = self.index.get(key)?;
        let p = &self.packages[pi];
        Some((p, &p.entries[ei]))
    }

    pub fn read(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        let (p, e) = self.get_entry(key)?;
        p.read(e).ok()
    }

    pub fn keys(&self) -> impl Iterator<Item = &ResourceKey> {
        self.index.keys()
    }

    pub fn keys_of_type(&self, t: u32) -> impl Iterator<Item = &ResourceKey> {
        self.index.keys().filter(move |k| k.t == t)
    }

    /// Finds any resource with matching type and instance, ignoring group.
    pub fn find_ti(&self, t: u32, i: u64) -> Option<ResourceKey> {
        self.by_ti.get(&(t, i)).copied()
    }

    /// Reads the resource with matching type and instance, ignoring group.
    pub fn read_ti(&self, t: u32, i: u64) -> Option<Vec<u8>> {
        self.read(&self.find_ti(t, i)?)
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

/// FNV hashes as used throughout Sims 3 for names -> instance ids.
pub fn fnv32(s: &str) -> u32 {
    let mut h: u32 = 0x811C9DC5;
    for b in s.to_ascii_lowercase().bytes() {
        h = h.wrapping_mul(0x01000193);
        h ^= b as u32;
    }
    h
}

pub fn fnv64(s: &str) -> u64 {
    let mut h: u64 = 0xCBF29CE484222325;
    for b in s.to_ascii_lowercase().bytes() {
        h = h.wrapping_mul(0x00000100000001B3);
        h ^= b as u64;
    }
    h
}
