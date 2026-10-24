// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Key and value encodings. Keys are fixed size and big-endian so that
//! byte order equals key order; values are whatever the tree's user
//! needs, up to [`MAX_VALUE_LEN`](super::node::MAX_VALUE_LEN) bytes.

use crate::obj::ObjectId;
use crate::{Result, StoreError};

/// A fixed-size key.
pub trait FixedKey: Ord + Clone + std::fmt::Debug {
    /// Encoded size in bytes, at most 256.
    const LEN: usize;
    /// Encode into exactly `LEN` bytes.
    fn write(&self, out: &mut [u8]);
    /// Decode from exactly `LEN` bytes.
    fn read(bytes: &[u8]) -> Self;
    /// Compare a stored key with `other`, as `Self::read(bytes).cmp(other)`.
    fn cmp_stored(bytes: &[u8], other: &Self) -> std::cmp::Ordering {
        Self::read(bytes).cmp(other)
    }
}

/// A value.
pub trait Value: Clone + std::fmt::Debug {
    /// Encode.
    fn to_bytes(&self) -> Vec<u8>;
    /// Decode; the bytes came from `to_bytes` of a well-formed value.
    fn from_bytes(bytes: &[u8]) -> Result<Self>;
}

fn short(what: &str, want: usize, got: usize) -> StoreError {
    StoreError::BadValue {
        what: format!("{what}: want {want} bytes, got {got}"),
    }
}

impl FixedKey for u64 {
    const LEN: usize = 8;
    fn write(&self, out: &mut [u8]) {
        out[..8].copy_from_slice(&self.to_be_bytes());
    }
    fn read(bytes: &[u8]) -> Self {
        u64::from_be_bytes(bytes[..8].try_into().expect("8 bytes"))
    }
}

impl FixedKey for ObjectId {
    const LEN: usize = 16;
    fn write(&self, out: &mut [u8]) {
        out[..16].copy_from_slice(&self.to_bytes());
    }
    fn read(bytes: &[u8]) -> Self {
        ObjectId::from_bytes(&bytes[..16])
    }
}

/// `(unit, lba)`: the allocator's key.
impl FixedKey for (u16, u64) {
    const LEN: usize = 10;
    fn write(&self, out: &mut [u8]) {
        out[..2].copy_from_slice(&self.0.to_be_bytes());
        out[2..10].copy_from_slice(&self.1.to_be_bytes());
    }
    fn read(bytes: &[u8]) -> Self {
        (
            u16::from_be_bytes([bytes[0], bytes[1]]),
            u64::from_be_bytes(bytes[2..10].try_into().expect("8 bytes")),
        )
    }
}

impl Value for u64 {
    fn to_bytes(&self) -> Vec<u8> {
        self.to_le_bytes().to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let b: [u8; 8] = bytes.try_into().map_err(|_| short("u64", 8, bytes.len()))?;
        Ok(u64::from_le_bytes(b))
    }
}

impl Value for ObjectId {
    fn to_bytes(&self) -> Vec<u8> {
        ObjectId::to_bytes(self).to_vec()
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 16 {
            return Err(short("ObjectId", 16, bytes.len()));
        }
        Ok(ObjectId::from_bytes(bytes))
    }
}

impl Value for Vec<u8> {
    fn to_bytes(&self) -> Vec<u8> {
        self.clone()
    }
    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(bytes.to_vec())
    }
}

impl Value for () {
    fn to_bytes(&self) -> Vec<u8> {
        Vec::new()
    }
    fn from_bytes(_: &[u8]) -> Result<Self> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sort_bytewise() {
        let mut a = [0u8; 10];
        let mut b = [0u8; 10];
        (1u16, 5u64).write(&mut a);
        (1u16, 6u64).write(&mut b);
        assert!(a < b);
        assert_eq!(<(u16, u64)>::read(&b), (1, 6));
        let mut k = [0u8; 8];
        300u64.write(&mut k);
        assert_eq!(u64::read(&k), 300);
    }

    #[test]
    fn values_round_trip() {
        assert_eq!(u64::from_bytes(&7u64.to_bytes()).unwrap(), 7);
        assert!(matches!(
            u64::from_bytes(&[1, 2]),
            Err(StoreError::BadValue { .. })
        ));
        assert_eq!(<Vec<u8>>::from_bytes(&[1, 2]).unwrap(), vec![1, 2]);
        let id = ObjectId {
            asp: 1,
            type_code: 9,
            serial: 2,
            generation: 0,
        };
        assert_eq!(
            <ObjectId as Value>::from_bytes(&Value::to_bytes(&id)).unwrap(),
            id
        );
    }
}

/// A key whose length is fixed per tree but known only at run time:
/// the raw big-endian bytes. Open the tree with `with_key_len`; `LEN`
/// is 0 and never used for layout.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DynKey(pub Vec<u8>);

impl FixedKey for DynKey {
    const LEN: usize = 0;
    fn write(&self, out: &mut [u8]) {
        out[..self.0.len()].copy_from_slice(&self.0);
    }
    fn read(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
    fn cmp_stored(bytes: &[u8], other: &Self) -> std::cmp::Ordering {
        bytes.cmp(other.0.as_slice())
    }
}
