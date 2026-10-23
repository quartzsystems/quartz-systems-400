// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Fat pointers and the spaces they address. A pointer names a space by
//! number and generation; a deref checks the kind, that the space is
//! still live and that the address is inside it, so a pointer into a
//! returned frame or a freed allocation fails instead of reaching
//! whatever is there now.
//!
//! A static area, a heap block and a mapped user space are each one
//! space with exact bounds. An entry's automatic storage is one space
//! per call-stack depth, as the machine's stack is one space: it grows
//! over every automatic address the entry takes, so a pointer advanced
//! past one automatic variable reads its neighbour, and it dies when the
//! entry returns.

use codegen::Cond;

/// What a pointer points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PtrKind {
    /// Null.
    Null = 0,
    /// A space: storage with an offset.
    Space = 1,
    /// A system object.
    System = 2,
    /// A procedure.
    Procedure = 3,
}

impl PtrKind {
    fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            0 => Self::Null,
            1 => Self::Space,
            2 => Self::System,
            3 => Self::Procedure,
            _ => return None,
        })
    }
}

/// The 16 bytes of a pointer in storage: kind, flags, generation,
/// space number, offset; little-endian fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FatPtr {
    /// Kind byte.
    pub kind: u8,
    /// Flags; unused so far.
    pub flags: u8,
    /// Generation of the space when the pointer was made.
    pub generation: u16,
    /// Space number, or the code address's low half for procedures.
    pub space: u32,
    /// Byte offset from the space's base, wrapping below it in a frame
    /// that grew down, or a code address for procedures.
    pub offset: u64,
}

impl FatPtr {
    /// From the 16 bytes in storage.
    #[must_use]
    pub fn from_bytes(b: &[u8; 16]) -> Self {
        Self {
            kind: b[0],
            flags: b[1],
            generation: u16::from_le_bytes([b[2], b[3]]),
            space: u32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            offset: u64::from_le_bytes(b[8..16].try_into().expect("8 bytes")),
        }
    }

    /// From the two halves generated code passes.
    #[must_use]
    pub fn from_halves(lo: u64, hi: u64) -> Self {
        let mut b = [0u8; 16];
        b[..8].copy_from_slice(&lo.to_le_bytes());
        b[8..].copy_from_slice(&hi.to_le_bytes());
        Self::from_bytes(&b)
    }

    /// The 16 bytes in storage.
    #[must_use]
    pub fn to_bytes(self) -> [u8; 16] {
        let mut b = [0u8; 16];
        b[0] = self.kind;
        b[1] = self.flags;
        b[2..4].copy_from_slice(&self.generation.to_le_bytes());
        b[4..8].copy_from_slice(&self.space.to_le_bytes());
        b[8..16].copy_from_slice(&self.offset.to_le_bytes());
        b
    }

    /// The null pointer.
    #[must_use]
    pub fn null() -> Self {
        Self::default()
    }

    /// Whether it is null.
    #[must_use]
    pub fn is_null(self) -> bool {
        self.kind == PtrKind::Null as u8
    }

    /// A procedure pointer to native code.
    #[must_use]
    pub fn procedure(code: u64) -> Self {
        Self {
            kind: PtrKind::Procedure as u8,
            flags: 0,
            generation: 0,
            space: 0,
            offset: code,
        }
    }

    /// The kind, when valid.
    #[must_use]
    pub fn kind(self) -> Option<PtrKind> {
        PtrKind::from_byte(self.kind)
    }

    fn space(space: usize, generation: u16, offset: u64) -> Self {
        Self {
            kind: PtrKind::Space as u8,
            flags: 0,
            generation,
            space: u32::try_from(space).expect("few spaces"),
            offset,
        }
    }
}

/// A region of storage a pointer can address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Space {
    /// Native address pointer offsets count from.
    pub base: u64,
    /// First byte covered; below `base` only in a frame that grew down.
    pub lo: u64,
    /// One past the last byte covered.
    pub hi: u64,
    /// Generation; bumped when the space dies so stale pointers fail.
    pub generation: u16,
    /// Alive.
    pub live: bool,
    /// Call-stack depth that owns it; 0 for static storage.
    pub owner_depth: u32,
    /// An entry's automatic storage: one per depth, growing over every
    /// automatic address the entry takes.
    pub frame: bool,
}

impl Space {
    fn new(base: u64, len: u64, owner_depth: u32, frame: bool) -> Self {
        Self {
            base,
            lo: base,
            hi: base + len,
            generation: 1,
            live: true,
            owner_depth,
            frame,
        }
    }

    /// The native address a pointer into this space names.
    fn native(&self, p: FatPtr) -> u64 {
        self.base.wrapping_add(p.offset)
    }

    fn die(&mut self) {
        self.live = false;
        self.generation = self.generation.wrapping_add(1);
    }
}

/// The job's space table.
#[derive(Clone, Debug, Default)]
#[allow(clippy::struct_field_names)]
pub struct Spaces {
    spaces: Vec<Space>,
    statics: Vec<(u64, u64)>,
    /// Static areas of system-state programs: the system domain.
    system: Vec<(u64, u64)>,
}

