use anyhow::{bail, Result};
use byteorder::{LE, ReadBytesExt};
use std::io::{Cursor, Read, Seek, SeekFrom};

#[derive(Debug, Clone)]
pub struct AfsEntry {
    pub name: String,
    pub offset: u32,
    pub size: u32,
}

pub struct AfsArchive {
    data: Vec<u8>,
    entries: Vec<AfsEntry>,
}

impl AfsArchive {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self> {
        let mut cur = Cursor::new(&data);

        let mut magic = [0u8; 4];
        cur.read_exact(&mut magic)?;
        if &magic != b"AFS\x00" {
            bail!("Invalid AFS magic: {:?}", magic);
        }

        let count = cur.read_u32::<LE>()? as usize;
        let mut raw_entries = Vec::with_capacity(count);
        for _ in 0..count {
            let offset = cur.read_u32::<LE>()?;
            let size = cur.read_u32::<LE>()?;
            raw_entries.push((offset, size));
        }

        // Name table is at 8 + count*8 (the next two u32 after the TOC)
        let toc_end = 8 + count * 8;
        cur.seek(SeekFrom::Start(toc_end as u64))?;
        let name_tbl_offset = cur.read_u32::<LE>()?;
        let _name_tbl_size = cur.read_u32::<LE>()?;

        let mut entries = Vec::with_capacity(count);

        if name_tbl_offset > 0 && (name_tbl_offset as usize) < data.len() {
            cur.seek(SeekFrom::Start(name_tbl_offset as u64))?;
            for (offset, size) in raw_entries {
                let mut name_buf = [0u8; 32];
                cur.read_exact(&mut name_buf)?;
                let _mtime = {
                    let mut m = [0u8; 16];
                    cur.read_exact(&mut m)?;
                    m
                };
                let end = name_buf.iter().position(|&b| b == 0).unwrap_or(32);
                let name = String::from_utf8_lossy(&name_buf[..end]).to_string();
                entries.push(AfsEntry { name, offset, size });
            }
        } else {
            // No name table — use index as name
            for (i, (offset, size)) in raw_entries.into_iter().enumerate() {
                entries.push(AfsEntry {
                    name: format!("{:04}", i),
                    offset,
                    size,
                });
            }
        }

        Ok(Self { data, entries })
    }

    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let data = std::fs::read(path)?;
        Self::from_bytes(data)
    }

    pub fn entries(&self) -> &[AfsEntry] {
        &self.entries
    }

    pub fn get_by_name(&self, name: &str) -> Option<&[u8]> {
        self.entries.iter().find(|e| e.name == name).map(|e| {
            let start = e.offset as usize;
            let end = start + e.size as usize;
            &self.data[start..end]
        })
    }

    pub fn get_by_index(&self, index: usize) -> Option<&[u8]> {
        self.entries.get(index).map(|e| {
            let start = e.offset as usize;
            let end = start + e.size as usize;
            &self.data[start..end]
        })
    }

    pub fn names_with_extension(&self, ext: &str) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| e.name.ends_with(ext))
            .map(|e| e.name.as_str())
            .collect()
    }
}
