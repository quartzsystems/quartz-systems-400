// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The XDR encoder and decoder the generated types use (RFC 4506).
//!
//! [`Xdr`] is what every generated type implements; the base types,
//! fixed opaques (`[u8; N]`), `Option` and `Box` implement it here.
//! Bounded items (`opaque<N>`, `string<N>`, `T<N>`, `T[N]`) and lists
//! are coded through [`Encoder`] and [`Decoder`] methods that take the
//! bound, so a bound is checked on encode and decode alike.
//! [`ProcBody`] is what a program version's generated `Args` and `Res`
//! implement: the arguments or results of every procedure.

mod decode;
mod encode;
mod error;

pub use decode::{Decoder, MAX_DEPTH};
pub use encode::Encoder;
pub use error::{Error, Result};

/// A type with an XDR encoding.
pub trait Xdr: Sized {
    /// The fewest bytes a value encodes to (arrays check counts with it).
    const MIN_SIZE: usize;
    /// Appends the value.
    fn encode(&self, e: &mut Encoder) -> Result<()>;
    /// Decodes a value.
    fn decode(d: &mut Decoder<'_>) -> Result<Self>;
}

/// The arguments or the results of every procedure of one program
/// version (the generated `Args` and `Res`).
pub trait ProcBody: Sized {
    /// The program number.
    const PROGRAM: u32;
    /// The version number.
    const VERSION: u32;
    /// The procedure this value belongs to.
    fn procedure(&self) -> u32;
    /// Appends the value, without its procedure number.
    fn encode_body(&self, e: &mut Encoder) -> Result<()>;
    /// Decodes the body of `procedure`; `None` when the version has no such procedure.
    fn decode_body(procedure: u32, d: &mut Decoder<'_>) -> Result<Option<Self>>;
}

/// `v` encoded.
pub fn to_bytes<T: Xdr>(v: &T) -> Result<Vec<u8>> {
    let mut e = Encoder::new();
    v.encode(&mut e)?;
    Ok(e.into_bytes())
}

/// A `T` from all of `b`.
pub fn from_bytes<T: Xdr>(b: &[u8]) -> Result<T> {
    let mut d = Decoder::new(b);
    let v = T::decode(&mut d)?;
    d.finish()?;
    Ok(v)
}

impl Xdr for u32 {
    const MIN_SIZE: usize = 4;
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.put_u32(*self);
        Ok(())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.get_u32()
    }
}

impl Xdr for i32 {
    const MIN_SIZE: usize = 4;
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.put_i32(*self);
        Ok(())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.get_i32()
    }
}

impl Xdr for u64 {
    const MIN_SIZE: usize = 8;
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.put_u64(*self);
        Ok(())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.get_u64()
    }
}

impl Xdr for i64 {
    const MIN_SIZE: usize = 8;
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.put_i64(*self);
        Ok(())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.get_i64()
    }
}

impl Xdr for bool {
    const MIN_SIZE: usize = 4;
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.put_bool(*self);
        Ok(())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        d.get_bool()
    }
}

/// `void`.
impl Xdr for () {
    const MIN_SIZE: usize = 0;
    fn encode(&self, _: &mut Encoder) -> Result<()> {
        Ok(())
    }
    fn decode(_: &mut Decoder<'_>) -> Result<Self> {
        Ok(())
    }
}

/// `opaque x[N]`.
impl<const N: usize> Xdr for [u8; N] {
    const MIN_SIZE: usize = N.div_ceil(4) * 4;
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.put_fixed(self);
        Ok(())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        let b = d.get_fixed(N)?;
        let mut a = [0u8; N];
        a.copy_from_slice(b);
        Ok(a)
    }
}

/// `T *x`, and a bool union of `TRUE: T` and `FALSE: void`.
impl<T: Xdr> Xdr for Option<T> {
    const MIN_SIZE: usize = 4;
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        e.put_bool(self.is_some());
        self.as_ref().map_or(Ok(()), |v| v.encode(e))
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        if d.get_bool()? {
            d.enter()?;
            let v = T::decode(d)?;
            d.leave();
            Ok(Some(v))
        } else {
            Ok(None)
        }
    }
}

impl<T: Xdr> Xdr for Box<T> {
    const MIN_SIZE: usize = T::MIN_SIZE;
    fn encode(&self, e: &mut Encoder) -> Result<()> {
        self.as_ref().encode(e)
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self> {
        T::decode(d).map(Box::new)
    }
}

#[cfg(test)]
mod tests;