impl Spaces {
    /// Record a static area, which pointers may address for ever.
    pub fn register_static(&mut self, base: u64, len: u64) {
        if !self.statics.contains(&(base, len)) {
            self.statics.push((base, len));
        }
    }

    /// Record a system-state program's static area as system domain.
    pub fn register_system(&mut self, base: u64, len: u64) {
        if !self.system.contains(&(base, len)) {
            self.system.push((base, len));
        }
    }

    /// Whether a native address lies in system-domain storage.
    #[must_use]
    pub fn in_system(&self, addr: u64) -> bool {
        self.system
            .iter()
            .any(|(b, l)| addr >= *b && addr < b.saturating_add(*l))
    }

    /// A space over storage the runtime owns (a heap block, a mapped
    /// user space): lives until [`Spaces::kill`].
    pub fn add_region(&mut self, base: u64, len: u64) -> FatPtr {
        self.spaces.push(Space::new(base, len, 0, false));
        FatPtr::space(self.spaces.len() - 1, 1, 0)
    }

    /// Kill the runtime-owned space at a base address, if any.
    pub fn kill_at(&mut self, base: u64) {
        for s in &mut self.spaces {
            if s.live && s.owner_depth == 0 && s.base == base {
                s.die();
            }
        }
    }

    /// Kill a space by number; pointers into it fail with `MCH3402`.
    pub fn kill(&mut self, space: u32) {
        if let Some(s) = self.spaces.get_mut(space as usize)
            && s.live
        {
            s.die();
        }
    }

    /// The space a pointer names, when live: its number and length.
    pub fn space_info(&self, p: FatPtr) -> Result<(u32, u64), Cond> {
        let s = self.space_of(p)?;
        Ok((p.space, s.hi - s.lo))
    }

    /// Forget a static area: the spaces inside it die, so pointers into
    /// a reclaimed activation group fail with `MCH3402`.
    pub fn kill_static(&mut self, base: u64) {
        self.statics.retain(|(b, _)| *b != base);
        self.system.retain(|(b, _)| *b != base);
        for s in &mut self.spaces {
            if s.live && s.owner_depth == 0 && s.base == base {
                s.die();
            }
        }
    }

    fn in_statics(&self, addr: u64, len: u64) -> bool {
        self.statics
            .iter()
            .any(|(b, l)| addr >= *b && addr.saturating_add(len) <= b.saturating_add(*l))
    }

    /// A space pointer to `len` bytes at a native address, owned by the
    /// call-stack entry at `depth` unless the storage is static. `frame`
    /// marks the entry's automatic storage, which shares the one frame
    /// space of that depth.
    pub fn addr(&mut self, native: u64, len: u64, depth: u32, frame: bool) -> FatPtr {
        if frame {
            return self.frame_addr(native, len, depth);
        }
        let owner = if self.in_statics(native, len) {
            0
        } else {
            depth
        };
        // Reuse a live space that already covers the bytes.
        for (i, s) in self.spaces.iter().enumerate() {
            if s.live
                && !s.frame
                && s.owner_depth == owner
                && native >= s.lo
                && native + len <= s.hi
            {
                return FatPtr::space(i, s.generation, native.wrapping_sub(s.base));
            }
        }
        // Grow a static space to cover the whole area when possible.
        if owner == 0
            && let Some((b, l)) = self
                .statics
                .iter()
                .find(|(b, l)| native >= *b && native + len <= b + l)
        {
            let (b, l) = (*b, *l);
            self.spaces.push(Space::new(b, l, 0, false));
            return FatPtr::space(self.spaces.len() - 1, 1, native - b);
        }
        self.spaces.push(Space::new(native, len, owner, false));
        FatPtr::space(self.spaces.len() - 1, 1, 0)
    }

