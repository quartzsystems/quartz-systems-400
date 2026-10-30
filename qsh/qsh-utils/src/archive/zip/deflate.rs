// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Deflating (RFC 1951): greedy LZ77 over a 32 KB window with hash
//! chains, then each run of tokens as whichever of a dynamic, fixed or
//! stored block is smallest.

use super::inflate::{CL_ORDER, DIST_BASE, DIST_EXTRA, LEN_BASE, LEN_EXTRA};

const WINDOW: usize = 32 * 1024;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
const MAX_CHAIN: usize = 128;
const HASH_BITS: u32 = 15;
const BLOCK_TOKENS: usize = 16 * 1024;

#[derive(Clone, Copy)]
enum Tok {
    Lit(u8),
    Match(u16, u16),
}

fn hash(d: &[u8], i: usize) -> usize {
    let v = u32::from(d[i]) << 16 | u32::from(d[i + 1]) << 8 | u32::from(d[i + 2]);
    (v.wrapping_mul(2_654_435_761) >> (32 - HASH_BITS)) as usize
}

/// Tokens and, for each, the input offset it starts at.
fn tokens(d: &[u8]) -> Vec<(Tok, usize)> {
    let mut head = vec![usize::MAX; 1 << HASH_BITS];
    let mut prev = vec![usize::MAX; d.len()];
    let mut out = Vec::new();
    let mut i = 0;
    let insert = |head: &mut Vec<usize>, prev: &mut Vec<usize>, i: usize| {
        if i + MIN_MATCH <= d.len() {
            let h = hash(d, i);
            prev[i] = head[h];
            head[h] = i;
        }
    };
    while i < d.len() {
        let mut best = (0usize, 0usize);
        if i + MIN_MATCH <= d.len() {
            let mut cand = head[hash(d, i)];
            let mut chain = 0;
            let limit = (d.len() - i).min(MAX_MATCH);
            while cand != usize::MAX && i - cand <= WINDOW && chain < MAX_CHAIN {
                let mut l = 0;
                while l < limit && d[cand + l] == d[i + l] {
                    l += 1;
                }
                if l > best.0 {
                    best = (l, i - cand);
                    if l == limit {
                        break;
                    }
                }
                cand = prev[cand];
                chain += 1;
            }
        }
        if best.0 >= MIN_MATCH {
            out.push((Tok::Match(best.0 as u16, best.1 as u16), i));
            for k in i..i + best.0 {
                insert(&mut head, &mut prev, k);
            }
            i += best.0;
        } else {
            out.push((Tok::Lit(d[i]), i));
            insert(&mut head, &mut prev, i);
            i += 1;
        }
    }
    out
}

struct Writer {
    out: Vec<u8>,
    buf: u64,
    cnt: u32,
}

impl Writer {
    fn put(&mut self, v: u32, n: u32) {
        self.buf |= u64::from(v) << self.cnt;
        self.cnt += n;
        while self.cnt >= 8 {
            self.out.push(self.buf as u8);
            self.buf >>= 8;
            self.cnt -= 8;
        }
    }

    fn align(&mut self) {
        if self.cnt > 0 {
            self.out.push(self.buf as u8);
        }
        self.buf = 0;
        self.cnt = 0;
    }
}

fn len_code(len: u16) -> (usize, u32, u32) {
    let i = LEN_BASE.iter().rposition(|&b| b <= len).expect("length");
    (257 + i, u32::from(LEN_EXTRA[i]), u32::from(len - LEN_BASE[i]))
}

fn dist_code(d: u16) -> (usize, u32, u32) {
    let i = DIST_BASE.iter().rposition(|&b| b <= d).expect("distance");
    (i, u32::from(DIST_EXTRA[i]), u32::from(d - DIST_BASE[i]))
}

/// Code lengths for frequencies, none longer than `limit`.
pub fn lengths(freq: &[u32], limit: u8) -> Vec<u8> {
    let mut f: Vec<u64> = freq.iter().map(|&x| u64::from(x)).collect();
    loop {
        let l = huffman(&f);
        if l.iter().all(|&x| x <= limit) {
            return l;
        }
        for x in &mut f {
            if *x > 0 {
                *x = (*x).div_ceil(2);
            }
        }
    }
}

/// A tree node: weight, children (none for a leaf) and the leaf's symbol.
type Node = (u64, Option<(usize, usize)>, usize);

