use std::cmp::Ordering;


#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct Interval {
    beg: u32,
    end: u32
}


impl Interval {
    pub fn new(beg: u32, end: u32) -> Self {
        assert!(beg <= end);
        Interval { beg, end }
    }

    #[inline]
    pub fn begin(&self) -> u32 {
        self.beg
    }

    #[inline]
    pub fn end(&self) -> u32 {
        self.end
    }
}


#[derive(Clone)]
pub struct Range {
    intervals: Box<[Interval]>
}


impl Range {
    pub fn empty() -> Self {
        Range {
            intervals: Box::new([])
        }
    }

    pub fn new(intervals: Box<[Interval]>) -> Self {
        Self::try_new(intervals).unwrap()
    }

    pub fn try_new(intervals: Box<[Interval]>) -> Option<Self> {
        for i in 1..intervals.len() {
            let c = intervals[i];
            let p = intervals[i-1];
            if p.end + 1 >= c.beg {
                return None
            }
        }
        Some(Range { intervals })
    }

    pub fn has(&self, point: u32) -> bool {
        self.containing_interval(point).is_some()
    }

    pub fn find_containing_interval(&self, point: u32) -> Option<Interval> {
        self.containing_interval(point).map(|i| self.intervals[i])
    }

    fn containing_interval(&self, point: u32) -> Option<usize> {
        self.intervals.binary_search_by(|i| {
            if point < i.beg {
                Ordering::Greater
            } else if i.end < point {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        }).ok()
    }

    pub fn includes(&self, interval: Interval) -> bool {
        if let Some(c) = self.find_containing_interval(interval.beg) {
            c.end >= interval.end
        } else {
            false
        }
    }
}
