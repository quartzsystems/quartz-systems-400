// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! P and Q over whole pages: XOR, and sums of products in GF(2^8) with
//! the polynomial 0x11D and generator 2, and the recovery of one or two
//! unknown pages from them.

const fn tables() -> ([u8; 512], [u8; 256]) {
    let mut exp = [0u8; 512];
    let mut log = [0u8; 256];
    let mut x: u16 = 1;
    let mut i = 0;
    while i < 255 {
        #[allow(clippy::cast_possible_truncation)] // below 256 by construction
        {
            exp[i] = x as u8;
            log[x as usize] = i as u8;
        }
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11D;
        }
        i += 1;
    }
    while i < 512 {
        exp[i] = exp[i - 255];
        i += 1;
    }
    (exp, log)
}

static TABLES: ([u8; 512], [u8; 256]) = tables();

/// `a * b`.
#[must_use]
pub fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let (exp, log) = &TABLES;
    exp[usize::from(log[usize::from(a)]) + usize::from(log[usize::from(b)])]
}

/// The inverse of a non-zero `a`.
#[must_use]
pub fn inv(a: u8) -> u8 {
    assert!(a != 0, "zero has no inverse");
    let (exp, log) = &TABLES;
    exp[255 - usize::from(log[usize::from(a)])]
}

/// The Q coefficient of member `m`: `2^m`.
#[must_use]
pub fn coef(m: usize) -> u8 {
    TABLES.0[m % 255]
}

/// `dst ^= src`.
pub fn xor_into(dst: &mut [u8], src: &[u8]) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d ^= s;
    }
}

/// `dst ^= c * src`.
pub fn mul_xor_into(dst: &mut [u8], src: &[u8], c: u8) {
    if c == 1 {
        xor_into(dst, src);
        return;
    }
    let t: [u8; 256] = std::array::from_fn(|x| mul(c, u8::try_from(x).expect("byte")));
    for (d, s) in dst.iter_mut().zip(src) {
        *d ^= t[usize::from(*s)];
    }
}

/// `buf *= c`.
pub fn scale(buf: &mut [u8], c: u8) {
    let t: [u8; 256] = std::array::from_fn(|x| mul(c, u8::try_from(x).expect("byte")));
    for b in buf.iter_mut() {
        *b = t[usize::from(*b)];
    }
}

/// P and, when `want_q`, Q of pages of `len` bytes; `coefs[i]` is the
/// coefficient of `data[i]`.
#[must_use]
pub fn compute(data: &[&[u8]], coefs: &[u8], len: usize, want_q: bool) -> (Vec<u8>, Option<Vec<u8>>) {
    let mut p = vec![0u8; len];
    let mut q = want_q.then(|| vec![0u8; len]);
    for (d, &c) in data.iter().zip(coefs) {
        xor_into(&mut p, d);
        if let Some(q) = q.as_mut() {
            mul_xor_into(q, d, c);
        }
    }
    (p, q)
}

/// Fill in the unknown pages of `data` from P and Q. Returns false when
/// there are more unknowns than known parity pages allow.
#[must_use]
pub fn solve(data: &mut [Option<Vec<u8>>], coefs: &[u8], p: Option<&[u8]>, q: Option<&[u8]>) -> bool {
    let unknown: Vec<usize> = (0..data.len()).filter(|&i| data[i].is_none()).collect();
    // What P and Q leave once the known pages are taken out.
    let rest = |with_q: bool| {
        let mut pr = p.map(<[u8]>::to_vec);
        let mut qr = q.filter(|_| with_q).map(<[u8]>::to_vec);
        for (i, d) in data.iter().enumerate() {
            if let Some(d) = d {
                if let Some(pr) = pr.as_mut() {
                    xor_into(pr, d);
                }
                if let Some(qr) = qr.as_mut() {
                    mul_xor_into(qr, d, coefs[i]);
                }
            }
        }
        (pr, qr)
    };
    match unknown.as_slice() {
        [] => true,
        [x] => {
            let (pr, qr) = rest(p.is_none());
            if let Some(pr) = pr {
                data[*x] = Some(pr);
            } else if let Some(mut qr) = qr {
                scale(&mut qr, inv(coefs[*x]));
                data[*x] = Some(qr);
            } else {
                return false;
            }
            true
        }
        [x, y] => {
            let (Some(pxy), Some(mut qxy)) = rest(true) else {
                return false;
            };
            // D_x = (Qxy + c_y Pxy) / (c_x + c_y); D_y = Pxy + D_x.
            mul_xor_into(&mut qxy, &pxy, coefs[*y]);
            scale(&mut qxy, inv(coefs[*x] ^ coefs[*y]));
            let mut dy = pxy;
            xor_into(&mut dy, &qxy);
            data[*x] = Some(qxy);
            data[*y] = Some(dy);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(seed: u8, len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251).to_le_bytes()[0].wrapping_mul(31).wrapping_add(seed.wrapping_mul(97))).collect()
    }

    #[test]
    fn field_rules() {
        for a in 1..=255u8 {
            assert_eq!(mul(a, inv(a)), 1);
            assert_eq!(mul(a, 1), a);
        }
        assert_eq!(coef(0), 1);
        assert_eq!(coef(1), 2);
        assert_eq!(coef(8), 0x1D);
    }

    #[test]
    fn recovers_any_one_or_two() {
        let len = 64;
        let n = 6;
        let pages: Vec<Vec<u8>> = (0..n).map(|i| page(u8::try_from(i).unwrap() + 1, len)).collect();
        let coefs: Vec<u8> = (0..n).map(|m| coef(m + 2)).collect();
        let refs: Vec<&[u8]> = pages.iter().map(Vec::as_slice).collect();
        let (p, q) = compute(&refs, &coefs, len, true);
        let q = q.unwrap();
        for x in 0..n {
            for y in x..n {
                let mut d: Vec<Option<Vec<u8>>> = pages.iter().cloned().map(Some).collect();
                d[x] = None;
                d[y] = None;
                assert!(solve(&mut d, &coefs, Some(&p), Some(&q)));
                assert_eq!(d.iter().map(|x| x.clone().unwrap()).collect::<Vec<_>>(), pages);
                // One page from Q alone.
                if x == y {
                    let mut d: Vec<Option<Vec<u8>>> = pages.iter().cloned().map(Some).collect();
                    d[x] = None;
                    assert!(solve(&mut d, &coefs, None, Some(&q)));
                    assert_eq!(d[x].as_ref().unwrap(), &pages[x]);
                }
            }
        }
        let mut d: Vec<Option<Vec<u8>>> = pages.iter().cloned().map(Some).collect();
        d[0] = None;
        d[1] = None;
        assert!(!solve(&mut d, &coefs, Some(&p), None));
    }
}
