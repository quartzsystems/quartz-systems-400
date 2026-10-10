// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Unsigned integers as little-endian decimal digit vectors: digit `i`
//! is the `10^i` place. A normalized vector has no high zeros; zero is
//! empty. Every function here takes normalized input and returns
//! normalized output.

use std::cmp::Ordering;

/// A magnitude.
pub type Digits = Vec<u8>;

/// Drop high zeros.
pub fn trim(v: &mut Digits) {
    while v.last() == Some(&0) {
        v.pop();
    }
}

/// From a machine integer.
#[must_use]
pub fn from_u128(mut n: u128) -> Digits {
    let mut v = Vec::new();
    while n > 0 {
        v.push(u8::try_from(n % 10).expect("digit"));
        n /= 10;
    }
    v
}

/// To a machine integer, when it fits.
#[must_use]
pub fn to_u128(d: &[u8]) -> Option<u128> {
    let mut n: u128 = 0;
    for &x in d.iter().rev() {
        n = n.checked_mul(10)?.checked_add(u128::from(x))?;
    }
    Some(n)
}

/// Compare two magnitudes.
#[must_use]
pub fn cmp(a: &[u8], b: &[u8]) -> Ordering {
    a.len()
        .cmp(&b.len())
        .then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

/// `a + b`.
#[must_use]
pub fn add(a: &[u8], b: &[u8]) -> Digits {
    let mut out = Vec::with_capacity(a.len().max(b.len()) + 1);
    let mut carry = 0u8;
    for i in 0..a.len().max(b.len()) {
        let s = a.get(i).copied().unwrap_or(0) + b.get(i).copied().unwrap_or(0) + carry;
        out.push(s % 10);
        carry = s / 10;
    }
    if carry > 0 {
        out.push(carry);
    }
    out
}

/// `a - b` for `a >= b`.
#[must_use]
pub fn sub(a: &[u8], b: &[u8]) -> Digits {
    debug_assert_ne!(cmp(a, b), Ordering::Less);
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0u8;
    for (i, &digit) in a.iter().enumerate() {
        let mut x = digit;
        let y = b.get(i).copied().unwrap_or(0) + borrow;
        if x < y {
            x += 10;
            borrow = 1;
        } else {
            borrow = 0;
        }
        out.push(x - y);
    }
    trim(&mut out);
    out
}

/// `a * b`.
#[must_use]
pub fn mul(a: &[u8], b: &[u8]) -> Digits {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut acc = vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        for (j, &y) in b.iter().enumerate() {
            acc[i + j] += u32::from(x) * u32::from(y);
        }
    }
    let mut out = Vec::with_capacity(acc.len());
    let mut carry = 0u32;
    for v in acc {
        let s = v + carry;
        out.push(u8::try_from(s % 10).expect("digit"));
        carry = s / 10;
    }
    while carry > 0 {
        out.push(u8::try_from(carry % 10).expect("digit"));
        carry /= 10;
    }
    trim(&mut out);
    out
}

/// `(a / b, a % b)` for `b != 0`, by schoolbook long division.
#[must_use]
pub fn divmod(a: &[u8], b: &[u8]) -> (Digits, Digits) {
    assert!(!b.is_empty(), "division by zero");
    if cmp(a, b) == Ordering::Less {
        return (Vec::new(), a.to_vec());
    }
    let mut q = vec![0u8; a.len()];
    let mut rem: Digits = Vec::new();
    for i in (0..a.len()).rev() {
        rem.insert(0, a[i]);
        trim(&mut rem);
        let mut digit = 0u8;
        while cmp(&rem, b) != Ordering::Less {
            rem = sub(&rem, b);
            digit += 1;
        }
        q[i] = digit;
    }
    trim(&mut q);
    (q, rem)
}

/// `a * 10^n`.
#[must_use]
pub fn shl(a: &[u8], n: usize) -> Digits {
    if a.is_empty() {
        return Vec::new();
    }
    let mut v = vec![0u8; n];
    v.extend_from_slice(a);
    v
}

/// `(a / 10^n, a % 10^n)`: the kept high part and the dropped low part.
/// The dropped part keeps its length so its first digit is `a[n - 1]`.
#[must_use]
pub fn shr(a: &[u8], n: usize) -> (Digits, Vec<u8>) {
    if n >= a.len() {
        let mut dropped = vec![0u8; n];
        dropped[..a.len()].copy_from_slice(a);
        return (Vec::new(), dropped);
    }
    let mut kept = a[n..].to_vec();
    trim(&mut kept);
    (kept, a[..n].to_vec())
}

/// `a + 1`.
#[must_use]
pub fn add_one(a: &[u8]) -> Digits {
    add(a, &[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(n: u128) -> Digits {
        from_u128(n)
    }

    #[test]
    fn round_trips_and_compare() {
        assert_eq!(d(0), Vec::<u8>::new());
        assert_eq!(d(1230), vec![0, 3, 2, 1]);
        assert_eq!(to_u128(&d(u128::MAX)), Some(u128::MAX));
        assert_eq!(cmp(&d(99), &d(100)), Ordering::Less);
        assert_eq!(cmp(&d(100), &d(100)), Ordering::Equal);
        assert_eq!(cmp(&d(101), &d(100)), Ordering::Greater);
    }

    #[test]
    fn arithmetic_matches_u128() {
        let mut x: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for _ in 0..2000 {
            let lhs =
                u128::from(next()) % 10u128.pow(u32::from(u8::try_from(next() % 20).unwrap()));
            let rhs =
                u128::from(next()) % 10u128.pow(u32::from(u8::try_from(next() % 20).unwrap())) + 1;
            assert_eq!(to_u128(&add(&d(lhs), &d(rhs))), Some(lhs + rhs));
            assert_eq!(to_u128(&mul(&d(lhs), &d(rhs))), Some(lhs * rhs));
            let (quot, rem) = divmod(&d(lhs), &d(rhs));
            assert_eq!(to_u128(&quot), Some(lhs / rhs));
            assert_eq!(to_u128(&rem), Some(lhs % rhs));
            let (hi, lo) = if lhs >= rhs { (lhs, rhs) } else { (rhs, lhs) };
            assert_eq!(to_u128(&sub(&d(hi), &d(lo))), Some(hi - lo));
        }
    }

    #[test]
    fn shifts() {
        assert_eq!(to_u128(&shl(&d(12), 3)), Some(12000));
        assert!(shl(&[], 5).is_empty());
        let (k, dr) = shr(&d(12345), 2);
        assert_eq!(to_u128(&k), Some(123));
        assert_eq!(dr, vec![5, 4]);
        let (k, dr) = shr(&d(5), 3);
        assert!(k.is_empty());
        assert_eq!(dr, vec![5, 0, 0]);
        assert_eq!(to_u128(&add_one(&d(999))), Some(1000));
    }
}
