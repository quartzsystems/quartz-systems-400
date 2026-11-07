// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A zlib stream of the one kind the 5250 screen's PNG uses: greedy
//! matching on a four-byte hash (good 4, insert 6, nice 32, chain 32),
//! a 32 KiB window, blocks of up to 32767 symbols with Huffman trees
//! built the classic heap way. The parse and the trees follow zlib-ng's
//! level-2 compressor step for step, so a screen encodes to the same
//! bytes every time, the golden images included.

// Bit, byte and pixel arithmetic on values the formats bound.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]

/// Window size.
const WSIZE: usize = 1 << 15;
/// Bytes the matcher keeps ahead.
const MIN_LOOKAHEAD: usize = 258 + 3 + 1;
/// Farthest match.
const MAX_DIST: usize = WSIZE - MIN_LOOKAHEAD;
/// Window buffer.
const WINDOW: usize = 2 * WSIZE;
/// Shortest match taken.
const MIN_MATCH: usize = 4;
/// Matches no longer than this have each position hashed.
const MAX_INSERT: usize = 6;
/// A match this long ends the search.
const NICE: usize = 32;
/// Candidates looked at.
const CHAIN: u32 = 32;
/// Hash table size.
const HASH: usize = 1 << 16;
/// Symbols a block holds (`lit_bufsize - 1`, memory level 9).
const BLOCK_SYMBOLS: usize = (1 << 15) - 1;

const L_CODES: usize = 286;
const D_CODES: usize = 30;
const BL_CODES: usize = 19;
const HEAP_SIZE: usize = 2 * L_CODES + 1;
const END_BLOCK: usize = 256;
const BL_ORDER: [usize; BL_CODES] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
const EXTRA_LBITS: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const BASE_LENGTH: [u16; 29] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 12, 14, 16, 20, 24, 28, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 255];
const EXTRA_DBITS: [u8; D_CODES] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
const BASE_DIST: [u16; D_CODES] = [
    0, 1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048, 3072, 4096, 6144, 8192, 12288,
    16384, 24576,
];
const EXTRA_BLBITS: [u8; BL_CODES] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 3, 7];

/// `data` as a zlib stream.
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = BitWriter::default();
    // CMF 32K window, FLG level "fast".
    out.bytes.extend_from_slice(&[0x78, 0x5e]);
    let mut d = Deflater::new(data);
    d.run(&mut out);
    out.align();
    out.bytes.extend_from_slice(&adler32(data).to_be_bytes());
    out.bytes
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += u32::from(x);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

/// Bits, least significant first.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    fn put(&mut self, v: u32, len: u32) {
        self.acc |= u64::from(v) << self.n;
        self.n += len;
        while self.n >= 8 {
            self.bytes.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    fn align(&mut self) {
        if self.n > 0 {
            self.bytes.push(self.acc as u8);
        }
        self.acc = 0;
        self.n = 0;
    }
}

/// One symbol: a literal (`dist` 0) or a match.
#[derive(Clone, Copy)]
struct Sym {
    dist: u16,
    lc: u8,
}

/// The matcher's state. Positions are absolute; `base` is where the
/// window starts after its slides, so a hash entry at or before it is
/// as gone as a zeroed one.
struct Deflater<'a> {
    data: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
    base: usize,
    loaded: usize,
    strstart: usize,
    block_start: usize,
    syms: Vec<Sym>,
    lfreq: [u32; HEAP_SIZE],
    dfreq: [u32; 2 * D_CODES + 1],
}

fn hash(v: u32) -> usize {
    (v.wrapping_mul(2_654_435_761) >> 16) as usize & (HASH - 1)
}

impl<'a> Deflater<'a> {
    fn new(data: &'a [u8]) -> Self {
        let mut lfreq = [0; HEAP_SIZE];
        lfreq[END_BLOCK] = 1;
        Self {
            data,
            head: vec![0; HASH],
            prev: vec![0; data.len() + 8],
            base: 0,
            loaded: 0,
            strstart: 0,
            block_start: 0,
            syms: Vec::with_capacity(BLOCK_SYMBOLS),
            lfreq,
            dfreq: [0; 2 * D_CODES + 1],
        }
    }