fn huffman(f: &[u64]) -> Vec<u8> {
    let mut lens = vec![0u8; f.len()];
    // Nodes: (weight, children or leaf).
    let mut nodes: Vec<Node> = Vec::new();
    let mut live: Vec<usize> = Vec::new();
    for (s, &w) in f.iter().enumerate() {
        if w > 0 {
            nodes.push((w, None, s));
            live.push(nodes.len() - 1);
        }
    }
    if live.len() == 1 {
        lens[nodes[0].2] = 1;
        return lens;
    }
    while live.len() > 1 {
        live.sort_by(|a, b| nodes[*b].0.cmp(&nodes[*a].0).then(b.cmp(a)));
        let a = live.pop().expect("node");
        let b = live.pop().expect("node");
        nodes.push((nodes[a].0 + nodes[b].0, Some((a, b)), 0));
        live.push(nodes.len() - 1);
    }
    let mut stack = vec![(live[0], 0u8)];
    while let Some((n, d)) = stack.pop() {
        match nodes[n].1 {
            Some((a, b)) => {
                stack.push((a, d + 1));
                stack.push((b, d + 1));
            }
            None => lens[nodes[n].2] = d.max(1),
        }
    }
    lens
}

/// Canonical codes, bit-reversed for writing low bit first.
fn codes(lens: &[u8]) -> Vec<u32> {
    let mut count = [0u32; 16];
    for &l in lens {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; 16];
    let mut code = 0;
    for b in 1..16 {
        code = (code + count[b - 1]) << 1;
        next[b] = code;
    }
    lens.iter()
        .map(|&l| {
            if l == 0 {
                return 0;
            }
            let c = next[l as usize];
            next[l as usize] += 1;
            let mut r = 0;
            for i in 0..l {
                if c >> i & 1 != 0 {
                    r |= 1 << (l - 1 - i);
                }
            }
            r
        })
        .collect()
}

/// Run-length coded code lengths: (symbol, extra bits value).
fn rle(lens: &[u8]) -> Vec<(u8, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lens.len() {
        let v = lens[i];
        let mut run = 1;
        while i + run < lens.len() && lens[i + run] == v {
            run += 1;
        }
        let mut left = run;
        if v == 0 {
            while left >= 11 {
                let n = left.min(138);
                out.push((18, (n - 11) as u32));
                left -= n;
            }
            if left >= 3 {
                out.push((17, (left - 3) as u32));
                left = 0;
            }
        } else {
            out.push((v, 0));
            left -= 1;
            while left >= 3 {
                let n = left.min(6);
                out.push((16, (n - 3) as u32));
                left -= n;
            }
        }
        for _ in 0..left {
            out.push((v, 0));
        }
        i += run;
    }
    out
}

fn fixed_lens() -> (Vec<u8>, Vec<u8>) {
    let mut l = vec![8u8; 288];
    l[144..256].fill(9);
    l[256..280].fill(7);
    (l, vec![5u8; 30])
}

fn cost(toks: &[(Tok, usize)], ll: &[u8], dl: &[u8]) -> u64 {
    let mut bits = u64::from(ll[256]);
    for (t, _) in toks {
        match *t {
            Tok::Lit(b) => bits += u64::from(ll[b as usize]),
            Tok::Match(l, d) => {
                let (lc, le, _) = len_code(l);
                let (dc, de, _) = dist_code(d);
                bits += u64::from(ll[lc]) + u64::from(le) + u64::from(dl[dc]) + u64::from(de);
            }
        }
    }
    bits
}

fn write_tokens(w: &mut Writer, toks: &[(Tok, usize)], ll: &[u8], dl: &[u8]) {
    let lc = codes(ll);
    let dc = codes(dl);
    for (t, _) in toks {
        match *t {
            Tok::Lit(b) => w.put(lc[b as usize], u32::from(ll[b as usize])),
            Tok::Match(l, d) => {
                let (c, n, v) = len_code(l);
                w.put(lc[c], u32::from(ll[c]));
                w.put(v, n);
                let (c, n, v) = dist_code(d);
                w.put(dc[c], u32::from(dl[c]));
                w.put(v, n);
            }
        }
    }
    w.put(lc[256], u32::from(ll[256]));
}

