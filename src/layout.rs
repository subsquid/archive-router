use std::fmt::{format, Formatter};


#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct DataChunk {
    top: u32,
    first_block: u32,
    last_block: u32
}


impl DataChunk {
    pub fn new(top: u32, first_block: u32, last_block: u32) -> Self {
        Self::try_new(top, first_block, last_block).expect("Invalid data chunk")
    }

    pub fn try_new(top: u32, first_block: u32, last_block: u32) -> Result<Self, String> {
        let chunk = DataChunk { top, first_block, last_block };
        if first_block < top {
            return Err(format!("first_block < top in {}", chunk))
        }
        if last_block < first_block {
            return Err(format!("last_block < first_block in {}", chunk))
        }
        Ok(chunk)
    }

    #[inline]
    pub fn top(&self) -> u32 {
        self.top
    }

    #[inline]
    pub fn first_block(&self) -> u32 {
        self.first_block
    }

    #[inline]
    pub fn last_block(&self) -> u32 {
        self.last_block
    }
}


impl std::fmt::Display for DataChunk {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:010}/{:010}-{:010}", self.top, self.first_block, self.last_block)
    }
}


fn parse_interval(s: &str) -> Option<(u32, u32)> {
    let split_idx = s.find('-')?;
    let (l, r) = s.split_at(split_idx);
    let beg: u32 = l.parse().map_or_else(|_| None, Some)?;
    let end: u32 = r.parse().map_or_else(|_| None, Some)?;
    Some((beg, end))
}
