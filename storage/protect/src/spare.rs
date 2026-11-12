// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Hot spares: disks held outside every set, ready to take the place of
//! a failed member.

use blockdev::BlockDev;

/// The spare to use for a member that needs `need` blocks: the smallest
/// that is large enough, the first of equals. `sizes` gives each
/// attached spare's index and size.
pub fn smallest_fit(sizes: impl IntoIterator<Item = (usize, u64)>, need: u64) -> Option<usize> {
    sizes
        .into_iter()
        .filter(|&(_, blocks)| blocks >= need)
        .min_by_key(|&(_, blocks)| blocks)
        .map(|(i, _)| i)
}

/// A hot spare a caller holds for its sets.
pub struct Spare {
    /// Its identity, which becomes the member's.
    pub uuid: [u8; 16],
    /// The device.
    pub dev: Box<dyn BlockDev>,
}

/// Take the spare for a member of `need` blocks out of the pool.
pub fn take(spares: &mut Vec<Spare>, need: u64) -> Option<Spare> {
    let at = smallest_fit(
        spares.iter().enumerate().map(|(i, s)| (i, s.dev.blocks())),
        need,
    )?;
    Some(spares.remove(at))
}

#[cfg(test)]
mod tests {
    use blockdev::MemBlockDev;

    use super::*;

    #[test]
    fn the_smallest_that_fits_is_taken() {
        assert_eq!(smallest_fit([(0, 100), (1, 50), (2, 80)], 60), Some(2));
        assert_eq!(smallest_fit([(0, 80), (1, 80)], 80), Some(0));
        assert_eq!(smallest_fit([(0, 10)], 60), None);
        let mut pool: Vec<Spare> = [(1u8, 64u64), (2, 32), (3, 48)]
            .into_iter()
            .map(|(id, blocks)| Spare {
                uuid: [id; 16],
                dev: Box::new(MemBlockDev::new(blocks)),
            })
            .collect();
        assert_eq!(take(&mut pool, 40).unwrap().uuid, [3; 16]);
        assert_eq!(pool.len(), 2);
        assert!(take(&mut pool, 100).is_none());
    }
}
