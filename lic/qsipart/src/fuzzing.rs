// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fuzz target: bytes as either side would receive them, in pieces,
//! through the reader and every decoder, the console's applied to a
//! grid. Nothing may panic, hang or grow without bound.

use crate::frame::Reader;
use crate::{console, service};

/// One input. The first byte chooses the piece size.
pub fn frames(data: &[u8]) {
    let Some((&first, rest)) = data.split_first() else {
        return;
    };
    let piece = usize::from(first % 61) + 1;
    let mut r = Reader::new();
    let mut grid = console::Grid::default();
    for chunk in rest.chunks(piece) {
        r.push(chunk);
        while let Some(f) = r.next_frame() {
            let _ = service::ToSystem::decode(&f);
            let _ = service::ToSystem::request_id(&f);
            let _ = service::FromSystem::decode(&f);
            let _ = console::ToSystem::decode(&f);
            if let Ok(Some(m)) = console::FromSystem::decode(&f) {
                let _ = grid.apply(&m);
                let _ = grid.text();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_and_noise() {
        let mut seed = vec![7u8];
        seed.extend(
            service::ToSystem::Ping(service::Ping { id: 1 })
                .encode()
                .unwrap(),
        );
        frames(&seed);
        frames(&[]);
        let mut x: u32 = 1;
        let noise: Vec<u8> = (0..20_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                if x.is_multiple_of(7) {
                    b'Q'
                } else {
                    x.to_le_bytes()[1]
                }
            })
            .collect();
        frames(&noise);
    }
}
