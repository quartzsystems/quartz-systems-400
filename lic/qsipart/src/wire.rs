// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Bodies: a message's fields as [`ipc::Wire`] encodes them, read
//! leniently (fields a later version adds after them are passed over),
//! and the small enums whose values a later version may add to.

use ipc::{Dec, Wire};

use crate::Error;
use crate::frame::{self, Frame};

/// A message's frame.
///
/// # Errors
/// The body is longer than a frame holds.
pub(crate) fn frame_of<T: Wire>(kind: u16, body: &T) -> Result<Vec<u8>, Error> {
    frame::encode(kind, &ipc::encode(body))
}

/// A body as `T`; what follows `T`'s fields is a later version's.
pub(crate) fn body<T: Wire>(f: &Frame) -> Result<T, Error> {
    let mut d = Dec::new(&f.body);
    T::get(&mut d).map_err(|source| Error::Body {
        kind: f.kind,
        source,
    })
}

/// An enum of one byte whose values a later version may add to: an
/// unknown value is kept as `Other`.
macro_rules! open_enum {
    ($(#[$m:meta])* $t:ident { $($(#[$vm:meta])* $v:ident = $n:literal),* $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $t {
            $($(#[$vm])* $v,)*
            /// A value a later version has.
            Other(u8),
        }

        impl $t {
            /// The byte on the wire.
            #[must_use]
            pub fn as_u8(self) -> u8 {
                match self {
                    $(Self::$v => $n,)*
                    Self::Other(b) => b,
                }
            }

            /// From the byte on the wire.
            #[must_use]
            pub fn from_u8(b: u8) -> Self {
                match b {
                    $($n => Self::$v,)*
                    b => Self::Other(b),
                }
            }
        }

        impl ipc::Wire for $t {
            fn put(&self, out: &mut Vec<u8>) {
                out.push(self.as_u8());
            }
            fn get(d: &mut ipc::Dec<'_>) -> Result<Self, ipc::WireError> {
                Ok(Self::from_u8(u8::get(d)?))
            }
        }
    };
}
pub(crate) use open_enum;
