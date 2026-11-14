// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LBER (`QSYSINC/H(LBER)`): `BerElement`s for encoding (`ber_alloc_t`,
//! `ber_printf`, `ber_flatten`) and decoding (`ber_init`, `ber_scanf`,
//! the tag and element walkers), and `berval` copies. Strings are bytes
//! as the program gives them; BER does not convert them.
//!
//! `ber_printf` letters: `t` tag (int) for the next element, `b`
//! boolean, `i` integer, `e` enumerated (ints), `n` null, `o` octets
//! (pointer and length), `O` a `berval *`, `s` a string, `B` a bit
//! string (pointer and bit count), `v` a `char **`, `V` a `berval **`,
//! `{` `}` a sequence, `[` `]` a set.
//!
//! `ber_scanf` letters: `a` a string allocated (`char **`), `s` a
//! string into a buffer (`char *`, `ber_len_t *` its room, then the
//! length), `O` a `berval` allocated (`struct berval **`), `o` a
//! `berval` filled (`struct berval *`), `b` `e` `i` (`int *`), `B` a bit
//! string allocated (`char **`, `ber_len_t *` bits), `n` null, `l` the
//! next element's length and `t` (or `T`) its tag without reading it,
//! `x` skip an element, `v` a sequence of strings (`char ***`), `V` of
//! `berval`s (`struct berval ***`), `{` `}` `[` `]`.

use mi::Job;

use super::{free_bervals, free_block, give_berval, give_bervals, null, ptr, with};
use crate::consts::*;
use crate::mem::{self, Image};
use crate::state::{Ber, State};

/// One variadic argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arg {
    /// An integer.
    Int(i64),
    /// A pointer.
    Ptr(u128),
}

/// What a format letter takes, in order: `i` an integer, `p` a pointer.
#[must_use]
pub fn takes(letter: char, printf: bool) -> &'static str {
    if printf {
        match letter {
            't' | 'b' | 'i' | 'e' => "i",
            'o' | 'B' => "pi",
            'O' | 's' | 'v' | 'V' => "p",
            _ => "",
        }
    } else {
        match letter {
            's' | 'B' => "pp",
            'a' | 'O' | 'o' | 'b' | 'e' | 'i' | 'l' | 't' | 'T' | 'v' | 'V' => "p",
            _ => "",
        }
    }
}

/// The arguments a format reads from the words of the call's stack
/// area, `first` the slot number of the first word: an integer takes a
/// slot, a pointer an aligned pair.
#[must_use]
pub fn args_from(fmt: &str, printf: bool, words: &[i64], first: usize) -> Vec<Arg> {
    let mut out = Vec::new();
    let mut slot = first;
    let word = |s: usize| words.get(s - first).copied().unwrap_or(0);
    for c in fmt.chars() {
        for k in takes(c, printf).chars() {
            if k == 'i' {
                out.push(Arg::Int(word(slot)));
                slot += 1;
            } else {
                if slot % 2 == 1 {
                    slot += 1;
                }
                let lo = word(slot).cast_unsigned();
                let hi = word(slot + 1).cast_unsigned();
                out.push(Arg::Ptr(u128::from(lo) | (u128::from(hi) << 64)));
                slot += 2;
            }
        }
    }
    out
}

/// An element being encoded.
#[derive(Debug, Clone, Default)]
pub struct Writing {
    open: Vec<(u8, Vec<u8>)>,
    out: Vec<u8>,
    tag: Option<u8>,
}

fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut w = ldap::ber::Writer::new();
    w.octets(tag, content);
    w.into_bytes()
}

impl Writing {
    fn put(&mut self, dft: u8, content: &[u8]) {
        let t = self.tag.take().unwrap_or(dft);
        let b = tlv(t, content);
        match self.open.last_mut() {
            Some((_, buf)) => buf.extend_from_slice(&b),
            None => self.out.extend_from_slice(&b),
        }
    }

    /// The bytes encoded (closed elements only).
    #[must_use]
    pub fn flatten(&self) -> Vec<u8> {
        self.out.clone()
    }
}

