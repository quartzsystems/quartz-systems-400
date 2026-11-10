//! Custom-marshaled INFO structures (MS-RPRN 2.2.2): fixed portions one
//! after another, then the variable data each one's offsets point to,
//! relative to the start of its own fixed portion. Strings are UTF-16LE
//! with their NUL, aligned to 2; other data to 4.

/// One fixed portion and the variable data it points to.
pub(crate) struct Block {
    fixed: Vec<u8>,
    vars: Vec<Var>,
}

struct Var {
    /// Where the offset goes in the fixed portion.
    field: usize,
    align: usize,
    data: Vec<u8>,
}

impl Block {
    /// A fixed portion of `size` zero bytes.
    pub(crate) fn new(size: usize) -> Self {
        Self {
            fixed: vec![0; size],
            vars: Vec::new(),
        }
    }

    pub(crate) fn u32(&mut self, at: usize, value: u32) -> &mut Self {
        self.fixed[at..at + 4].copy_from_slice(&value.to_le_bytes());
        self
    }

    pub(crate) fn u16(&mut self, at: usize, value: u16) -> &mut Self {
        self.fixed[at..at + 2].copy_from_slice(&value.to_le_bytes());
        self
    }

    pub(crate) fn u64(&mut self, at: usize, value: u64) -> &mut Self {
        self.fixed[at..at + 8].copy_from_slice(&value.to_le_bytes());
        self
    }

    pub(crate) fn bytes(&mut self, at: usize, value: &[u8]) -> &mut Self {
        self.fixed[at..at + value.len()].copy_from_slice(value);
        self
    }

    /// The offset at `at` points to `text` and its NUL.
    pub(crate) fn string(&mut self, at: usize, text: &str) -> &mut Self {
        self.var(at, 2, utf16z(text))
    }

    /// The offset at `at` points to `text` as 8-bit characters and a NUL.
    pub(crate) fn ansi(&mut self, at: usize, text: &str) -> &mut Self {
        let mut data: Vec<u8> = text.bytes().collect();
        data.push(0);
        self.var(at, 1, data)
    }

    /// The offset at `at` points to a multi-string: each of `items` with
    /// its NUL, then a NUL. No items leaves the offset 0 (NULL).
    pub(crate) fn multi_sz(&mut self, at: usize, items: &[String]) -> &mut Self {
        if items.is_empty() {
            return self;
        }
        let mut data = Vec::new();
        for item in items {
            data.extend(utf16z(item));
        }
        data.extend_from_slice(&[0, 0]);
        self.var(at, 2, data)
    }

    /// The offset at `at` points to `data`, aligned to 4.
    pub(crate) fn blob(&mut self, at: usize, data: Vec<u8>) -> &mut Self {
        self.var(at, 4, data)
    }

    fn var(&mut self, field: usize, align: usize, data: Vec<u8>) -> &mut Self {
        self.vars.push(Var { field, align, data });
        self
    }
}

/// `text` as UTF-16LE with its NUL.
pub(crate) fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// The blocks laid out: fixed portions first, variable data after.
pub(crate) fn pack(blocks: Vec<Block>) -> Vec<u8> {
    let fixed: usize = blocks.iter().map(|b| b.fixed.len()).sum();
    let mut out = vec![0; fixed];
    let mut start = 0;
    for block in blocks {
        let size = block.fixed.len();
        out[start..start + size].copy_from_slice(&block.fixed);
        for var in block.vars {
            out.resize(out.len().next_multiple_of(var.align), 0);
            let offset = u32::try_from(out.len() - start).expect("a small buffer");
            out[start + var.field..start + var.field + 4].copy_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&var.data);
        }
        start += size;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(buf: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn offsets_count_from_each_fixed_portion() {
        let mut a = Block::new(8);
        a.u32(0, 7).string(4, "ab");
        let mut b = Block::new(8);
        b.u32(0, 9).blob(4, vec![1, 2, 3, 4]);
        let out = pack(vec![a, b]);
        // a's string at 16, b's blob aligned to 24, offset from 8.
        assert_eq!(u32_at(&out, 4), 16);
        assert_eq!(&out[16..22], &[b'a', 0, b'b', 0, 0, 0]);
        assert_eq!(u32_at(&out, 12), 16);
        assert_eq!(&out[24..28], &[1, 2, 3, 4]);
        assert_eq!(out.len(), 28);
    }

    #[test]
    fn multi_strings_end_with_two_nuls_and_none_is_null() {
        let mut a = Block::new(8);
        a.multi_sz(0, &["x".into(), "y".into()]).multi_sz(4, &[]);
        let out = pack(vec![a]);
        assert_eq!(u32_at(&out, 0), 8);
        assert_eq!(u32_at(&out, 4), 0);
        assert_eq!(&out[8..], &[b'x', 0, 0, 0, b'y', 0, 0, 0, 0, 0]);
    }

    #[test]
    fn ansi_strings_are_bytes() {
        let mut a = Block::new(4);
        a.ansi(0, "A4");
        assert_eq!(&pack(vec![a])[4..], b"A4\0");
    }
}
