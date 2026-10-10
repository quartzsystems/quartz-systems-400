// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Minimal ELF64 reader: enough to prove a binary is static and stripped.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const DT_NULL: i64 = 0;
const DT_NEEDED: i64 = 1;
const DT_STRTAB: i64 = 5;
const SHT_SYMTAB: u32 = 2;

/// What we learned about an ELF file.
#[derive(Debug)]
pub struct ElfInfo {
    /// `e_machine`.
    pub machine: u16,
    /// A `PT_INTERP` segment exists (dynamic loader requested).
    pub has_interp: bool,
    /// `DT_NEEDED` entries (shared library dependencies).
    pub needed: Vec<String>,
    /// A `.symtab` section exists (not stripped).
    pub has_symtab: bool,
}

struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn slice(&self, off: usize, len: usize) -> Result<&[u8]> {
        self.0
            .get(off..off.checked_add(len).context("ELF offset overflow")?)
            .context("ELF structure past end of file")
    }
    fn u16(&self, off: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(self.slice(off, 2)?.try_into()?))
    }
    fn u32(&self, off: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(self.slice(off, 4)?.try_into()?))
    }
    fn u64(&self, off: usize) -> Result<u64> {
        Ok(u64::from_le_bytes(self.slice(off, 8)?.try_into()?))
    }
    fn cstr(&self, off: usize) -> Result<String> {
        let rest = self
            .0
            .get(off..)
            .context("string offset past end of file")?;
        let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());
        Ok(String::from_utf8_lossy(&rest[..end]).into_owned())
    }
}

fn usize_of(v: u64) -> Result<usize> {
    usize::try_from(v).context("ELF value does not fit in usize")
}

/// Inspect a little-endian ELF64 file.
pub fn inspect(path: &Path) -> Result<ElfInfo> {
    let data = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let b = Bytes(&data);
    ensure!(
        b.slice(0, 4)? == b"\x7fELF",
        "{} is not an ELF file",
        path.display()
    );
    ensure!(data[4] == 2, "{} is not ELF64", path.display());
    ensure!(data[5] == 1, "{} is not little-endian", path.display());

    let machine = b.u16(0x12)?;
    let phoff = usize_of(b.u64(0x20)?)?;
    let shoff = usize_of(b.u64(0x28)?)?;
    let phentsize = usize::from(b.u16(0x36)?);
    let phnum = usize::from(b.u16(0x38)?);
    let shentsize = usize::from(b.u16(0x3a)?);
    let shnum = usize::from(b.u16(0x3c)?);

    let mut has_interp = false;
    let mut dynamic = None;
    let mut loads = Vec::new();
    for i in 0..phnum {
        let ph = phoff + i * phentsize;
        let p_type = b.u32(ph)?;
        let p_offset = b.u64(ph + 8)?;
        let p_vaddr = b.u64(ph + 16)?;
        let p_filesz = b.u64(ph + 32)?;
        match p_type {
            PT_INTERP => has_interp = true,
            PT_DYNAMIC => dynamic = Some((usize_of(p_offset)?, usize_of(p_filesz)?)),
            PT_LOAD => loads.push((p_vaddr, p_offset, p_filesz)),
            _ => {}
        }
    }

    let mut needed = Vec::new();
    if let Some((off, size)) = dynamic {
        let mut tags = Vec::new();
        let mut strtab = None;
        let mut pos = off;
        while pos + 16 <= off + size {
            let tag = i64::from_le_bytes(b.slice(pos, 8)?.try_into()?);
            let val = b.u64(pos + 8)?;
            match tag {
                DT_NULL => break,
                DT_NEEDED => tags.push(val),
                DT_STRTAB => strtab = Some(val),
                _ => {}
            }
            pos += 16;
        }
        let strtab_off = strtab.and_then(|va| vaddr_to_offset(&loads, va));
        for val in tags {
            match strtab_off {
                Some(base) => needed.push(b.cstr(usize_of(base + val)?)?),
                None => needed.push(format!("<strtab+{val:#x}>")),
            }
        }
    }

    let mut has_symtab = false;
    if shoff != 0 {
        for i in 0..shnum {
            let sh = shoff + i * shentsize;
            if b.u32(sh + 4)? == SHT_SYMTAB {
                has_symtab = true;
                break;
            }
        }
    }

    if phnum == 0 {
        bail!("{} has no program headers", path.display());
    }
    Ok(ElfInfo {
        machine,
        has_interp,
        needed,
        has_symtab,
    })
}

/// Map a virtual address to a file offset through the `PT_LOAD` segments.
fn vaddr_to_offset(loads: &[(u64, u64, u64)], va: u64) -> Option<u64> {
    loads
        .iter()
        .find(|(vaddr, _, filesz)| va >= *vaddr && va < vaddr + filesz)
        .map(|(vaddr, offset, _)| offset + (va - vaddr))
}
