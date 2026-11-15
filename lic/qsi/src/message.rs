// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Families and their messages. A family is a set of operations with a
//! number ([`Family::ID`]), a version and a capability bit; each
//! operation has a number within it. A request of operation `n` is
//! answered by a reply of operation `n` (or a refusal), and a family's
//! events have numbers of their own. A body is an [`ipc::Wire`] value,
//! read leniently: what follows the fields a receiver knows is a later
//! version's and passed over.
//!
//! [`messages!`] writes a family's enumeration of messages and its
//! [`Message`] implementation from a list of `operation => Variant(Type)`.

use crate::Error;

/// One of a family's enumerations: its requests, its replies or its
/// events.
pub trait Message: Sized + Clone + std::fmt::Debug + PartialEq {
    /// The family's number.
    const FAMILY: u16;
    /// Every operation this side knows, in order.
    const OPS: &'static [u16];

    /// The operation's number.
    fn op(&self) -> u16;
    /// The body.
    fn encode(&self) -> Vec<u8>;
    /// The message of operation `op` with `body`; `None` when this side
    /// does not know the operation.
    ///
    /// # Errors
    /// The body does not read.
    fn decode(op: u16, body: &[u8]) -> Result<Option<Self>, Error>;
}

/// A family: its number, version and capability bit, and its three
/// enumerations.
pub trait Family {
    /// The family's number.
    const ID: u16;
    /// The family's version this crate speaks.
    const VERSION: u16;
    /// The capability bit that says a side has this version.
    const CAP: u64;
    /// What a console asks.
    type Request: Message;
    /// What the managed side answers.
    type Reply: Message;
    /// What the managed side pushes.
    type Event: Message;
}

/// A body as `T`, leniently.
///
/// # Errors
/// The body does not read as `T`.
pub fn body<T: ipc::Wire>(family: u16, op: u16, bytes: &[u8]) -> Result<T, Error> {
    let mut d = ipc::Dec::new(bytes);
    T::get(&mut d).map_err(|source| Error::Body { family, op, source })
}

/// A family's enumeration of messages:
///
/// ```ignore
/// messages! {
///     /// What a console asks.
///     pub enum Request: family::BASE {
///         /// Who the system is.
///         0x0001 => Identify(Identify),
///     }
/// }
/// ```
#[macro_export]
macro_rules! messages {
    ($(#[$m:meta])* pub enum $t:ident : $fam:expr => { $($(#[$vm:meta])* $op:literal => $v:ident($ty:ty)),* $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq)]
        pub enum $t {
            $($(#[$vm])* $v($ty),)*
        }

        impl $crate::Message for $t {
            const FAMILY: u16 = $fam;
            const OPS: &'static [u16] = &[$($op),*];

            fn op(&self) -> u16 {
                match *self {
                    $(Self::$v(_) => $op,)*
                }
            }

            fn encode(&self) -> Vec<u8> {
                match *self {
                    $(Self::$v(ref m) => ::ipc::encode(m),)*
                }
            }

            fn decode(op: u16, body: &[u8]) -> ::core::result::Result<Option<Self>, $crate::Error> {
                match op {
                    $($op => Ok(Some(Self::$v($crate::message::body(Self::FAMILY, op, body)?))),)*
                    _ => { let _ = body; Ok(None) }
                }
            }
        }
    };
}

/// An enum of one byte whose values a later version may add to: an
/// unknown value is kept as `Other`.
#[macro_export]
macro_rules! open_enum {
    ($(#[$m:meta])* $t:ident { $($(#[$vm:meta])* $v:ident = $n:literal),* $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

        impl ::ipc::Wire for $t {
            fn put(&self, out: &mut Vec<u8>) {
                out.push(self.as_u8());
            }
            fn get(d: &mut ::ipc::Dec<'_>) -> Result<Self, ::ipc::WireError> {
                Ok(Self::from_u8(<u8 as ::ipc::Wire>::get(d)?))
            }
        }
    };
}

/// A message with no fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Empty;

impl ipc::Wire for Empty {
    fn put(&self, _out: &mut Vec<u8>) {}
    fn get(_d: &mut ipc::Dec<'_>) -> Result<Self, ipc::WireError> {
        Ok(Self)
    }
}