    /// The window's view of byte `i`: data, and zero past its end.
    fn byte(&self, i: usize) -> u8 {
        self.data.get(i).copied().unwrap_or(0)
    }

    fn word(&self, i: usize) -> u32 {
        u32::from_le_bytes([self.byte(i), self.byte(i + 1), self.byte(i + 2), self.byte(i + 3)])
    }

    fn lookahead(&self) -> usize {
        self.loaded - self.strstart
    }

    /// Hash position `p`; the old head of its chain.
    fn insert(&mut self, p: usize) -> u32 {
        let h = hash(self.word(p));
        let old = self.head[h];
        if old as usize != p {
            self.prev[p] = old;
            self.head[h] = p as u32;
        }
        old
    }

    /// Whether a hash entry still points into the window.
    fn live(&self, pos: u32) -> bool {
        pos as usize > self.base
    }

    /// Slide when the window is full and load what fits.
    fn fill(&mut self) {
        if self.strstart - self.base >= WSIZE + MAX_DIST {
            self.base += WSIZE;
        }
        if self.loaded == self.data.len() {
            return;
        }
        self.loaded = (self.base + WINDOW).min(self.data.len());
        if self.strstart >= 1 {
            self.insert(self.strstart - 1);
        }
    }

    /// The longest match at `strstart` along the chain from `cur`:
    /// candidates that cannot beat the best so far are passed over (the
    /// first eight bytes, and from a best of 8 the eight ending at
    /// `best`), a candidate that passes but is no longer ends the search.
    fn longest_match(&self, mut cur: u32) -> (usize, usize) {
        let s = self.strstart;
        let lookahead = self.lookahead();
        let limit = self.base.max(s.saturating_sub(MAX_DIST)) as u32;
        let mut best = MIN_MATCH - 2;
        let mut start = 0usize;
        let mut chain = CHAIN;
        loop {
            let c = cur as usize;
            if c >= s {
                break;
            }
            let len = self.common(c, s);
            let passes = if best < 8 {
                len > best
            } else {
                let off = best - 7;
                (0..8).all(|i| self.byte(c + i) == self.byte(s + i) && self.byte(c + off + i) == self.byte(s + off + i))
            };
            if passes {
                if len <= best {
                    break;
                }
                start = c;
                if len >= lookahead {
                    return (lookahead, start);
                }
                best = len;
                if best >= NICE {
                    return (best, start);
                }
            }
            chain -= 1;
            if chain == 0 {
                return (best, start);
            }
            cur = self.prev[c];
            if cur <= limit {
                return (best, start);
            }
        }
        (best, start)
    }

    /// Bytes in common at `a` and `b`, at most 258.
    fn common(&self, a: usize, b: usize) -> usize {
        let mut n = 0;
        while n < 258 && self.byte(a + n) == self.byte(b + n) {
            n += 1;
        }
        n
    }

    fn tally(&mut self, dist: usize, lc: usize) -> bool {
        self.syms.push(Sym { dist: dist as u16, lc: lc as u8 });
        if dist == 0 {
            self.lfreq[lc] += 1;
        } else {
            self.lfreq[length_code(lc) + 257] += 1;
            self.dfreq[dist_code(dist - 1)] += 1;
        }
        self.syms.len() == BLOCK_SYMBOLS
    }

    fn run(&mut self, out: &mut BitWriter) {
        loop {
            if self.lookahead() < MIN_LOOKAHEAD {
                self.fill();
                if self.lookahead() == 0 {
                    break;
                }
            }
            let s = self.strstart;
            let full = if self.lookahead() >= MIN_MATCH {
                let head = self.insert(s);
                let dist = s.wrapping_sub(head as usize);
                let mut taken = None;
                if self.live(head) && dist > 0 && dist <= MAX_DIST {
                    let (len, at) = self.longest_match(head);
                    if len >= MIN_MATCH {
                        taken = Some((len, at));
                    }
                }
                if let Some((len, at)) = taken {
                    let full = self.tally(s - at, len - 3);
                    let ahead = self.lookahead() - len;
                    if len <= MAX_INSERT && ahead >= MIN_MATCH {
                        for p in s + 1..s + len {
                            self.insert(p);
                        }
                        self.strstart = s + len;
                    } else {
                        self.strstart = s + len;
                        self.insert(self.strstart - 1);
                    }
                    if full {
                        self.flush(out, false);
                    }
                    continue;
                }
                self.tally(0, usize::from(self.byte(s)))
            } else {
                self.tally(0, usize::from(self.byte(s)))
            };
            self.strstart += 1;
            if full {
                self.flush(out, false);
            }
        }
        self.flush(out, true);
    }

