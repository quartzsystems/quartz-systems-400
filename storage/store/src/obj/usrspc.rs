// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*USRSPC`: a byte space with a size, an initial value and an
//! extendable flag. See the format spec §10.

use crate::bytes::{get_u64, put_u64};
use crate::obj::ObjectId;
use crate::obj::ops::{self, CreateParams};
use crate::obj::space::Space;
use crate::obj::types;
use crate::spec::{Layout, field};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// Bytes of the attribute area used.
pub const ATTR_LEN: usize = 16;
/// Largest user space, 16 MiB in Phase 2.
pub const MAX_SIZE: u64 = 16 << 20;

/// The spec layout.
pub static LAYOUT: Layout = Layout {
    name: "UsrspcAttr",
    size: Some(ATTR_LEN),
    fields: &[
        field("size", 0, 8),
        field("extendable", 8, 1),
        field("initial_value", 9, 1),
        field("reserved", 10, 6),
    ],
};

/// The attribute area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpaceAttr {
    /// Size in bytes.
    pub size: u64,
    /// Whether writes past the end grow the space.
    pub extendable: bool,
    /// Fill byte for new space.
    pub initial_value: u8,
}

impl SpaceAttr {
    fn encode(&self) -> [u8; ATTR_LEN] {
        let mut a = [0u8; ATTR_LEN];
        put_u64(&mut a, 0, self.size);
        a[8] = u8::from(self.extendable);
        a[9] = self.initial_value;
        a
    }

    fn decode(a: &[u8]) -> Self {
        Self {
            size: get_u64(a, 0),
            extendable: a[8] != 0,
            initial_value: a[9],
        }
    }
}

fn too_big(size: u64) -> StoreError {
    StoreError::BadValue {
        what: format!("user space size {size} exceeds {MAX_SIZE}"),
    }
}

/// Create a user space of `size` bytes.
pub fn create(
    txn: &mut Txn<'_>,
    params: &CreateParams,
    size: u64,
    extendable: bool,
    initial_value: u8,
) -> Result<ObjectId> {
    if size > MAX_SIZE {
        return Err(too_big(size));
    }
    let params = CreateParams {
        type_code: types::USRSPC,
        ..params.clone()
    };
    let id = ops::create(txn, &params)?;
    let attr = SpaceAttr {
        size,
        extendable,
        initial_value,
    };
    save(txn, id, &attr)?;
    Space::ensure(txn, id, size, initial_value)?;
    Ok(id)
}

/// The attribute area.
pub fn describe(txn: &mut Txn<'_>, id: ObjectId) -> Result<SpaceAttr> {
    let (page, h) = ops::read_header(txn, id)?;
    if h.id.type_code != types::USRSPC {
        return Err(StoreError::ObjectNotFound {
            name: h.name.to_string(),
            library: String::new(),
        });
    }
    let a = ops::read_attr(txn, page)?;
    Ok(SpaceAttr::decode(&a[..ATTR_LEN]))
}

fn save(txn: &mut Txn<'_>, id: ObjectId, attr: &SpaceAttr) -> Result<()> {
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &attr.encode())
}

fn outside(off: u64, len: usize, size: u64) -> StoreError {
    StoreError::BadValue {
        what: format!("{off}+{len} is outside a {size}-byte user space"),
    }
}

/// Read `len` bytes at `offset`.
pub fn read(txn: &mut Txn<'_>, id: ObjectId, offset: u64, len: usize) -> Result<Vec<u8>> {
    let attr = describe(txn, id)?;
    if offset + len as u64 > attr.size {
        return Err(outside(offset, len, attr.size));
    }
    let (space, _) = Space::load(txn, id)?;
    space.read(txn, offset, len)
}

/// Write `bytes` at `offset`, growing an extendable space as needed.
pub fn write(txn: &mut Txn<'_>, id: ObjectId, offset: u64, bytes: &[u8]) -> Result<()> {
    let attr = describe(txn, id)?;
    let end = offset + bytes.len() as u64;
    if end > attr.size {
        if !attr.extendable {
            return Err(outside(offset, bytes.len(), attr.size));
        }
        change_size(txn, id, end)?;
    }
    let (space, _) = Space::load(txn, id)?;
    space.write(txn, offset, bytes)
}

/// Resize. Growth is filled with the initial value.
pub fn change_size(txn: &mut Txn<'_>, id: ObjectId, size: u64) -> Result<()> {
    if size > MAX_SIZE {
        return Err(too_big(size));
    }
    let mut attr = describe(txn, id)?;
    if size > attr.size {
        Space::ensure(txn, id, size, attr.initial_value)?;
    } else if size < attr.size {
        Space::truncate(txn, id, size)?;
    }
    attr.size = size;
    save(txn, id, &attr)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attr_round_trip() {
        let a = SpaceAttr {
            size: 1 << 20,
            extendable: true,
            initial_value: 0x40,
        };
        assert_eq!(SpaceAttr::decode(&a.encode()), a);
    }
}