/// An element being decoded.
#[derive(Debug, Clone, Default)]
pub struct Reading {
    /// The bytes.
    pub data: Vec<u8>,
    /// Where the next element starts.
    pub pos: usize,
    /// Ends of the constructed elements entered.
    pub ends: Vec<usize>,
    tag: Option<u8>,
}

impl Reading {
    /// Reading `data`.
    #[must_use]
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            ..Self::default()
        }
    }

    fn limit(&self) -> usize {
        self.ends.last().copied().unwrap_or(self.data.len())
    }

    /// The next element's tag, header length and content length.
    fn peek(&self) -> Option<(u8, usize, usize)> {
        if self.pos >= self.limit() {
            return None;
        }
        let (t, h, n) = ldap::ber::header(&self.data[self.pos..self.limit()]).ok()??;
        (self.pos + h + n <= self.limit()).then_some((t, h, n))
    }

    /// The next element's tag and contents, read.
    fn take(&mut self, want: Option<u8>) -> Option<(u8, Vec<u8>)> {
        let (t, h, n) = self.peek()?;
        let want = self.tag.take().or(want);
        if want.is_some_and(|w| w != t) {
            return None;
        }
        let c = self.data[self.pos + h..self.pos + h + n].to_vec();
        self.pos += h + n;
        Some((t, c))
    }

    fn enter(&mut self, want: u8) -> Option<u8> {
        let (t, h, n) = self.peek()?;
        let want = self.tag.take().unwrap_or(want);
        if t != want {
            return None;
        }
        self.pos += h;
        self.ends.push(self.pos + n);
        Some(t)
    }

    fn leave(&mut self) -> Option<()> {
        let end = self.ends.pop()?;
        (self.pos == end).then_some(())
    }
}

fn int_of(c: &[u8]) -> Option<i64> {
    if c.is_empty() || c.len() > 8 {
        return None;
    }
    let mut v: i64 = if c[0] & 0x80 != 0 { -1 } else { 0 };
    for b in c {
        v = (v << 8) | i64::from(*b);
    }
    Some(v)
}

fn int_bytes(v: i64) -> Vec<u8> {
    let mut w = ldap::ber::Writer::new();
    w.integer(0x02, v);
    w.into_bytes()[2..].to_vec()
}

/// `BerElement *ber_alloc_t(int options)`.
pub fn alloc_t(j: &mut Job, _options: i64) -> u128 {
    with(j, |s, j| ptr(s.new_ber(j, Ber::Write(Writing::default()))))
}

/// `BerElement *ber_init(struct berval *bv)`: a copy of the bytes to
/// decode.
pub fn init(j: &mut Job, bv: u128) -> u128 {
    let Some(data) = mem::berval(j, bv) else {
        return null();
    };
    with(j, |s, j| ptr(s.new_ber(j, Ber::Read(Reading::new(data)))))
}

/// `void ber_init2(BerElement *ber, struct berval *bv, int options)`:
/// the element starts again, decoding `bv` (encoding when it is null).
pub fn init2(j: &mut Job, ber: u128, bv: u128, _options: i64) -> i64 {
    let data = mem::berval(j, bv);
    with(j, |s, j| {
        if let Some(n) = mem::native(j, ber)
            && let Some(b) = s.bers.get_mut(&n)
        {
            *b = match data {
                Some(d) => Ber::Read(Reading::new(d)),
                None => Ber::Write(Writing::default()),
            };
        }
        0
    })
}

fn with_ber<T>(
    j: &mut Job,
    ber: u128,
    bad: T,
    f: impl FnOnce(&mut State, &mut Job, u64) -> T,
) -> T {
    with(j, |s, j| {
        match mem::native(j, ber).filter(|n| s.bers.contains_key(n)) {
            Some(n) => f(s, j, n),
            None => bad,
        }
    })
}

