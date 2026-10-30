// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `memcpy`, `memmove`, `memset`, `memchr`, `memcmp`, `strlen`,
//! `strtok`, `atoi`, `atoll`, `sscanf` (integer conversions), `cvthc`
//! and `cvtch`.

use codegen::abi::Env;
use ifs::Errno;
use mi::{FatPtr, Job};

use crate::net::abi::{bound_c, fat, int, is_null, mem, raw, uint};

/// The bytes from a pointer to the end of its space.
fn rest(j: &Job, p: u128) -> Result<&'static mut [u8], Errno> {
    let (at, avail) = j.spaces.extent(fat(p)).map_err(|_| Errno::EFAULT)?;
    Ok(mi::storage::view(at, usize::try_from(avail).unwrap_or(usize::MAX)))
}

/// A NUL-terminated string's bytes, without the NUL.
fn cbytes(j: &Job, p: u128) -> Result<&'static mut [u8], Errno> {
    let b = rest(j, p)?;
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    Ok(&mut b[..n])
}

/// A length argument: C's 32-bit `size_t`, the register's upper half
/// whatever the caller left there.
fn len(n: i64) -> usize {
    usize::try_from(uint(n)).unwrap_or(0)
}

/// A pointer `n` bytes on from `p`, null when it cannot be formed.
fn plus(j: &Job, p: u128, n: usize) -> u128 {
    j.spaces
        .add(fat(p), i64::try_from(n).unwrap_or(0))
        .map_or_else(|_| raw(FatPtr::null()), raw)
}

fn signal(j: &mut Job) {
    j.errno.set(Errno::EFAULT.code());
}

fn memcpy(j: &mut Job, dst: u128, _pad: i64, src: u128, n: i64) -> u128 {
    let n = len(n);
    if n > 0 {
        match (mem(j, src, n), mem(j, dst, n)) {
            (Ok(s), Ok(d)) => {
                let tmp = s.to_vec();
                d.copy_from_slice(&tmp);
            }
            _ => signal(j),
        }
    }
    dst
}

fn memset(j: &mut Job, dst: u128, c: i64, n: i64) -> u128 {
    match mem(j, dst, len(n)) {
        Ok(d) => d.fill(c.to_le_bytes()[0]),
        Err(_) => signal(j),
    }
    dst
}

fn memchr(j: &mut Job, s: u128, c: i64, n: i64) -> u128 {
    let Ok(b) = mem(j, s, len(n)) else {
        signal(j);
        return raw(FatPtr::null());
    };
    let c = c.to_le_bytes()[0];
    match b.iter().position(|&x| x == c) {
        Some(i) => plus(j, s, i),
        None => raw(FatPtr::null()),
    }
}

fn memcmp(j: &mut Job, a: u128, _pad: i64, b: u128, n: i64) -> i64 {
    match (mem(j, a, len(n)), mem(j, b, len(n))) {
        (Ok(x), Ok(y)) => x.iter().zip(y.iter()).find(|(p, q)| p != q).map_or(0, |(p, q)| i64::from(*p) - i64::from(*q)),
        _ => {
            signal(j);
            0
        }
    }
}

fn strlen(j: &mut Job, s: u128) -> i64 {
    match cbytes(j, s) {
        Ok(b) => i64::try_from(b.len()).unwrap_or(0),
        Err(_) => {
            signal(j);
            0
        }
    }
}

/// `strtok(s, delims)`: the next token, NUL-terminated in place; a null
/// `s` goes on where the last call stopped.
fn strtok(j: &mut Job, s: u128, delims: u128) -> u128 {
    let null = raw(FatPtr::null());
    let start = if is_null(s) {
        match super::with_state(j, |st| st.strtok) {
            Some(p) => raw(p),
            None => return null,
        }
    } else {
        s
    };
    let (Ok(d), Ok(b)) = (cbytes(j, delims), cbytes(j, start)) else {
        signal(j);
        return null;
    };
    let d = d.to_vec();
    let Some(first) = b.iter().position(|c| !d.contains(c)) else {
        super::with_state(j, |st| st.strtok = None);
        return null;
    };
    let end = b[first..].iter().position(|c| d.contains(c)).map(|i| first + i);
    let tok = plus(j, start, first);
    let next = match end {
        Some(e) => {
            b[e] = 0;
            Some(fat(plus(j, start, e + 1)))
        }
        None => None,
    };
    super::with_state(j, |st| st.strtok = next);
    tok
}

