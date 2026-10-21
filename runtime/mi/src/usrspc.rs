// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! User spaces mapped into the job: `QUSPTRUS` hands out a space
//! pointer to a copy of the object's bytes, changes go back to the
//! object when the job syncs, and a deleted or unmapped space leaves
//! its pointers dead. The object store behind it is a trait, so the
//! runtime does not depend on where `*USRSPC` objects live.

use codegen::Cond;

use crate::space::{FatPtr, Spaces};

/// Where user space contents come from and go to.
pub trait UserSpaceSource {
    /// The bytes of `lib/name`, or `None` when it does not exist.
    fn load(&mut self, name: &str, lib: &str) -> Option<Vec<u8>>;
    /// Write the bytes back; `false` when the object is gone.
    fn store(&mut self, name: &str, lib: &str, data: &[u8]) -> bool;
    /// Create `lib/name` of `size` bytes filled with `init`, or fail
    /// with the message identifier (`CPF9870` exists, `CPF9810` no
    /// such library).
    fn create(
        &mut self,
        _name: &str,
        _lib: &str,
        _size: usize,
        _init: u8,
        _text: &str,
    ) -> Result<(), &'static str> {
        Err("CPF9810")
    }
    /// Delete `lib/name`; `false` when absent.
    fn delete(&mut self, _name: &str, _lib: &str) -> bool {
        false
    }
    /// The library an existing space was found in.
    fn library_of(&mut self, _name: &str, _lib: &str) -> Option<String> {
        None
    }
}

/// User spaces held in memory, for tests and the IR driver.
#[derive(Default)]
pub struct MemUserSpaces {
    /// `(library, name, bytes)`.
    pub spaces: Vec<(String, String, Vec<u8>)>,
}

impl UserSpaceSource for MemUserSpaces {
    fn load(&mut self, name: &str, lib: &str) -> Option<Vec<u8>> {
        self.spaces
            .iter()
            .find(|(l, n, _)| n == name && (lib == "*LIBL" || lib == "*CURLIB" || l == lib))
            .map(|(_, _, d)| d.clone())
    }

    fn store(&mut self, name: &str, lib: &str, data: &[u8]) -> bool {
        match self
            .spaces
            .iter_mut()
            .find(|(l, n, _)| n == name && (lib == "*LIBL" || lib == "*CURLIB" || l == lib))
        {
            Some((_, _, d)) => {
                *d = data.to_vec();
                true
            }
            None => false,
        }
    }

    fn create(
        &mut self,
        name: &str,
        lib: &str,
        size: usize,
        init: u8,
        _text: &str,
    ) -> Result<(), &'static str> {
        let lib = if lib == "*LIBL" || lib == "*CURLIB" {
            "QGPL"
        } else {
            lib
        };
        if self.spaces.iter().any(|(l, n, _)| n == name && l == lib) {
            return Err("CPF9870");
        }
        self.spaces
            .push((lib.into(), name.into(), vec![init; size]));
        Ok(())
    }

    fn delete(&mut self, name: &str, lib: &str) -> bool {
        let before = self.spaces.len();
        self.spaces
            .retain(|(l, n, _)| !(n == name && (lib == "*LIBL" || lib == "*CURLIB" || l == lib)));
        before != self.spaces.len()
    }

    fn library_of(&mut self, name: &str, lib: &str) -> Option<String> {
        self.spaces
            .iter()
            .find(|(l, n, _)| n == name && (lib == "*LIBL" || lib == "*CURLIB" || l == lib))
            .map(|(l, _, _)| l.clone())
    }
}

/// A mapped user space.
pub struct Mapped {
    /// Object name.
    pub name: String,
    /// Library as given.
    pub lib: String,
    /// The copy pointers address.
    pub data: Vec<u8>,
    /// The space.
    pub space: u32,
}

/// The job's mapped user spaces.
#[derive(Default)]
pub struct UserSpaces {
    source: Option<Box<dyn UserSpaceSource>>,
    mapped: Vec<Mapped>,
}

