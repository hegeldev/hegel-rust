//! Port of `hypothesis.internal.intervalsets.IntervalSet` — a compact
//! representation of a set of `(start, end)` codepoint intervals with O(log n)
//! indexing and set-algebra operations.

/// A sorted, disjoint set of `(start, end)` codepoint intervals. Inclusive on
/// both endpoints. Acts like a sorted sequence of the covered integers.
#[derive(Debug, Clone)]
pub struct IntervalSet {
    pub intervals: Vec<(u32, u32)>,
    offsets: Vec<usize>,
    size: usize,
    idx_of_zero: usize,
    idx_of_z: isize,
}

impl IntervalSet {
    /// Build from a list of `(start, end)` intervals. Each must satisfy
    /// `start <= end`; the caller is expected to pass disjoint, sorted
    /// intervals (as all producers in this codebase do).
    pub fn new(intervals: Vec<(u32, u32)>) -> Self {
        let mut offsets = Vec::with_capacity(intervals.len() + 1);
        offsets.push(0usize);
        for &(u, v) in &intervals {
            assert!(u <= v, "invalid interval ({u}, {v})");
            let last = *offsets.last().unwrap();
            offsets.push(last + (v - u + 1) as usize);
        }
        let size = offsets.pop().unwrap();

        let mut set = IntervalSet {
            intervals,
            offsets,
            size,
            idx_of_zero: 0,
            idx_of_z: -1,
        };
        set.idx_of_zero = set.index_above('0' as u32);
        let z_above = set.index_above('Z' as u32);
        set.idx_of_z = if size == 0 {
            -1
        } else {
            z_above.min(size - 1) as isize
        };
        set
    }

    pub fn len(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn idx_of_zero(&self) -> usize {
        self.idx_of_zero
    }

    pub fn idx_of_z(&self) -> isize {
        self.idx_of_z
    }

    /// Codepoint at position `i`, accepting negative indices (Python-style).
    /// Returns `None` when `i` is out of range.
    pub fn get(&self, i: isize) -> Option<u32> {
        let resolved = if i < 0 { self.size as isize + i } else { i };
        if resolved < 0 || resolved >= self.size as isize {
            return None;
        }
        let i = resolved as usize;

        let mut j = self.intervals.len() - 1;
        if self.offsets[j] > i {
            let mut lo = 0usize;
            let mut hi = j;
            while lo + 1 < hi {
                let mid = lo + (hi - lo) / 2;
                if self.offsets[mid] <= i {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            j = lo;
        }
        let t = i - self.offsets[j];
        let (u, _v) = self.intervals[j];
        Some(u + t as u32)
    }

    pub fn contains(&self, elem: u32) -> bool {
        self.intervals.iter().any(|&(s, e)| s <= elem && elem <= e)
    }

    /// Position of `value`, or `None` if not present.
    pub fn index(&self, value: u32) -> Option<usize> {
        for (&offset, &(u, v)) in self.offsets.iter().zip(&self.intervals) {
            if u == value {
                return Some(offset);
            } else if u > value {
                return None;
            }
            if value <= v {
                return Some(offset + (value - u) as usize);
            }
        }
        None
    }

    /// Smallest position `i` with `self[i] >= value`, or `self.len()` if
    /// every element is below `value`.
    pub fn index_above(&self, value: u32) -> usize {
        for (&offset, &(u, v)) in self.offsets.iter().zip(&self.intervals) {
            if u >= value {
                return offset;
            }
            if value <= v {
                return offset + (value - u) as usize;
            }
        }
        self.size
    }

    /// Iterate the covered codepoints in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.intervals.iter().flat_map(|&(u, v)| u..=v)
    }

    /// Set-difference: elements in `self` not in `other`.
    pub fn difference(&self, other: &IntervalSet) -> IntervalSet {
        let mut x: Vec<(u32, u32)> = self.intervals.clone();
        let y = &other.intervals;
        let mut i = 0usize;
        let mut j = 0usize;
        let mut result: Vec<(u32, u32)> = Vec::new();
        while i < x.len() && j < y.len() {
            let (xl, xr) = x[i];
            let (yl, yr) = y[j];
            if yr < xl {
                j += 1;
            } else if yl > xr {
                result.push(x[i]);
                i += 1;
            } else if yl <= xl {
                if yr >= xr {
                    i += 1;
                } else {
                    x[i].0 = yr + 1;
                    j += 1;
                }
            } else {
                result.push((xl, yl - 1));
                if yr < xr {
                    x[i].0 = yr + 1;
                    j += 1;
                } else {
                    i += 1;
                }
            }
        }
        result.extend_from_slice(&x[i..]);
        IntervalSet::new(result)
    }

    /// Set-intersection: elements in both.
    pub fn intersection(&self, other: &IntervalSet) -> IntervalSet {
        let mut result = Vec::new();
        let mut i = 0usize;
        let mut j = 0usize;
        while i < self.intervals.len() && j < other.intervals.len() {
            let (u, v) = self.intervals[i];
            let (uu, vv) = other.intervals[j];
            if u > vv {
                j += 1;
            } else if uu > v {
                i += 1;
            } else {
                result.push((u.max(uu), v.min(vv)));
                if v < vv {
                    i += 1;
                } else {
                    j += 1;
                }
            }
        }
        IntervalSet::new(result)
    }

    /// Character at position `i` under shrink-preferred ordering: '0', then
    /// the digits up through 'Z', then everything below '0', then everything
    /// above 'Z' — so shrinking walks toward '0'.
    pub fn char_in_shrink_order(&self, i: usize) -> char {
        let mut i = i as isize;
        if i <= self.idx_of_z {
            let n = self.idx_of_z - self.idx_of_zero as isize;
            if i <= n {
                i += self.idx_of_zero as isize;
            } else {
                i = self.idx_of_zero as isize - (i - n);
            }
        }
        char::from_u32(self.get(i).unwrap()).unwrap()
    }

    /// Inverse of `char_in_shrink_order`.
    pub fn index_from_char_in_shrink_order(&self, c: char) -> usize {
        let mut i = self.index(c as u32).unwrap() as isize;
        if i <= self.idx_of_z {
            let n = self.idx_of_z - self.idx_of_zero as isize;
            if (self.idx_of_zero as isize) <= i && i <= self.idx_of_z {
                i -= self.idx_of_zero as isize;
            } else {
                i = self.idx_of_zero as isize - i + n;
            }
        }
        i as usize
    }
}

impl PartialEq for IntervalSet {
    fn eq(&self, other: &Self) -> bool {
        self.intervals == other.intervals
    }
}

impl Eq for IntervalSet {}
