// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Just enough unsigned arithmetic for the Diffie-Hellman exchange:
//! numbers of a few hundred bits, multiplied and reduced by shifting.

// Limb arithmetic: the casts keep the low 32 bits on purpose.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::many_single_char_names)]

/// An unsigned number, 32-bit limbs, least significant first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Num(Vec<u32>);

impl Num {
    /// From big-endian bytes.
    pub fn from_be(b: &[u8]) -> Self {
        let mut limbs = Vec::with_capacity(b.len() / 4 + 1);
        for chunk in b.rchunks(4) {
            let mut v = 0u32;
            for &x in chunk {
                v = (v << 8) | u32::from(x);
            }
            limbs.push(v);
        }
        let mut n = Self(limbs);
        n.trim();
        n
    }

    /// Big-endian bytes, left-padded with zeros (or cut) to `len`.
    pub fn to_be(&self, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        for (i, limb) in self.0.iter().enumerate() {
            for (j, byte) in limb.to_le_bytes().iter().enumerate() {
                let k = i * 4 + j;
                if k < len {
                    out[len - 1 - k] = *byte;
                }
            }
        }
        out
    }

    fn trim(&mut self) {
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    fn bits(&self) -> usize {
        match self.0.last() {
            None => 0,
            Some(&top) => (self.0.len() - 1) * 32 + (32 - top.leading_zeros() as usize),
        }
    }

    fn bit(&self, i: usize) -> bool {
        self.0.get(i / 32).is_some_and(|l| (l >> (i % 32)) & 1 == 1)
    }

    fn mul(&self, o: &Self) -> Self {
        let mut r = vec![0u32; self.0.len() + o.0.len() + 1];
        for (i, &a) in self.0.iter().enumerate() {
            let mut carry = 0u64;
            for (j, &b) in o.0.iter().enumerate() {
                let t = u64::from(a) * u64::from(b) + u64::from(r[i + j]) + carry;
                r[i + j] = t as u32;
                carry = t >> 32;
            }
            let mut k = i + o.0.len();
            while carry > 0 {
                let t = u64::from(r[k]) + carry;
                r[k] = t as u32;
                carry = t >> 32;
                k += 1;
            }
        }
        let mut n = Self(r);
        n.trim();
        n
    }

    /// `self mod m` by binary long division.
    fn rem(&self, m: &Self) -> Self {
        let width = m.0.len() + 1;
        let mut r = vec![0u32; width];
        for i in (0..self.bits()).rev() {
            // r = r << 1 | bit
            let mut carry = u32::from(self.bit(i));
            for limb in &mut r {
                let next = *limb >> 31;
                *limb = (*limb << 1) | carry;
                carry = next;
            }
            if !less(&r, &m.0) {
                sub(&mut r, &m.0);
            }
        }
        let mut n = Self(r);
        n.trim();
        n
    }

    /// `self ^ e mod m`.
    #[must_use]
    pub fn pow_mod(&self, e: &Self, m: &Self) -> Self {
        let mut result = Self(vec![1]).rem(m);
        let base = self.rem(m);
        for i in (0..e.bits()).rev() {
            result = result.mul(&result).rem(m);
            if e.bit(i) {
                result = result.mul(&base).rem(m);
            }
        }
        result
    }
}

/// `a < b`, both little-endian limbs (any lengths).
fn less(a: &[u32], b: &[u32]) -> bool {
    let n = a.len().max(b.len());
    for i in (0..n).rev() {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x < y;
        }
    }
    false
}

/// `a -= b`, `a >= b`.
fn sub(a: &mut [u32], b: &[u32]) {
    let mut borrow = 0i64;
    for (i, limb) in a.iter_mut().enumerate() {
        let t = i64::from(*limb) - i64::from(b.get(i).copied().unwrap_or(0)) - borrow;
        if t < 0 {
            *limb = (t + (1i64 << 32)) as u32;
            borrow = 1;
        } else {
            *limb = t as u32;
            borrow = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_powers() {
        let n = |v: u64| Num::from_be(&v.to_be_bytes());
        assert_eq!(n(4).pow_mod(&n(13), &n(497)), n(445));
        assert_eq!(n(2).pow_mod(&n(64), &n(1_000_000_007)), n(582_344_008));
        let big = Num::from_be(&[0xFF; 40]);
        assert_eq!(big.to_be(40), vec![0xFF; 40]);
    }
}
