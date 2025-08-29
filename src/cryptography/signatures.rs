use std::{collections::HashMap, io::Write};

use fbuzhash::BuzHash;
use xxhash_rust::xxh3::xxh3_64;

pub const BLOCK_SIZE: usize = 4096;

#[derive(Debug, Default)]
pub struct Signature {
    entries: HashMap<u32, Vec<SigEntry>>,
}

#[derive(Debug, Default)]
pub struct SigEntry {
    pub strong: u64,
    pub offset: usize,
    pub len: usize,
}

impl Signature {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn build(&mut self, base: &[u8]) -> &Self {
        let mut sigs: HashMap<u32, Vec<SigEntry>> = HashMap::new();
        let mut offset = 0usize;

        while offset < base.len() {
            let len = BLOCK_SIZE.min(base.len() - offset);
            let block = &base[offset..offset + len];

            // Use a BuzHash sized to the block length
            let mut bh = BuzHash::new(len as u32);
            // write_all (Write trait) feeds all bytes and updates internal rolling buffer/state
            bh.write_all(block).unwrap();
            let weak = bh.sum32(); // u32 weak hash

            let strong = xxh3_64(block); // verification hash (u64)

            sigs.entry(weak).or_default().push(SigEntry {
                strong,
                offset,
                len,
            });

            offset += len;
        }
        self.entries = sigs;
        return self;
    }
    pub fn get_entries(&self) -> &HashMap<u32, Vec<SigEntry>> {
        &self.entries
    }
}
