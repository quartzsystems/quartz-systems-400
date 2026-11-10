// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RC4, for NTLM's key exchange only.

/// An RC4 key stream.
pub struct Rc4 {
    s: [u8; 256],
    i: u8,
    j: u8,
}

impl Rc4 {
    /// A stream keyed by `key` (1 to 256 bytes).
    pub fn new(key: &[u8]) -> Self {
        assert!(!key.is_empty() && key.len() <= 256, "RC4 key length");
        let mut s = [0u8; 256];
        for (i, b) in s.iter_mut().enumerate() {
            *b = u8::try_from(i).expect("byte index");
        }
        let mut j = 0u8;
        for i in 0..256 {
            j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
            s.swap(i, usize::from(j));
        }
        Self { s, i: 0, j: 0 }
    }

    /// XORs the next key stream bytes into `data`.
    pub fn apply(&mut self, data: &mut [u8]) {
        for b in data {
            self.i = self.i.wrapping_add(1);
            self.j = self.j.wrapping_add(self.s[usize::from(self.i)]);
            self.s.swap(usize::from(self.i), usize::from(self.j));
            let k = self.s[usize::from(self.s[usize::from(self.i)].wrapping_add(self.s[usize::from(self.j)]))];
            *b ^= k;
        }
    }
}

/// `data` encrypted (or decrypted) under `key` from the start of the stream.
pub fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    Rc4::new(key).apply(&mut out);
    out
}