impl UserSpaces {
    /// Where the objects live.
    pub fn set_source(&mut self, source: Box<dyn UserSpaceSource>) {
        self.source = Some(source);
    }

    /// Map `lib/name`, once; the pointer to its first byte.
    pub fn map(&mut self, name: &str, lib: &str, spaces: &mut Spaces) -> Result<FatPtr, Cond> {
        if let Some(m) = self.mapped.iter().find(|m| m.name == name && m.lib == lib) {
            return Ok(FatPtr {
                kind: crate::space::PtrKind::Space as u8,
                flags: 0,
                generation: 1,
                space: m.space,
                offset: 0,
            });
        }
        let src = self.source.as_mut().ok_or(Cond::Mch3401)?;
        let data = src.load(name, lib).ok_or(Cond::Mch3401)?;
        let mut data = data;
        if data.is_empty() {
            data.push(0);
        }
        let p = spaces.add_region(data.as_ptr() as u64, data.len() as u64);
        self.mapped.push(Mapped {
            name: name.into(),
            lib: lib.into(),
            data,
            space: p.space,
        });
        Ok(p)
    }

    /// Write every mapped space back to its object.
    pub fn sync(&mut self) {
        let Some(src) = self.source.as_mut() else {
            return;
        };
        for m in &self.mapped {
            src.store(&m.name, &m.lib, &m.data);
        }
    }

    /// Unmap `lib/name` without writing it back, as a delete does;
    /// pointers into it die.
    pub fn unmap(&mut self, name: &str, lib: &str, spaces: &mut Spaces) -> bool {
        let Some(i) = self
            .mapped
            .iter()
            .position(|m| m.name == name && m.lib == lib)
        else {
            return false;
        };
        let m = self.mapped.remove(i);
        spaces.kill(m.space);
        true
    }

    /// Create an object through the source.
    pub fn create(
        &mut self,
        name: &str,
        lib: &str,
        size: usize,
        init: u8,
        text: &str,
    ) -> Result<(), &'static str> {
        match self.source.as_mut() {
            Some(s) => s.create(name, lib, size, init, text),
            None => Err("CPF9810"),
        }
    }

    /// Delete an object: unmapped first, so pointers into it die.
    pub fn delete(&mut self, name: &str, lib: &str, spaces: &mut Spaces) -> bool {
        self.unmap(name, lib, spaces);
        self.source.as_mut().is_some_and(|s| s.delete(name, lib))
    }

    /// The library an existing object was found in.
    pub fn library_of(&mut self, name: &str, lib: &str) -> Option<String> {
        self.source.as_mut().and_then(|s| s.library_of(name, lib))
    }

    /// Grow a mapped space to `len` bytes: the copy moves, so the old
    /// region dies and pointers handed out before fail with `MCH3402`.
    pub fn grow(&mut self, name: &str, lib: &str, len: usize, spaces: &mut Spaces) -> bool {
        let Some(m) = self
            .mapped
            .iter_mut()
            .find(|m| m.name == name && m.lib == lib)
        else {
            return false;
        };
        if m.data.len() >= len {
            return true;
        }
        let mut data = std::mem::take(&mut m.data);
        data.resize(len, 0);
        spaces.kill(m.space);
        let p = spaces.add_region(data.as_ptr() as u64, data.len() as u64);
        m.data = data;
        m.space = p.space;
        true
    }

    /// The mapped copy of `lib/name`, writable; it is written back on
    /// the next sync.
    pub fn mapped_mut(&mut self, name: &str, lib: &str) -> Option<&mut Vec<u8>> {
        self.mapped
            .iter_mut()
            .find(|m| m.name == name && m.lib == lib)
            .map(|m| &mut m.data)
    }

    /// The mapped copy of `lib/name`, for displays and tests.
    #[must_use]
    pub fn mapped(&self, name: &str, lib: &str) -> Option<&[u8]> {
        self.mapped
            .iter()
            .find(|m| m.name == name && m.lib == lib)
            .map(|m| m.data.as_slice())
    }
}
