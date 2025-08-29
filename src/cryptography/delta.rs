use std::{
    fs::File,
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
};

use color_eyre::eyre::Result;
use fbuzhash::BuzHash;
use xxhash_rust::xxh3::xxh3_64;

use super::signatures::BLOCK_SIZE;

#[derive(Debug)]
enum DeltaOperations {
    Copy { offset: usize, len: usize },
    Insert { data: Vec<u8> },
}

#[derive(Debug)]
pub struct Delta {
    operations: Vec<DeltaOperations>,
}

impl Delta {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }
    pub fn generate_delta(&self, base: &[u8], new_: &[u8]) -> Self {
        let mut sigs = super::signatures::Signature::new();
        sigs.build(base).get_entries();

        let mut delta: Vec<DeltaOperations> = Vec::new();
        let mut insert_buf: Vec<u8> = Vec::new();

        // If new file shorter than block, everything is literal
        if new_.len() < BLOCK_SIZE {
            if !new_.is_empty() {
                delta.push(DeltaOperations::Insert {
                    data: new_.to_vec(),
                });
            }
            return Delta { operations: delta };
        }

        let mut pos = 0usize;

        // seed the rolling buzhash with first BLOCK_SIZE bytes
        let mut bh = BuzHash::new(BLOCK_SIZE as u32);
        bh.write_all(&new_[0..BLOCK_SIZE]).unwrap();
        let mut weak = bh.sum32();

        // helper to flush inserts
        let flush_inserts = |delta: &mut Vec<DeltaOperations>, buf: &mut Vec<u8>| {
            if !buf.is_empty() {
                delta.push(DeltaOperations::Insert {
                    data: std::mem::take(buf),
                });
            }
        };

        while pos + BLOCK_SIZE <= new_.len() {
            let window = &new_[pos..pos + BLOCK_SIZE];
            let mut matched = false;

            if let Some(cands) = sigs.get_entries().get(&weak) {
                let strong = xxh3_64(window);

                // find candidate with same strong hash and same length (full block)
                if let Some(entry) = cands
                    .iter()
                    .find(|e| e.len == BLOCK_SIZE && e.strong == strong)
                {
                    // confirmed match
                    flush_inserts(&mut delta, &mut insert_buf);
                    delta.push(DeltaOperations::Copy {
                        offset: entry.offset,
                        len: entry.len,
                    });

                    pos += BLOCK_SIZE;

                    // If there are enough bytes left, reseed the buzhash for the next aligned window
                    if pos + BLOCK_SIZE <= new_.len() {
                        bh = BuzHash::new(BLOCK_SIZE as u32);
                        bh.write_all(&new_[pos..pos + BLOCK_SIZE]).unwrap();
                        weak = bh.sum32();
                    } else {
                        // append leftover tail (if any) as insert and finish
                        if pos < new_.len() {
                            insert_buf.extend_from_slice(&new_[pos..]);
                        }
                        flush_inserts(&mut delta, &mut insert_buf);
                        break;
                    }

                    matched = true;
                }
            }

            if !matched {
                // no match: emit first byte of current window into insert buffer and slide by 1
                insert_buf.push(new_[pos]);
                pos += 1;

                // if not enough to form a full window after sliding, everything left is literal
                if pos + BLOCK_SIZE > new_.len() {
                    insert_buf.extend_from_slice(&new_[pos..]);
                    flush_inserts(&mut delta, &mut insert_buf);
                    break;
                } else {
                    // slide the rolling hash by feeding next byte (BuzHash handles the buffer internals)
                    let next_in = new_[pos + BLOCK_SIZE - 1];
                    let _ = bh.hash_byte(next_in); // returns u32 but we just update state
                    weak = bh.sum32();
                }
            }
        }
        Delta { operations: delta }
    }

    pub fn apply(&self, base: &[u8], out_path: PathBuf) -> Result<()> {
        let mut out = File::create("temp_out").unwrap();
        let delta = &self.operations;
        for op in delta {
            match op {
                DeltaOperations::Copy { offset, len } => {
                    let start = *offset;
                    let end = start.saturating_add(*len).min(base.len());
                    if start < base.len() && start < end {
                        out.write_all(&base[start..end])?;
                    }
                }
                DeltaOperations::Insert { data } => {
                    out.write_all(data)?;
                }
            }
        }
        out.flush()?;
        std::fs::rename("temp_out", out_path)?;
        let _ = std::fs::remove_file("temp_out");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;

    #[test]
    fn test_transfer() {
        let mut from_file = File::open("test/from.txt").unwrap();
        let mut to_file = File::open("test/to.txt").unwrap();

        let mut base = Vec::new();
        let mut new_ = Vec::new();
        from_file.read_to_end(&mut base).unwrap();
        to_file.read_to_end(&mut new_).unwrap();

        let del = Delta::new();
        let delta = del.generate_delta(&new_, &base);
        delta.apply(&base, PathBuf::from("test/to.txt")).unwrap();
        let base = std::fs::read("test/from.txt").unwrap();
        let new_ = std::fs::read("test/to.txt").unwrap();
        pretty_assertions::assert_eq!(base, new_);
    }
}