/// `int ber_flatten(BerElement *ber, struct berval **bvPtr)`.
pub fn flatten(j: &mut Job, ber: u128, out: u128) -> i64 {
    with_ber(j, ber, -1, |s, j, n| {
        let bytes = match &s.bers[&n] {
            Ber::Write(w) => w.flatten(),
            Ber::Read(r) => r.data.clone(),
            Ber::Attrs(_) => return -1,
        };
        let p = give_berval(s, j, &bytes);
        if mem::put_ptr(j, out, p) { 0 } else { -1 }
    })
}

/// `void ber_free(BerElement *ber, int freebuf)`.
pub fn free(j: &mut Job, ber: u128, freebuf: i64) -> i64 {
    super::entry::ber_free(j, ber, freebuf)
}

fn printf_into(j: &Job, w: &mut Writing, fmt: &str, args: &[Arg]) -> Option<()> {
    let mut seq = args.iter().copied();
    for c in fmt.chars() {
        match c {
            't' => {
                let Some(Arg::Int(v)) = seq.next() else {
                    return None;
                };
                w.tag = Some(u8::try_from(v & 0xFF).ok()?);
            }
            'b' => {
                let Some(Arg::Int(v)) = seq.next() else {
                    return None;
                };
                w.put(LBER_BOOLEAN as u8, &[if v != 0 { 0xFF } else { 0 }]);
            }
            'i' | 'e' => {
                let Some(Arg::Int(v)) = seq.next() else {
                    return None;
                };
                let v = i64::from(mem::int(v));
                let t = if c == 'i' {
                    LBER_INTEGER
                } else {
                    LBER_ENUMERATED
                };
                w.put(t as u8, &int_bytes(v));
            }
            'n' => w.put(LBER_NULL as u8, &[]),
            'o' => {
                let (Some(Arg::Ptr(p)), Some(Arg::Int(n))) = (seq.next(), seq.next()) else {
                    return None;
                };
                let n = mem::uint(n) as usize;
                let b = if n == 0 {
                    Vec::new()
                } else {
                    mem::bytes(j, p, n)?.to_vec()
                };
                w.put(LBER_OCTETSTRING as u8, &b);
            }
            'O' => {
                let Some(Arg::Ptr(p)) = seq.next() else {
                    return None;
                };
                w.put(LBER_OCTETSTRING as u8, &mem::berval(j, p)?);
            }
            's' => {
                let Some(Arg::Ptr(p)) = seq.next() else {
                    return None;
                };
                w.put(LBER_OCTETSTRING as u8, &mem::cbytes(j, p)?);
            }
            'B' => {
                let (Some(Arg::Ptr(p)), Some(Arg::Int(bits))) = (seq.next(), seq.next()) else {
                    return None;
                };
                let bits = mem::uint(bits) as usize;
                let n = bits.div_ceil(8);
                let mut b = vec![u8::try_from((n * 8 - bits) % 8).ok()?];
                if n > 0 {
                    b.extend_from_slice(mem::bytes(j, p, n)?);
                }
                w.put(LBER_BITSTRING as u8, &b);
            }
            'v' => {
                let Some(Arg::Ptr(p)) = seq.next() else {
                    return None;
                };
                for e in mem::ptr_array(j, p) {
                    w.put(LBER_OCTETSTRING as u8, &mem::cbytes(j, e)?);
                }
            }
            'V' => {
                let Some(Arg::Ptr(p)) = seq.next() else {
                    return None;
                };
                for e in mem::ptr_array(j, p) {
                    w.put(LBER_OCTETSTRING as u8, &mem::berval(j, e)?);
                }
            }
            '{' | '[' => {
                let dft = if c == '{' { LBER_SEQUENCE } else { LBER_SET } as u8;
                let t = w.tag.take().unwrap_or(dft);
                w.open.push((t, Vec::new()));
            }
            '}' | ']' => {
                let (t, buf) = w.open.pop()?;
                let b = tlv(t, &buf);
                match w.open.last_mut() {
                    Some((_, o)) => o.extend_from_slice(&b),
                    None => w.out.extend_from_slice(&b),
                }
            }
            ' ' | '\t' | '\n' => {}
            _ => return None,
        }
    }
    Some(())
}

