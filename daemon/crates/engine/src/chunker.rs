//! Range-splitting strategy (Phase 4).

use crate::MIN_CHUNK_SIZE;

/// Number of segments for a file of `size`, capped at `max_segments`.
pub fn calculate_segments(size: u64, max_segments: u8) -> u8 {
    if size == 0 {
        return 1;
    }
    let by_size = size.div_ceil(MIN_CHUNK_SIZE);
    by_size.clamp(1, max_segments as u64) as u8
}

/// Split `size` bytes into `n` inclusive `(start, end)` ranges.
pub fn split_ranges(size: u64, n: u8) -> Vec<(u64, u64)> {
    let n = n.max(1) as u64;
    let chunk = size / n;
    let mut ranges = Vec::with_capacity(n as usize);
    for i in 0..n {
        let start = i * chunk;
        let end = if i == n - 1 {
            size - 1
        } else {
            (i + 1) * chunk - 1
        };
        ranges.push((start, end));
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_split_even() {
        let ranges = split_ranges(100_000_000, 4);
        assert_eq!(ranges.len(), 4);
        assert_eq!(ranges[0], (0, 24_999_999));
        assert_eq!(ranges[3].1, 99_999_999);
    }

    #[test]
    fn test_segment_count_clamped() {
        assert_eq!(calculate_segments(500_000, 8), 1); // < 1 MiB → 1
        assert_eq!(calculate_segments(50 * 1024 * 1024, 8), 8); // capped
    }
}
