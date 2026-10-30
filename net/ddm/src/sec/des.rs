// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DES in CBC mode with PKCS #5 padding, as the encrypted security
//! mechanisms use it.

use des::Des;
use des::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};

fn cipher(key: [u8; 8]) -> Des {
    Des::new_from_slice(&key).expect("8-byte key")
}

/// Encrypt `plain` (padded to a multiple of eight).
pub fn cbc_encrypt(key: &[u8; 8], iv: &[u8; 8], plain: &[u8]) -> Vec<u8> {
    let c = cipher(*key);
    let pad = 8 - plain.len() % 8;
    let mut data = plain.to_vec();
    data.extend(std::iter::repeat_n(u8::try_from(pad).unwrap_or(8), pad));
    let mut prev = *iv;
    for chunk in data.chunks_mut(8) {
        let mut b = [0u8; 8];
        for i in 0..8 {
            b[i] = chunk[i] ^ prev[i];
        }
        let mut block = des::cipher::Block::<Des>::from(b);
        c.encrypt_block(&mut block);
        prev = block.into();
        chunk.copy_from_slice(&prev);
    }
    data
}

/// Decrypt and strip the padding; `None` when it is not valid.
pub fn cbc_decrypt(key: &[u8; 8], iv: &[u8; 8], enc: &[u8]) -> Option<Vec<u8>> {
    if enc.is_empty() || !enc.len().is_multiple_of(8) {
        return None;
    }
    let c = cipher(*key);
    let mut out = Vec::with_capacity(enc.len());
    let mut prev = *iv;
    for chunk in enc.chunks(8) {
        let mut b = [0u8; 8];
        b.copy_from_slice(chunk);
        let mut block = des::cipher::Block::<Des>::from(b);
        c.decrypt_block(&mut block);
        let d: [u8; 8] = block.into();
        for i in 0..8 {
            out.push(d[i] ^ prev[i]);
        }
        prev = b;
    }
    let pad = usize::from(*out.last()?);
    if pad == 0 || pad > 8 || out[out.len() - pad..].iter().any(|&x| usize::from(x) != pad) {
        return None;
    }
    out.truncate(out.len() - pad);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let k = [1, 2, 3, 4, 5, 6, 7, 8];
        let iv = [9; 8];
        for n in [0, 1, 7, 8, 9, 16] {
            let p: Vec<u8> = (0..n).map(|i: u8| i).collect();
            let e = cbc_encrypt(&k, &iv, &p);
            assert_eq!(e.len() % 8, 0);
            assert!(e.len() > p.len());
            assert_eq!(cbc_decrypt(&k, &iv, &e).unwrap(), p);
        }
    }
}
