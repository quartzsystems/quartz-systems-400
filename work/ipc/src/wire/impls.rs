// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`Wire`] for integers, `bool`, strings, options, boxes, vectors,
//! byte arrays and tuples.

use super::{Dec, Wire};
use crate::WireError;

macro_rules! int {
    ($($t:ty),*) => {$(
        impl Wire for $t {
            fn put(&self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
            fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
                let b = d.take(size_of::<$t>())?;
                Ok(<$t>::from_le_bytes(b.try_into().expect("sized")))
            }
        }
    )*};
}
int!(u16, u32, u64, u128, i8, i16, i32, i64, i128);

impl Wire for u8 {
    fn put(&self, out: &mut Vec<u8>) {
        out.push(*self);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(d.take(1)?[0])
    }
    fn put_slice(items: &[Self], out: &mut Vec<u8>) {
        out.extend_from_slice(items);
    }
    fn get_vec(d: &mut Dec<'_>, n: usize) -> Result<Vec<Self>, WireError> {
        Ok(d.take(n)?.to_vec())
    }
}

impl Wire for usize {
    fn put(&self, out: &mut Vec<u8>) {
        (*self as u64).put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let at = d.remaining();
        usize::try_from(u64::get(d)?).map_err(|_| WireError::Truncated { at })
    }
}

impl Wire for bool {
    fn put(&self, out: &mut Vec<u8>) {
        out.push(u8::from(*self));
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        match u8::get(d)? {
            0 => Ok(false),
            1 => Ok(true),
            tag => Err(WireError::Tag { what: "bool", tag }),
        }
    }
}

/// A length as `u32`.
fn put_len(n: usize, out: &mut Vec<u8>) {
    u32::try_from(n).expect("length fits u32").put(out);
}

fn get_len(d: &mut Dec<'_>) -> Result<usize, WireError> {
    Ok(u32::get(d)? as usize)
}

impl Wire for String {
    fn put(&self, out: &mut Vec<u8>) {
        put_len(self.len(), out);
        out.extend_from_slice(self.as_bytes());
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let n = get_len(d)?;
        String::from_utf8(d.take(n)?.to_vec()).map_err(|_| WireError::Utf8)
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn put(&self, out: &mut Vec<u8>) {
        put_len(self.len(), out);
        T::put_slice(self, out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let n = get_len(d)?;
        T::get_vec(d, n)
    }
}

impl<T: Wire> Wire for Option<T> {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            None => out.push(0),
            Some(v) => {
                out.push(1);
                v.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        match u8::get(d)? {
            0 => Ok(None),
            1 => Ok(Some(T::get(d)?)),
            tag => Err(WireError::Tag {
                what: "Option",
                tag,
            }),
        }
    }
}

impl<T: Wire, E: Wire> Wire for Result<T, E> {
    fn put(&self, out: &mut Vec<u8>) {
        match self {
            Ok(v) => {
                out.push(0);
                v.put(out);
            }
            Err(e) => {
                out.push(1);
                e.put(out);
            }
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        match u8::get(d)? {
            0 => Ok(Ok(T::get(d)?)),
            1 => Ok(Err(E::get(d)?)),
            tag => Err(WireError::Tag {
                what: "Result",
                tag,
            }),
        }
    }
}

impl<T: Wire> Wire for Box<T> {
    fn put(&self, out: &mut Vec<u8>) {
        (**self).put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(Box::new(T::get(d)?))
    }
}

impl<const N: usize> Wire for [u8; N] {
    fn put(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(d.take(N)?.try_into().expect("sized"))
    }
}

impl Wire for () {
    fn put(&self, _: &mut Vec<u8>) {}
    fn get(_: &mut Dec<'_>) -> Result<Self, WireError> {
        Ok(())
    }
}

macro_rules! tuple {
    ($($n:ident),+) => {
        impl<$($n: Wire),+> Wire for ($($n,)+) {
            #[allow(non_snake_case)]
            fn put(&self, out: &mut Vec<u8>) {
                let ($($n,)+) = self;
                $($n.put(out);)+
            }
            fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
                Ok(($($n::get(d)?,)+))
            }
        }
    };
}
tuple!(A);
tuple!(A, B);
tuple!(A, B, C);
tuple!(A, B, C, D);
tuple!(A, B, C, D, E);
tuple!(A, B, C, D, E, F);

impl Wire for char {
    fn put(&self, out: &mut Vec<u8>) {
        u32::from(*self).put(out);
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        char::from_u32(u32::get(d)?).ok_or(WireError::Utf8)
    }
}

impl<const N: usize> Wire for [String; N] {
    fn put(&self, out: &mut Vec<u8>) {
        for s in self {
            s.put(out);
        }
    }
    fn get(d: &mut Dec<'_>) -> Result<Self, WireError> {
        let mut v = Vec::with_capacity(N);
        for _ in 0..N {
            v.push(String::get(d)?);
        }
        Ok(v.try_into().expect("sized"))
    }
}