    /// Emit the symbols so far as one block.
    fn flush(&mut self, out: &mut BitWriter, last: bool) {
        let stored_len = self.strstart - self.block_start;
        let block = Block::build(&self.syms, &self.lfreq, &self.dfreq);
        let (opt_lenb, static_lenb) = if self.syms.is_empty() {
            (0, 0)
        } else {
            let opt = block.opt_len.wrapping_add(3 + 7) >> 3;
            let stat = block.static_len.wrapping_add(3 + 7) >> 3;
            (opt.min(stat), stat)
        };
        let stored_ok = self.block_start >= self.base;
        if stored_len + 4 <= opt_lenb && stored_ok {
            out.put(u32::from(last), 3);
            out.align();
            let n = stored_len as u16;
            out.bytes.extend_from_slice(&n.to_le_bytes());
            out.bytes.extend_from_slice(&(!n).to_le_bytes());
            out.bytes.extend_from_slice(&self.data[self.block_start..self.strstart]);
        } else if static_lenb == opt_lenb {
            out.put(2 | u32::from(last), 3);
            let (lt, dt) = static_trees();
            compress(out, &self.syms, &lt, &dt);
        } else {
            out.put(4 | u32::from(last), 3);
            block.send_trees(out);
            compress(out, &self.syms, &block.ltree, &block.dtree);
        }
        self.syms.clear();
        self.lfreq = [0; HEAP_SIZE];
        self.lfreq[END_BLOCK] = 1;
        self.dfreq = [0; 2 * D_CODES + 1];
        self.block_start = self.strstart;
    }
}

fn length_code(lc: usize) -> usize {
    (0..29).rev().find(|&c| usize::from(BASE_LENGTH[c]) <= lc).unwrap_or(0)
}

fn dist_code(d: usize) -> usize {
    (0..D_CODES).rev().find(|&c| usize::from(BASE_DIST[c]) <= d).unwrap_or(0)
}

/// A code: its bits (reversed for sending) and length.
#[derive(Clone, Copy, Default)]
struct Code {
    code: u16,
    len: u16,
}

fn static_trees() -> (Vec<Code>, Vec<Code>) {
    let mut lens = vec![0u16; 288];
    for (i, l) in lens.iter_mut().enumerate() {
        *l = match i {
            0..=143 | 280..=287 => 8,
            144..=255 => 9,
            _ => 7,
        };
    }
    let lt = codes_of(&lens);
    let dt = codes_of(&[5; D_CODES]);
    (lt, dt)
}