/// `int ber_printf(BerElement *ber, const char *fmt, ...)`: 0, or -1
/// for a bad format or argument.
pub fn printf(j: &mut Job, ber: u128, fmt: &str, args: &[Arg]) -> i64 {
    with_ber(j, ber, -1, |s, j, n| {
        let Some(Ber::Write(w)) = s.bers.get_mut(&n) else {
            return -1;
        };
        let mut copy = w.clone();
        match printf_into(j, &mut copy, fmt, args) {
            Some(()) => {
                *w = copy;
                0
            }
            None => -1,
        }
    })
}

fn put_len(j: &Job, p: u128, n: usize) -> Option<()> {
    mem::put_u32(j, p, u32::try_from(n).ok()?).then_some(())
}

#[allow(clippy::too_many_lines)]
fn scanf_from(s: &mut State, j: &mut Job, r: &mut Reading, fmt: &str, args: &[Arg]) -> Option<u32> {
    let mut seq = args.iter().copied();
    let mut next = || match seq.next() {
        Some(Arg::Ptr(p)) => Some(p),
        _ => None,
    };
    let mut last = 0u32;
    for c in fmt.chars() {
        match c {
            'a' => {
                let out = next()?;
                let (t, b) = r.take(Some(LBER_OCTETSTRING as u8))?;
                let mut im = Image::default();
                im.cstr(&b);
                let p = s.heap.place(j, &im);
                mem::put_ptr(j, out, p).then_some(())?;
                last = u32::from(t);
            }
            's' => {
                let (buf, lenp) = (next()?, next()?);
                let room = mem::get_u32(j, lenp)? as usize;
                let (t, b) = r.take(Some(LBER_OCTETSTRING as u8))?;
                if b.len() + 1 > room {
                    return None;
                }
                let o = mem::bytes(j, buf, b.len() + 1)?;
                o[..b.len()].copy_from_slice(&b);
                o[b.len()] = 0;
                put_len(j, lenp, b.len())?;
                last = u32::from(t);
            }
            'O' => {
                let out = next()?;
                let (t, b) = r.take(Some(LBER_OCTETSTRING as u8))?;
                let p = give_berval(s, j, &b);
                mem::put_ptr(j, out, p).then_some(())?;
                last = u32::from(t);
            }
            'o' => {
                let out = next()?;
                let (t, b) = r.take(Some(LBER_OCTETSTRING as u8))?;
                let mut im = Image::default();
                im.data(&b);
                let d = s.heap.place(j, &im);
                let bv = mem::bytes(j, out, mem::BERVAL)?;
                bv[0..4].copy_from_slice(&u32::try_from(b.len()).ok()?.to_be_bytes());
                bv[16..32].copy_from_slice(&d.to_bytes());
                last = u32::from(t);
            }
            'b' => {
                let out = next()?;
                let (t, b) = r.take(Some(LBER_BOOLEAN as u8))?;
                mem::put_i32(j, out, i32::from(b.first().is_some_and(|v| *v != 0)));
                last = u32::from(t);
            }
            'e' | 'i' => {
                let out = next()?;
                let want = if c == 'i' {
                    LBER_INTEGER
                } else {
                    LBER_ENUMERATED
                } as u8;
                let (t, b) = r.take(Some(want))?;
                let v = i32::try_from(int_of(&b)?).ok()?;
                mem::put_i32(j, out, v);
                last = u32::from(t);
            }
            'B' => {
                let (out, bitsp) = (next()?, next()?);
                let (t, b) = r.take(Some(LBER_BITSTRING as u8))?;
                let (&unused, bits) = b.split_first()?;
                let n = bits.len() * 8 - usize::from(unused);
                let mut im = Image::default();
                im.data(bits);
                let p = s.heap.place(j, &im);
                mem::put_ptr(j, out, p).then_some(())?;
                put_len(j, bitsp, n)?;
                last = u32::from(t);
            }
            'n' => {
                let (t, _) = r.take(Some(LBER_NULL as u8))?;
                last = u32::from(t);
            }
            'l' => {
                let out = next()?;
                let (_, _, n) = r.peek()?;
                put_len(j, out, n)?;
            }
            't' | 'T' => {
                let out = next()?;
                let (t, _, _) = r.peek()?;
                mem::put_u32(j, out, u32::from(t));
                r.tag = Some(t);
                last = u32::from(t);
            }
            'x' => {
                let (t, _) = r.take(None)?;
                last = u32::from(t);
            }
            'v' | 'V' => {
                let out = next()?;
                let t = r.enter(LBER_SEQUENCE as u8)?;
                let mut items = Vec::new();
                while r.pos < r.limit() {
                    items.push(r.take(None)?.1);
                }
                r.leave()?;
                let p = if c == 'v' {
                    let mut im = Image::default();
                    im.strings(&items);
                    s.heap.place(j, &im)
                } else {
                    give_bervals(s, j, &items)
                };
                mem::put_ptr(j, out, p).then_some(())?;
                last = u32::from(t);
            }
            '{' | '[' => {
                let dft = if c == '{' { LBER_SEQUENCE } else { LBER_SET } as u8;
                last = u32::from(r.enter(dft)?);
            }
            '}' | ']' => r.leave()?,
            ' ' | '\t' | '\n' => {}
            _ => return None,
        }
    }
    Some(last)
}