    /// The frame space of `depth`, grown over the bytes.
    fn frame_addr(&mut self, native: u64, len: u64, depth: u32) -> FatPtr {
        let found = self
            .spaces
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.live && s.frame && s.owner_depth == depth);
        if let Some((i, s)) = found {
            s.lo = s.lo.min(native);
            s.hi = s.hi.max(native + len);
            return FatPtr::space(i, s.generation, native.wrapping_sub(s.base));
        }
        self.spaces.push(Space::new(native, len, depth, true));
        FatPtr::space(self.spaces.len() - 1, 1, 0)
    }

    fn space_of(&self, p: FatPtr) -> Result<&Space, Cond> {
        match p.kind() {
            Some(PtrKind::Null) => return Err(Cond::Mch3601),
            Some(PtrKind::Space) => {}
            _ => return Err(Cond::Mch3602),
        }
        let s = self.spaces.get(p.space as usize).ok_or(Cond::Mch3402)?;
        if !s.live || s.generation != p.generation {
            return Err(Cond::Mch3402);
        }
        Ok(s)
    }

    /// The native address a pointer addresses.
    pub fn deref(&self, p: FatPtr) -> Result<u64, Cond> {
        let s = self.space_of(p)?;
        let n = s.native(p);
        if n < s.lo || n >= s.hi {
            return Err(Cond::Mch0601);
        }
        Ok(n)
    }

    /// The pointer `n` bytes on; one past the end is allowed, as the
    /// machine allows, but not beyond.
    pub fn add(&self, p: FatPtr, n: i64) -> Result<FatPtr, Cond> {
        let s = self.space_of(p)?;
        let at = i128::from(s.native(p)) + i128::from(n);
        if at < i128::from(s.lo) || at > i128::from(s.hi) {
            return Err(Cond::Mch0601);
        }
        let at = u64::try_from(at).expect("in range");
        Ok(FatPtr {
            offset: at.wrapping_sub(s.base),
            ..p
        })
    }

    /// `p - q` in bytes, within one space.
    pub fn diff(&self, p: FatPtr, q: FatPtr) -> Result<i64, Cond> {
        let s = self.space_of(p)?;
        self.space_of(q)?;
        if p.space != q.space {
            return Err(Cond::Mch0601);
        }
        let d = i128::from(s.native(p)) - i128::from(s.native(q));
        Ok(i64::try_from(d).expect("fits"))
    }

    /// Kill the spaces owned by call-stack entries deeper than `depth`.
    pub fn leave(&mut self, depth: u32) {
        for s in &mut self.spaces {
            if s.live && s.owner_depth > depth {
                s.die();
            }
        }
    }

    /// Live spaces, for displays.
    #[must_use]
    pub fn live(&self) -> usize {
        self.spaces.iter().filter(|s| s.live).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_die_on_leave() {
        let mut sp = Spaces::default();
        sp.register_static(1000, 100);
        let st = sp.addr(1010, 4, 3, false);
        let fr = sp.addr(5000, 8, 3, true);
        assert_eq!(sp.deref(st).unwrap(), 1010);
        assert_eq!(sp.deref(fr).unwrap(), 5000);
        assert_eq!(sp.add(fr, 8).unwrap().offset, 8);
        assert_eq!(sp.add(fr, 9), Err(Cond::Mch0601));
        assert_eq!(sp.diff(sp.add(st, 20).unwrap(), st).unwrap(), 20);
        assert_eq!(sp.diff(st, fr), Err(Cond::Mch0601));
        sp.leave(2);
        assert_eq!(sp.deref(st).unwrap(), 1010);
        assert_eq!(sp.deref(fr), Err(Cond::Mch3402));
        assert_eq!(sp.deref(FatPtr::null()), Err(Cond::Mch3601));
        assert_eq!(sp.deref(FatPtr::procedure(7)), Err(Cond::Mch3602));
        let again = sp.addr(5000, 8, 3, true);
        assert_ne!((again.space, again.generation), (fr.space, fr.generation));
        assert_eq!(FatPtr::from_bytes(&again.to_bytes()), again);
    }

    #[test]
    fn a_frame_covers_the_entry_s_automatics() {
        let mut sp = Spaces::default();
        let first = sp.addr(5000, 8, 3, true);
        // A neighbour above joins the frame: one past `first` reads it.
        let above = sp.addr(5008, 4, 3, true);
        assert_eq!(above.space, first.space);
        assert_eq!(sp.deref(sp.add(first, 8).unwrap()).unwrap(), 5008);
        assert_eq!(sp.deref(sp.add(first, 11).unwrap()).unwrap(), 5011);
        assert_eq!(sp.add(first, 13), Err(Cond::Mch0601));
        assert_eq!(sp.diff(above, first).unwrap(), 8);
        // A neighbour below grows the frame down past the base.
        let below = sp.addr(4990, 4, 3, true);
        assert_eq!(below.space, first.space);
        assert_eq!(sp.deref(below).unwrap(), 4990);
        assert_eq!(sp.deref(sp.add(first, -10).unwrap()).unwrap(), 4990);
        assert_eq!(sp.add(first, -11), Err(Cond::Mch0601));
        assert_eq!(sp.diff(below, first).unwrap(), -10);
        assert_eq!(sp.diff(sp.add(below, 10).unwrap(), first).unwrap(), 0);
        assert_eq!(sp.space_info(first).unwrap().1, 22);
        // Another depth is another frame; heap blocks keep exact bounds.
        let deeper = sp.addr(5012, 4, 4, true);
        assert_ne!(deeper.space, first.space);
        assert_eq!(sp.diff(deeper, first), Err(Cond::Mch0601));
        let heap = sp.add_region(7000, 8);
        assert_eq!(sp.add(heap, 9), Err(Cond::Mch0601));
        sp.leave(3);
        assert_eq!(sp.deref(deeper), Err(Cond::Mch3402));
        assert_eq!(sp.deref(below).unwrap(), 4990);
        sp.leave(2);
        assert_eq!(sp.deref(below), Err(Cond::Mch3402));
        assert_eq!(sp.deref(heap).unwrap(), 7000);
    }
}
