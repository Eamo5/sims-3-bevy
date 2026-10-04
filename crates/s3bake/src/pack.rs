//! Pack files: many postcard-encoded blobs addressed by resource key, read lazily.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::types::Key;

const MAGIC: &[u8; 4] = b"S3PK";

pub struct PackWriter {
    out: BufWriter<File>,
    pos: u64,
    index: Vec<(Key, u64, u32)>,
}

impl PackWriter {
    pub fn create(path: &Path) -> io::Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut out = BufWriter::new(File::create(path)?);
        out.write_all(MAGIC)?;
        out.write_all(&crate::types::BAKE_VERSION.to_le_bytes())?;
        out.write_all(&0u64.to_le_bytes())?;
        Ok(Self { out, pos: 16, index: Vec::new() })
    }

    pub fn add<T: Serialize>(&mut self, key: Key, value: &T) -> io::Result<()> {
        let bytes = postcard::to_stdvec(value).map_err(io::Error::other)?;
        self.out.write_all(&bytes)?;
        self.index.push((key, self.pos, bytes.len() as u32));
        self.pos += bytes.len() as u64;
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<()> {
        let idx = postcard::to_stdvec(&self.index).map_err(io::Error::other)?;
        self.out.write_all(&idx)?;
        let mut f = self.out.into_inner().map_err(|e| e.into_error())?;
        f.seek(SeekFrom::Start(8))?;
        f.write_all(&self.pos.to_le_bytes())?;
        Ok(())
    }
}

pub struct PackReader {
    file: Mutex<File>,
    pub index: HashMap<Key, (u64, u32)>,
}

impl PackReader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut f = File::open(path)?;
        let mut hdr = [0u8; 16];
        f.read_exact(&mut hdr)?;
        if &hdr[0..4] != MAGIC || u32::from_le_bytes(hdr[4..8].try_into().unwrap()) != crate::types::BAKE_VERSION {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "stale or invalid pack"));
        }
        let index_off = u64::from_le_bytes(hdr[8..16].try_into().unwrap());
        f.seek(SeekFrom::Start(index_off))?;
        let mut idx = Vec::new();
        f.read_to_end(&mut idx)?;
        let list: Vec<(Key, u64, u32)> = postcard::from_bytes(&idx).map_err(io::Error::other)?;
        let index = list.into_iter().map(|(k, o, l)| (k, (o, l))).collect();
        Ok(Self { file: Mutex::new(f), index })
    }

    pub fn contains(&self, key: &Key) -> bool {
        self.index.contains_key(key)
    }

    pub fn get<T: DeserializeOwned>(&self, key: &Key) -> Option<T> {
        let &(off, len) = self.index.get(key)?;
        let mut buf = vec![0u8; len as usize];
        {
            let mut f = self.file.lock().unwrap();
            f.seek(SeekFrom::Start(off)).ok()?;
            f.read_exact(&mut buf).ok()?;
        }
        postcard::from_bytes(&buf).ok()
    }

    /// Reads every entry (used for world packs, which are loaded whole).
    pub fn all<T: DeserializeOwned>(&self) -> Vec<(Key, T)> {
        let mut entries: Vec<(Key, (u64, u32))> = self.index.iter().map(|(k, v)| (*k, *v)).collect();
        entries.sort_by_key(|e| e.1.0);
        entries.into_iter().filter_map(|(k, _)| Some((k, self.get(&k)?))).collect()
    }
}

/// Writes a single postcard value to a file.
pub fn write_value<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let bytes = postcard::to_stdvec(value).map_err(io::Error::other)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

pub fn read_value<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let bytes = std::fs::read(path)?;
    postcard::from_bytes(&bytes).map_err(io::Error::other)
}