/// `ber_tag_t ber_scanf(BerElement *ber, const char *fmt, ...)`: the
/// last tag read, `LBER_ERROR` when the element does not match.
pub fn scanf(j: &mut Job, ber: u128, fmt: &str, args: &[Arg]) -> i64 {
    let err = i64::from(LBER_ERROR);
    with_ber(j, ber, err, |s, j, n| {
        let Some(Ber::Read(r)) = s.bers.get(&n) else {
            return err;
        };
        let mut copy = r.clone();
        match scanf_from(s, j, &mut copy, fmt, args) {
            Some(t) => {
                s.bers.insert(n, Ber::Read(copy));
                i64::from(t)
            }
            None => err,
        }
    })
}

fn reading<T>(j: &mut Job, ber: u128, bad: T, f: impl FnOnce(&mut Job, &mut Reading) -> T) -> T {
    with(j, |s, j| {
        let Some(n) = mem::native(j, ber) else {
            return bad;
        };
        match s.bers.get_mut(&n) {
            Some(Ber::Read(r)) => f(j, r),
            _ => bad,
        }
    })
}

/// `ber_tag_t ber_peek_tag(BerElement *ber, ber_len_t *len)`.
pub fn peek_tag(j: &mut Job, ber: u128, lenp: u128) -> i64 {
    reading(j, ber, i64::from(LBER_ERROR), |j, r| match r.peek() {
        Some((t, _, n)) => {
            let _ = put_len(j, lenp, n);
            i64::from(t)
        }
        None => i64::from(LBER_ERROR),
    })
}

/// `ber_tag_t ber_skip_tag(BerElement *ber, ber_len_t *len)`: the tag
/// and length are read; the contents are next.
pub fn skip_tag(j: &mut Job, ber: u128, lenp: u128) -> i64 {
    reading(j, ber, i64::from(LBER_ERROR), |j, r| match r.peek() {
        Some((t, h, n)) => {
            r.pos += h;
            let _ = put_len(j, lenp, n);
            i64::from(t)
        }
        None => i64::from(LBER_ERROR),
    })
}

/// `ber_tag_t ber_get_tag(BerElement *ber)`: as `ber_skip_tag`
/// without the length.
pub fn get_tag(j: &mut Job, ber: u128) -> i64 {
    skip_tag(j, ber, 0)
}

