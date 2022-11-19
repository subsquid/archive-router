use crate::interval::Interval;


pub struct DataChunk {
    pub first_block: u32,
    pub last_block: u32,
    pub top: u32,
}


fn parse_interval(s: &str) -> Option<Interval> {
    let split_idx = s.find('-')?;
    let (l, r) = s.split_at(split_idx);
    let beg: u32 = l.parse().map_or_else(|_| None, Some)?;
    let end: u32 = r.parse().map_or_else(|_| None, Some)?;
    Some((beg, end))
}