/// Canonical codes for `lens`, bit-reversed.
fn codes_of(lens: &[u16]) -> Vec<Code> {
    let mut count = [0u16; 16];
    for &l in lens {
        count[usize::from(l)] += 1;
    }
    count[0] = 0;
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for bits in 1..16 {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    lens.iter()
        .map(|&len| {
            if len == 0 {
                return Code::default();
            }
            let c = next[usize::from(len)];
            next[usize::from(len)] += 1;
            Code {
                code: c.reverse_bits() >> (16 - len),
                len,
            }
        })
        .collect()
}

fn compress(out: &mut BitWriter, syms: &[Sym], lt: &[Code], dt: &[Code]) {
    let send = |out: &mut BitWriter, c: Code| out.put(u32::from(c.code), u32::from(c.len));
    for s in syms {
        if s.dist == 0 {
            send(out, lt[usize::from(s.lc)]);
            continue;
        }
        let lc = usize::from(s.lc);
        let code = length_code(lc);
        send(out, lt[code + 257]);
        let extra = u32::from(EXTRA_LBITS[code]);
        if extra > 0 {
            out.put((lc - usize::from(BASE_LENGTH[code])) as u32, extra);
        }
        let d = usize::from(s.dist) - 1;
        let code = dist_code(d);
        send(out, dt[code]);
        let extra = u32::from(EXTRA_DBITS[code]);
        if extra > 0 {
            out.put((d - usize::from(BASE_DIST[code])) as u32, extra);
        }
    }
    send(out, lt[END_BLOCK]);
}

/// A tree being built: frequencies, then lengths and codes.
struct Tree {
    freq: Vec<u32>,
    len: Vec<u16>,
    dad: Vec<u16>,
    max_code: usize,
}

/// One block's dynamic trees and its sizes in bits.
struct Block {
    ltree: Vec<Code>,
    dtree: Vec<Code>,
    bltree: Vec<Code>,
    llens: Vec<u16>,
    dlens: Vec<u16>,
    lmax: usize,
    dmax: usize,
    max_blindex: usize,
    opt_len: usize,
    static_len: usize,
}

/// The static tree's lengths for `static_len`.
fn static_len_of(n: usize, kind: Kind) -> usize {
    match kind {
        Kind::Lit => match n {
            0..=143 | 280..=287 => 8,
            144..=255 => 9,
            _ => 7,
        },
        Kind::Dist => 5,
        Kind::Bl => 0,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Lit,
    Dist,
    Bl,
}

impl Kind {
    fn elems(self) -> usize {
        match self {
            Kind::Lit => L_CODES,
            Kind::Dist => D_CODES,
            Kind::Bl => BL_CODES,
        }
    }

    fn max_length(self) -> u16 {
        if self == Kind::Bl { 7 } else { 15 }
    }

    fn extra(self, n: usize) -> usize {
        match self {
            Kind::Lit if n >= 257 => usize::from(EXTRA_LBITS[n - 257]),
            Kind::Dist => usize::from(EXTRA_DBITS[n]),
            Kind::Bl => usize::from(EXTRA_BLBITS[n]),
            Kind::Lit => 0,
        }
    }
}

/// The heap of `build_tree`.
struct Heap {
    nodes: [usize; HEAP_SIZE + 1],
    len: usize,
    max: usize,
    depth: [u8; HEAP_SIZE + 1],
}

impl Heap {
    fn key(&self, t: &Tree, n: usize) -> u64 {
        (u64::from(t.freq[n]) << 8) | u64::from(self.depth[n])
    }

    fn down(&mut self, t: &Tree, mut k: usize) {
        let v = self.nodes[k];
        let vk = self.key(t, v);
        let mut j = k << 1;
        while j <= self.len {
            let mut jk = self.key(t, self.nodes[j]);
            if j < self.len {
                let j1 = self.key(t, self.nodes[j + 1]);
                if j1 <= jk {
                    j += 1;
                    jk = j1;
                }
            }
            if vk <= jk {
                break;
            }
            self.nodes[k] = self.nodes[j];
            k = j;
            j <<= 1;
        }
        self.nodes[k] = v;
    }

    fn pop(&mut self, t: &Tree) -> usize {
        let top = self.nodes[1];
        self.nodes[1] = self.nodes[self.len];
        self.len -= 1;
        self.down(t, 1);
        top
    }
}

/// Build the tree of `t` (frequencies set): lengths limited to the
/// kind's maximum, `opt_len` and `static_len` grown by its symbols.
fn build_tree(t: &mut Tree, kind: Kind, opt_len: &mut usize, static_len: &mut usize) {
    let elems = kind.elems();
    let mut h = Heap {
        nodes: [0; HEAP_SIZE + 1],
        len: 0,
        max: HEAP_SIZE,
        depth: [0; HEAP_SIZE + 1],
    };
    let mut max_code: isize = -1;
    for n in 0..elems {
        if t.freq[n] > 0 {
            h.len += 1;
            h.nodes[h.len] = n;
            max_code = n as isize;
            h.depth[n] = 0;
        } else {
            t.len[n] = 0;
        }
    }
    while h.len < 2 {
        let node = if max_code < 2 {
            max_code += 1;
            max_code as usize
        } else {
            0
        };
        h.len += 1;
        h.nodes[h.len] = node;
        t.freq[node] = 1;
        h.depth[node] = 0;
        *opt_len = opt_len.wrapping_sub(1);
        if kind != Kind::Bl {
            *static_len = static_len.wrapping_sub(static_len_of(node, kind));
        }
    }
    t.max_code = max_code as usize;
    for n in (1..=h.len / 2).rev() {
        h.down(t, n);
    }
    let mut node = elems;
    loop {
        let n = h.pop(t);
        let m = h.nodes[1];
        h.max -= 1;
        h.nodes[h.max] = n;
        h.max -= 1;
        h.nodes[h.max] = m;
        t.freq[node] = t.freq[n] + t.freq[m];
        h.depth[node] = h.depth[n].max(h.depth[m]) + 1;
        t.dad[n] = node as u16;
        t.dad[m] = node as u16;
        h.nodes[1] = node;
        node += 1;
        h.down(t, 1);
        if h.len < 2 {
            break;
        }
    }
    h.max -= 1;
    h.nodes[h.max] = h.nodes[1];
    gen_bitlen(t, &h, kind, opt_len, static_len);
}

fn gen_bitlen(t: &mut Tree, h: &Heap, kind: Kind, opt_len: &mut usize, static_len: &mut usize) {
    let max_length = kind.max_length();
    let mut bl_count = [0u16; 16];
    let root = h.nodes[h.max];
    t.len[root] = 0;
    let mut overflow = 0i32;
    for &n in &h.nodes[h.max + 1..HEAP_SIZE] {
        let mut bits = t.len[usize::from(t.dad[n])] + 1;
        if bits > max_length {
            bits = max_length;
            overflow += 1;
        }
        t.len[n] = bits;
        if n > t.max_code {
            continue;
        }
        bl_count[usize::from(bits)] += 1;
        let x = kind.extra(n);
        let f = t.freq[n] as usize;
        *opt_len = opt_len.wrapping_add(f * (usize::from(bits) + x));
        if kind != Kind::Bl {
            *static_len = static_len.wrapping_add(f * (static_len_of(n, kind) + x));
        }
    }
    if overflow > 0 {
        loop {
            let mut bits = usize::from(max_length) - 1;
            while bl_count[bits] == 0 {
                bits -= 1;
            }
            bl_count[bits] -= 1;
            bl_count[bits + 1] += 2;
            bl_count[usize::from(max_length)] -= 1;
            overflow -= 2;
            if overflow <= 0 {
                break;
            }
        }
        let mut i = HEAP_SIZE;
        for bits in (1..=max_length).rev() {
            let mut n = bl_count[usize::from(bits)];
            while n != 0 {
                i -= 1;
                let m = h.nodes[i];
                if m > t.max_code {
                    continue;
                }
                if t.len[m] != bits {
                    *opt_len = opt_len.wrapping_add(usize::from(bits) * t.freq[m] as usize);
                    *opt_len = opt_len.wrapping_sub(usize::from(t.len[m]) * t.freq[m] as usize);
                    t.len[m] = bits;
                }
                n -= 1;
            }
        }
    }
}

const REP_3_6: usize = 16;
const REPZ_3_10: usize = 17;
const REPZ_11_138: usize = 18;

/// The runs `send_tree` makes of `lens[..=max_code]`: (symbol, extra
/// bits value, extra length).
fn runs(lens: &[u16], max_code: usize) -> Vec<(usize, u32, u32)> {
    let mut out = Vec::new();
    let mut prevlen: i32 = -1;
    let mut nextlen = lens[0];
    let (mut max_count, mut min_count) = if nextlen == 0 { (138, 3) } else { (7, 4) };
    let mut count = 0;
    for n in 0..=max_code {
        let curlen = nextlen;
        nextlen = if n < max_code { lens[n + 1] } else { u16::MAX };
        count += 1;
        if count < max_count && curlen == nextlen {
            continue;
        }
        if count < min_count {
            for _ in 0..count {
                out.push((usize::from(curlen), 0, 0));
            }
        } else if curlen != 0 {
            if i32::from(curlen) != prevlen {
                out.push((usize::from(curlen), 0, 0));
                count -= 1;
            }
            out.push((REP_3_6, count - 3, 2));
        } else if count <= 10 {
            out.push((REPZ_3_10, count - 3, 3));
        } else {
            out.push((REPZ_11_138, count - 11, 7));
        }
        count = 0;
        prevlen = i32::from(curlen);
        (max_count, min_count) = if nextlen == 0 {
            (138, 3)
        } else if curlen == nextlen {
            (6, 3)
        } else {
            (7, 4)
        };
    }
    out
}

impl Block {
    fn build(_syms: &[Sym], lfreq: &[u32; HEAP_SIZE], dfreq: &[u32; 2 * D_CODES + 1]) -> Block {
        let (mut opt_len, mut static_len) = (0usize, 0usize);
        let mut lt = Tree {
            freq: lfreq.to_vec(),
            len: vec![0; HEAP_SIZE],
            dad: vec![0; HEAP_SIZE],
            max_code: 0,
        };
        build_tree(&mut lt, Kind::Lit, &mut opt_len, &mut static_len);
        let mut dt = Tree {
            freq: {
                let mut f = dfreq.to_vec();
                f.resize(HEAP_SIZE, 0);
                f
            },
            len: vec![0; HEAP_SIZE],
            dad: vec![0; HEAP_SIZE],
            max_code: 0,
        };
        build_tree(&mut dt, Kind::Dist, &mut opt_len, &mut static_len);
        // The bit length tree.
        let mut blt = Tree {
            freq: vec![0; HEAP_SIZE],
            len: vec![0; HEAP_SIZE],
            dad: vec![0; HEAP_SIZE],
            max_code: 0,
        };
        for (lens, max) in [(&lt.len, lt.max_code), (&dt.len, dt.max_code)] {
            for (sym, _, _) in runs(lens, max) {
                blt.freq[sym] += 1;
            }
        }
        build_tree(&mut blt, Kind::Bl, &mut opt_len, &mut static_len);
        let mut max_blindex = BL_CODES - 1;
        while max_blindex >= 3 && blt.len[BL_ORDER[max_blindex]] == 0 {
            max_blindex -= 1;
        }
        opt_len = opt_len.wrapping_add(3 * (max_blindex + 1) + 5 + 5 + 4);
        let codes = |t: &Tree, n: usize| codes_of(&t.len[..n]);
        Block {
            ltree: codes(&lt, L_CODES),
            dtree: codes(&dt, D_CODES),
            bltree: codes(&blt, BL_CODES),
            llens: lt.len[..L_CODES].to_vec(),
            dlens: dt.len[..D_CODES].to_vec(),
            lmax: lt.max_code,
            dmax: dt.max_code,
            max_blindex,
            opt_len,
            static_len,
        }
    }

    fn send_trees(&self, out: &mut BitWriter) {
        out.put((self.lmax + 1 - 257) as u32, 5);
        out.put(self.dmax as u32, 5);
        out.put((self.max_blindex + 1 - 4) as u32, 4);
        for &i in &BL_ORDER[..=self.max_blindex] {
            out.put(u32::from(self.bltree[i].len), 3);
        }
        for (lens, max) in [(&self.llens, self.lmax), (&self.dlens, self.dmax)] {
            for (sym, v, n) in runs(lens, max) {
                let c = self.bltree[sym];
                out.put(u32::from(c.code), u32::from(c.len));
                if n > 0 {
                    out.put(v, n);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain inflater, enough to check round trips.
    #[allow(clippy::too_many_lines, clippy::comparison_chain)]
    fn inflate(z: &[u8]) -> Vec<u8> {
        struct R<'a> {
            b: &'a [u8],
            p: usize,
        }
        impl R<'_> {
            fn bit(&mut self) -> u32 {
                let v = (self.b[self.p >> 3] >> (self.p & 7)) & 1;
                self.p += 1;
                u32::from(v)
            }
            fn bits(&mut self, n: u32) -> u32 {
                (0..n).fold(0, |v, i| v | (self.bit() << i))
            }
            fn sym(&mut self, t: &[(u16, u16, usize)]) -> usize {
                let (mut code, mut len) = (0u16, 0u16);
                loop {
                    code = (code << 1) | self.bit() as u16;
                    len += 1;
                    if let Some(e) = t.iter().find(|e| e.0 == len && e.1 == code) {
                        return e.2;
                    }
                }
            }
        }
        fn table(lens: &[u16]) -> Vec<(u16, u16, usize)> {
            let mut count = [0u16; 16];
            for &l in lens {
                count[usize::from(l)] += 1;
            }
            count[0] = 0;
            let mut next = [0u16; 16];
            let mut code = 0;
            for b in 1..16 {
                code = (code + count[b - 1]) << 1;
                next[b] = code;
            }
            let mut t = Vec::new();
            for (s, &l) in lens.iter().enumerate() {
                if l > 0 {
                    t.push((l, next[usize::from(l)], s));
                    next[usize::from(l)] += 1;
                }
            }
            t
        }
        let mut r = R { b: &z[2..], p: 0 };
        let mut out: Vec<u8> = Vec::new();
        loop {
            let last = r.bit();
            let kind = r.bits(2);
            let (lt, dt) = if kind == 0 {
                r.p = r.p.div_ceil(8) * 8;
                let n = r.bits(16) as usize;
                r.bits(16);
                for _ in 0..n {
                    out.push(r.bits(8) as u8);
                }
                if last == 1 {
                    break;
                }
                continue;
            } else if kind == 1 {
                let (l, d) = static_trees();
                (table(&l.iter().map(|c| c.len).collect::<Vec<_>>()), table(&d.iter().map(|c| c.len).collect::<Vec<_>>()))
            } else {
                let hlit = r.bits(5) as usize + 257;
                let hdist = r.bits(5) as usize + 1;
                let hclen = r.bits(4) as usize + 4;
                let mut cl = [0u16; 19];
                for &i in &BL_ORDER[..hclen] {
                    cl[i] = r.bits(3) as u16;
                }
                let ct = table(&cl);
                let mut lens: Vec<u16> = Vec::new();
                while lens.len() < hlit + hdist {
                    match r.sym(&ct) {
                        s @ 0..=15 => lens.push(s as u16),
                        16 => {
                            let l = *lens.last().unwrap();
                            for _ in 0..r.bits(2) + 3 {
                                lens.push(l);
                            }
                        }
                        17 => lens.extend(std::iter::repeat_n(0, r.bits(3) as usize + 3)),
                        _ => lens.extend(std::iter::repeat_n(0, r.bits(7) as usize + 11)),
                    }
                }
                (table(&lens[..hlit]), table(&lens[hlit..]))
            };
            loop {
                let s = r.sym(&lt);
                if s < 256 {
                    out.push(s as u8);
                } else if s == 256 {
                    break;
                } else {
                    let c = s - 257;
                    let len = usize::from(BASE_LENGTH[c]) + 3 + r.bits(u32::from(EXTRA_LBITS[c])) as usize;
                    let d = r.sym(&dt);
                    let dist = usize::from(BASE_DIST[d]) + 1 + r.bits(u32::from(EXTRA_DBITS[d])) as usize;
                    for _ in 0..len {
                        out.push(out[out.len() - dist]);
                    }
                }
            }
            if last == 1 {
                break;
            }
        }
        out
    }

    #[test]
    fn round_trips() {
        let mut big = Vec::new();
        let mut x = 7u32;
        for i in 0..300_000u32 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            big.push(if i % 7 < 4 { (x >> 24) as u8 } else { (i / 1000) as u8 });
        }
        for data in [Vec::new(), b"a".to_vec(), b"abcabcabcabcabcabc".to_vec(), vec![0; 100_000], big] {
            let z = zlib(&data);
            assert_eq!(&z[..2], &[0x78, 0x5e]);
            assert_eq!(inflate(&z), data);
            assert_eq!(u32::from_be_bytes(z[z.len() - 4..].try_into().unwrap()), adler32(&data));
        }
    }
}