/// `ber_tag_t ber_skip_element(BerElement *ber, struct berval *bv)`:
/// the next element is passed over; `bv`, when given, gets its
/// contents.
pub fn skip_element(j: &mut Job, ber: u128, bv: u128) -> i64 {
    let got = reading(j, ber, None, |_, r| r.take(None));
    let Some((t, c)) = got else {
        return i64::from(LBER_ERROR);
    };
    if !mem::is_null(bv) {
        with(j, |s, j| {
            let mut im = Image::default();
            im.data(&c);
            let d = s.heap.place(j, &im);
            if let Some(b) = mem::bytes(j, bv, mem::BERVAL) {
                b[0..4].copy_from_slice(&u32::try_from(c.len()).unwrap_or(0).to_be_bytes());
                b[16..32].copy_from_slice(&d.to_bytes());
            }
        });
    }
    i64::from(t)
}

/// `ber_tag_t ber_first_element(BerElement *ber, ber_len_t *len, char
/// **last)`: enters the sequence or set; its first element's tag
/// (`LBER_DEFAULT` when empty). `*last` is a cookie for
/// `ber_next_element`.
pub fn first_element(j: &mut Job, ber: u128, lenp: u128, last: u128) -> i64 {
    let r = reading(j, ber, None, |j, r| {
        let (t, h, n) = r.peek()?;
        if t & 0x20 == 0 {
            return None;
        }
        r.pos += h;
        r.ends.push(r.pos + n);
        match r.peek() {
            Some((t, _, n)) => {
                let _ = put_len(j, lenp, n);
                Some(i64::from(t))
            }
            None => {
                r.ends.pop();
                Some(i64::from(LBER_ERROR))
            }
        }
    });
    mem::put_ptr(j, last, mem::fat(ber));
    r.unwrap_or(i64::from(LBER_ERROR))
}

/// `ber_tag_t ber_next_element(BerElement *ber, ber_len_t *len, char
/// *last)`: the next element's tag, `LBER_DEFAULT` after the last
/// (the sequence is left).
pub fn next_element(j: &mut Job, ber: u128, lenp: u128, _last: u128) -> i64 {
    reading(j, ber, i64::from(LBER_ERROR), |j, r| {
        if r.pos >= r.limit() {
            r.ends.pop();
            return i64::from(LBER_ERROR);
        }
        match r.peek() {
            Some((t, _, n)) => {
                let _ = put_len(j, lenp, n);
                i64::from(t)
            }
            None => i64::from(LBER_ERROR),
        }
    })
}

/// `struct berval *ber_bvdup(struct berval *bv)`.
pub fn bvdup(j: &mut Job, bv: u128) -> u128 {
    let Some(d) = mem::berval(j, bv) else {
        return null();
    };
    with(j, |s, j| ptr(give_berval(s, j, &d)))
}

/// `void ber_bvfree(struct berval *bv)`.
pub fn bvfree(j: &mut Job, bv: u128) -> i64 {
    with(j, |s, j| {
        free_block(s, j, bv);
        0
    })
}

/// `void ber_bvecfree(struct berval **bv)`.
pub fn bvecfree(j: &mut Job, bv: u128) -> i64 {
    with(j, |s, j| {
        free_bervals(s, j, bv);
        0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_to_args() {
        // An int, then a pointer aligned to the next even slot.
        let w = [5, 0x99, 0x11, 0x22];
        let a = args_from("is", true, &w, 2);
        assert_eq!(a, [Arg::Int(5), Arg::Ptr(0x22_0000_0000_0000_0000 | 0x11)]);
        let a = args_from("ii", true, &w, 3);
        assert_eq!(a, [Arg::Int(5), Arg::Int(0x99)]);
    }

    #[test]
    fn writing_nests() {
        let mut w = Writing::default();
        w.open.push((0x30, Vec::new()));
        w.put(0x02, &int_bytes(5));
        let (t, b) = w.open.pop().unwrap();
        w.out.extend_from_slice(&tlv(t, &b));
        assert_eq!(w.flatten(), [0x30, 0x03, 0x02, 0x01, 0x05]);
        assert_eq!(int_of(&[0xFF]), Some(-1));
        assert_eq!(int_bytes(128), [0x00, 0x80]);
    }
}
