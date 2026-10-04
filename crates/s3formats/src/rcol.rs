//! RCOL ("resource collection") container used by MODL, MLOD, MATD, GEOM, VPXY, FTPT, ...

use crate::util::{Eof, R, Reader};
use s3pkg::ResourceKey;

#[derive(Clone, Debug)]
pub struct ChunkEntry {
    pub key: ResourceKey,
    pub offset: usize,
    pub size: usize,
}

pub struct Rcol<'a> {
    pub data: &'a [u8],
    pub public_chunks: usize,
    pub chunks: Vec<ChunkEntry>,
    pub external: Vec<ResourceKey>,
}

/// A decoded 32-bit chunk reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkRef {
    Null,
    Chunk(usize),
    External(usize),
}

impl<'a> Rcol<'a> {
    pub fn parse(data: &'a [u8]) -> R<Self> {
        let mut r = Reader::new(data);
        let _version = r.u32()?;
        let public_chunks = r.i32()?.max(0) as usize;
        let _unused = r.u32()?;
        let external_count = r.i32()?.max(0) as usize;
        let internal_count = r.i32()?.max(0) as usize;
        if internal_count > 4096 || external_count > 65536 {
            return Err(Eof);
        }
        let mut keys = Vec::with_capacity(internal_count);
        for _ in 0..internal_count {
            let i = r.u64()?;
            let t = r.u32()?;
            let g = r.u32()?;
            keys.push(ResourceKey::new(t, g, i));
        }
        let mut external = Vec::with_capacity(external_count);
        for _ in 0..external_count {
            let i = r.u64()?;
            let t = r.u32()?;
            let g = r.u32()?;
            external.push(ResourceKey::new(t, g, i));
        }
        let mut chunks = Vec::with_capacity(internal_count);
        for key in keys {
            let offset = r.u32()? as usize;
            let size = r.u32()? as usize;
            chunks.push(ChunkEntry { key, offset, size });
        }
        if chunks.len() == 1 {
            // Single-chunk quirk: extent derived from the header length.
            let pos = 0x2C + external_count * 16;
            chunks[0].offset = pos;
            chunks[0].size = data.len().saturating_sub(pos);
        }
        for c in &chunks {
            if c.offset + c.size > data.len() {
                return Err(Eof);
            }
        }
        Ok(Self { data, public_chunks, chunks, external })
    }

    pub fn decode_ref(&self, raw: u32) -> ChunkRef {
        if raw == 0 {
            return ChunkRef::Null;
        }
        let kind = raw >> 28;
        let index = ((raw & 0x0FFF_FFFF) as usize).wrapping_sub(1);
        match kind {
            0 => ChunkRef::Chunk(index),
            1 => ChunkRef::Chunk(index + self.public_chunks),
            3 => ChunkRef::External(index),
            _ => ChunkRef::Null,
        }
    }

    pub fn chunk_data(&self, index: usize) -> Option<&'a [u8]> {
        let c = self.chunks.get(index)?;
        self.data.get(c.offset..c.offset + c.size)
    }

    pub fn chunk_tag(&self, index: usize) -> Option<[u8; 4]> {
        let d = self.chunk_data(index)?;
        d.get(0..4).map(|b| b.try_into().unwrap())
    }

    /// Resolves a raw reference to chunk bytes (internal references only).
    pub fn resolve(&self, raw: u32) -> Option<(usize, &'a [u8])> {
        match self.decode_ref(raw) {
            ChunkRef::Chunk(i) => Some((i, self.chunk_data(i)?)),
            _ => None,
        }
    }

    pub fn external_key(&self, raw: u32) -> Option<ResourceKey> {
        match self.decode_ref(raw) {
            ChunkRef::External(i) => self.external.get(i).copied(),
            _ => None,
        }
    }

    pub fn find_tag(&self, tag: &[u8; 4]) -> Option<usize> {
        (0..self.chunks.len()).find(|&i| self.chunk_tag(i).as_ref() == Some(tag))
    }
}