fn block(w: &mut Writer, data: &[u8], toks: &[(Tok, usize)], span: (usize, usize), last: bool) {
    let mut lf = vec![0u32; 286];
    let mut df = vec![0u32; 30];
    lf[256] = 1;
    for (t, _) in toks {
        match *t {
            Tok::Lit(b) => lf[b as usize] += 1,
            Tok::Match(l, d) => {
                lf[len_code(l).0] += 1;
                df[dist_code(d).0] += 1;
            }
        }
    }
    // Two codes at least in each tree, for every inflater's sake.
    for f in [&mut lf, &mut df] {
        let mut used = f.iter().filter(|&&x| x > 0).count();
        let mut k = 0;
        while used < 2 {
            if f[k] == 0 {
                f[k] = 1;
                used += 1;
            }
            k += 1;
        }
    }
    let ll = lengths(&lf, 15);
    let dl = lengths(&df, 15);
    let nlen = ll.iter().rposition(|&x| x > 0).map_or(257, |p| (p + 1).max(257));
    let ndist = dl.iter().rposition(|&x| x > 0).map_or(1, |p| p + 1);
    let mut all = ll[..nlen].to_vec();
    all.extend_from_slice(&dl[..ndist]);
    let runs = rle(&all);
    let mut cf = vec![0u32; 19];
    for (s, _) in &runs {
        cf[*s as usize] += 1;
    }
    let cl = lengths(&cf, 7);
    let ncode = CL_ORDER.iter().rposition(|&i| cl[i] > 0).map_or(4, |p| (p + 1).max(4));
    let header: u64 = 14
        + 3 * ncode as u64
        + runs
            .iter()
            .map(|(s, _)| {
                u64::from(cl[*s as usize])
                    + match s {
                        16 => 2,
                        17 => 3,
                        18 => 7,
                        _ => 0,
                    }
            })
            .sum::<u64>();
    let dyn_bits = header + cost(toks, &ll, &dl);
    let (fl, fd) = fixed_lens();
    let fixed_bits = cost(toks, &fl, &fd);
    let raw = &data[span.0..span.1];
    let stored_bits = (raw.len().div_ceil(65535).max(1) * 5 * 8 + raw.len() * 8) as u64 + 8;
    let lastb = u32::from(last);
    if stored_bits <= dyn_bits.min(fixed_bits) {
        let chunks: Vec<&[u8]> = if raw.is_empty() {
            vec![raw]
        } else {
            raw.chunks(65535).collect()
        };
        let n = chunks.len();
        for (i, c) in chunks.into_iter().enumerate() {
            w.put(if i + 1 == n { lastb } else { 0 }, 1);
            w.put(0, 2);
            w.align();
            let len = c.len() as u16;
            w.out.extend_from_slice(&len.to_le_bytes());
            w.out.extend_from_slice(&(!len).to_le_bytes());
            w.out.extend_from_slice(c);
        }
    } else if fixed_bits <= dyn_bits {
        w.put(lastb, 1);
        w.put(1, 2);
        write_tokens(w, toks, &fl, &fd);
    } else {
        w.put(lastb, 1);
        w.put(2, 2);
        w.put((nlen - 257) as u32, 5);
        w.put((ndist - 1) as u32, 5);
        w.put((ncode - 4) as u32, 4);
        for &i in &CL_ORDER[..ncode] {
            w.put(u32::from(cl[i]), 3);
        }
        let cc = codes(&cl);
        for (s, extra) in runs {
            w.put(cc[s as usize], u32::from(cl[s as usize]));
            match s {
                16 => w.put(extra, 2),
                17 => w.put(extra, 3),
                18 => w.put(extra, 7),
                _ => {}
            }
        }
        write_tokens(w, toks, &ll, &dl);
    }
}

/// Deflate bytes to raw deflate data.
pub fn deflate(data: &[u8]) -> Vec<u8> {
    let toks = tokens(data);
    let mut w = Writer {
        out: Vec::new(),
        buf: 0,
        cnt: 0,
    };
    if toks.is_empty() {
        block(&mut w, data, &[], (0, 0), true);
    }
    let n = toks.len().div_ceil(BLOCK_TOKENS);
    for (bi, chunk) in toks.chunks(BLOCK_TOKENS).enumerate() {
        let start = chunk[0].1;
        let end = toks.get((bi + 1) * BLOCK_TOKENS).map_or(data.len(), |t| t.1);
        block(&mut w, data, chunk, (start, end), bi + 1 == n);
    }
    w.align();
    w.out
}

#[cfg(test)]
mod tests {
    use super::super::inflate::inflate;
    use super::*;

    #[test]
    fn round_trips() {
        let samples: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"a".to_vec(),
            b"abcabcabcabcabcabc".to_vec(),
            b"the quick brown fox jumps over the lazy dog ".repeat(200),
            (0..70_000u32).map(|i| (i * 7 % 251) as u8).collect(),
            vec![0u8; 100_000],
        ];
        for s in samples {
            let z = deflate(&s);
            assert_eq!(inflate(&z).unwrap(), s);
        }
    }

    #[test]
    fn limited_lengths() {
        let f: Vec<u32> = (0..40).map(|i| 1u32 << (i % 30)).collect();
        assert!(lengths(&f, 15).iter().all(|&l| l <= 15 && l > 0));
    }
}