fn is_space(c: u8) -> bool {
    matches!(c, 0x40 | 0x05 | 0x25 | 0x15 | 0x0B | 0x0C | 0x0D)
}

/// The number a string starts with, C's way: blanks, a sign, digits.
fn leading_number(b: &[u8]) -> i64 {
    let mut i = b.iter().position(|&c| !is_space(c)).unwrap_or(b.len());
    let neg = b.get(i) == Some(&0x60);
    if matches!(b.get(i), Some(0x60 | 0x4E)) {
        i += 1;
    }
    let mut v: i64 = 0;
    while let Some(&c) = b.get(i) {
        if !(0xF0..=0xF9).contains(&c) {
            break;
        }
        v = v.wrapping_mul(10).wrapping_add(i64::from(c - 0xF0));
        i += 1;
    }
    if neg { -v } else { v }
}

fn atoi(j: &mut Job, s: u128) -> i64 {
    match cbytes(j, s) {
        Ok(b) => i64::from(int(leading_number(b))),
        Err(_) => {
            signal(j);
            0
        }
    }
}

fn atoll(j: &mut Job, s: u128) -> i64 {
    match cbytes(j, s) {
        Ok(b) => leading_number(b),
        Err(_) => {
            signal(j);
            0
        }
    }
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// `cvthc(hex, chars, n)`: `n` hexadecimal digits for the bytes.
fn cvthc(j: &mut Job, out: u128, _pad: i64, input: u128, n: i64) -> i64 {
    let n = len(n);
    let (Ok(o), Ok(i)) = (mem(j, out, n), mem(j, input, n.div_ceil(2))) else {
        signal(j);
        return 0;
    };
    for (k, slot) in o.iter_mut().enumerate() {
        let b = i[k / 2];
        let nib = if k % 2 == 0 { b >> 4 } else { b & 15 };
        *slot = ccsid::byte_of(char::from(HEX[usize::from(nib)])).unwrap_or(0x40);
    }
    0
}

/// `cvtch(chars, hex, n)`: the bytes `n` hexadecimal digits stand for.
fn cvtch(j: &mut Job, out: u128, _pad: i64, input: u128, n: i64) -> i64 {
    let n = len(n);
    let (Ok(o), Ok(i)) = (mem(j, out, n.div_ceil(2)), mem(j, input, n)) else {
        signal(j);
        return 0;
    };
    o.fill(0);
    for (k, &c) in i.iter().enumerate() {
        let nib = match c {
            0xF0..=0xF9 => c - 0xF0,
            0xC1..=0xC6 => c - 0xC1 + 10,
            0x81..=0x86 => c - 0x81 + 10,
            _ => 0,
        };
        o[k / 2] |= if k % 2 == 0 { nib << 4 } else { nib };
    }
    0
}

/// `sscanf(str, format, ...)` for integer conversions (`%d %i %u %x
/// %X %o`, with `h`, `l`, `ll`) and literal text: the conversions
/// stored.
fn sscanf(j: &mut Job, argc: i64, s: u128, f: u128, out: [u128; 4]) -> i64 {
    let (Ok(sb), Ok(fb)) = (cbytes(j, s), cbytes(j, f)) else {
        signal(j);
        return -1;
    };
    let text: Vec<char> = ccsid::from_ebcdic(sb).chars().collect();
    let fmt: Vec<char> = ccsid::from_ebcdic(fb).chars().collect();
    let slots = usize::try_from(int(argc) - 2).unwrap_or(0).min(out.len());
    let (mut ti, mut fi, mut stored) = (0usize, 0usize, 0usize);
    while fi < fmt.len() {
        let c = fmt[fi];
        if c.is_whitespace() {
            while text.get(ti).is_some_and(|c| c.is_whitespace()) {
                ti += 1;
            }
            fi += 1;
            continue;
        }
        if c != '%' {
            if text.get(ti) != Some(&c) {
                break;
            }
            ti += 1;
            fi += 1;
            continue;
        }
        fi += 1;
        let mut wide = 4;
        while let Some(&m) = fmt.get(fi) {
            match m {
                'h' => wide = 2,
                'l' => wide = if wide == 8 || fmt.get(fi + 1) == Some(&'l') { 8 } else { 4 },
                _ => break,
            }
            fi += 1;
        }
        let Some(&conv) = fmt.get(fi) else { break };
        fi += 1;
        let radix = match conv {
            'd' | 'i' | 'u' => 10,
            'x' | 'X' => 16,
            'o' => 8,
            _ => break,
        };
        while text.get(ti).is_some_and(|c| c.is_whitespace()) {
            ti += 1;
        }
        let neg = text.get(ti) == Some(&'-');
        if matches!(text.get(ti), Some('-' | '+')) {
            ti += 1;
        }
        if radix == 16 && text.get(ti) == Some(&'0') && matches!(text.get(ti + 1), Some('x' | 'X')) {
            ti += 2;
        }
        let from = ti;
        let mut v: i64 = 0;
        while let Some(d) = text.get(ti).and_then(|c| c.to_digit(radix)) {
            v = v.wrapping_mul(i64::from(radix)).wrapping_add(i64::from(d));
            ti += 1;
        }
        if ti == from || stored >= slots {
            break;
        }
        let v = if neg { -v } else { v };
        let Ok(b) = mem(j, out[stored], wide) else {
            signal(j);
            break;
        };
        let bytes = v.to_be_bytes();
        b.copy_from_slice(&bytes[8 - wide..]);
        stored += 1;
    }
    if stored == 0 && text.is_empty() { -1 } else { i64::try_from(stored).unwrap_or(0) }
}

#[allow(clippy::too_many_arguments)]
extern "sysv64" fn b_sscanf(env: *mut Env, argc: i64, _od: i64, s: u128, f: u128, a: u128, b: u128, c: u128, d: u128) -> i64 {
    sscanf(mi::storage::job_of(env), argc, s, f, [a, b, c, d])
}

// Generated code passes a second pointer that finds one register left
// on the stack and leaves that register unused, so the integer after it
// goes on the stack too; Rust would put that integer in the register.
// The pad takes the register, so both agree (ABI.md §3).
bound_c!(b_memcpy(d: u128, pad: i64, s: u128, n: i64) -> u128 = memcpy);
bound_c!(b_memset(d: u128, c: i64, n: i64) -> u128 = memset);
bound_c!(b_memchr(s: u128, c: i64, n: i64) -> u128 = memchr);
bound_c!(b_memcmp(a: u128, pad: i64, b: u128, n: i64) -> i64 = memcmp);
bound_c!(b_strlen(s: u128) -> i64 = strlen);
bound_c!(b_strtok(s: u128, d: u128) -> u128 = strtok);
bound_c!(b_atoi(s: u128) -> i64 = atoi);
bound_c!(b_atoll(s: u128) -> i64 = atoll);
bound_c!(b_cvthc(o: u128, pad: i64, i: u128, n: i64) -> i64 = cvthc);
bound_c!(b_cvtch(o: u128, pad: i64, i: u128, n: i64) -> i64 = cvtch);

pub(super) fn procedures() -> Vec<(&'static str, usize)> {
    macro_rules! at {
        ($f:ident) => {
            $f as *const () as usize
        };
    }
    vec![
        ("memcpy", at!(b_memcpy)),
        // memcpy copies through a buffer, so overlap is safe.
        ("memmove", at!(b_memcpy)),
        ("memset", at!(b_memset)),
        ("memchr", at!(b_memchr)),
        ("memcmp", at!(b_memcmp)),
        ("strlen", at!(b_strlen)),
        ("strtok", at!(b_strtok)),
        ("atoi", at!(b_atoi)),
        ("atoll", at!(b_atoll)),
        ("cvthc", at!(b_cvthc)),
        ("cvtch", at!(b_cvtch)),
        ("sscanf", at!(b_sscanf)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_read_c_s_way() {
        let e = |s: &str| ccsid::to_ebcdic(s).unwrap();
        assert_eq!(leading_number(&e("  -42abc")), -42);
        assert_eq!(leading_number(&e("+7")), 7);
        assert_eq!(leading_number(&e("x1")), 0);
        assert_eq!(leading_number(&e("2130706433")), 2_130_706_433);
    }
}
