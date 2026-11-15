// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Any family's message, by its family and operation numbers: what a
//! session hands up once it has a request, a reply or an event's
//! numbers and body.

use crate::host::{console, dumps, host, panel, partition, partitions, profiles, refcodes, vres};
use crate::{Error, Message, base, pair};

/// Write the three enumerations over every family.
macro_rules! any {
    ($($(#[$m:meta])* $v:ident => $module:ident),* $(,)?) => {
        /// A request of any family.
        #[derive(Clone, Debug, PartialEq)]
        #[allow(clippy::large_enum_variant)] // a message is passed whole, once
        pub enum AnyRequest {
            $($(#[$m])* $v($module::Request),)*
        }

        /// A reply of any family.
        #[derive(Clone, Debug, PartialEq)]
        #[allow(clippy::large_enum_variant)] // a message is passed whole, once
        pub enum AnyReply {
            $($(#[$m])* $v($module::Reply),)*
        }

        /// An event of any family.
        #[derive(Clone, Debug, PartialEq)]
        #[allow(clippy::large_enum_variant)] // a message is passed whole, once
        pub enum AnyEvent {
            $($(#[$m])* $v($module::Event),)*
        }

        /// The families this crate knows, by number.
        pub const FAMILIES: &[u16] = &[$(<$module::Request as Message>::FAMILY),*];

        impl AnyRequest {
            /// The protocol's bounds: what a receiver refuses with
            /// [`crate::msgid::BAD`] before it acts.
            pub fn check(&self) -> Result<(), &'static str> {
                match self {
                    $(Self::$v(m) => m.check(),)*
                }
            }
        }

        any!(@impl AnyRequest, Request, $($v => $module),*);
        any!(@impl AnyReply, Reply, $($v => $module),*);
        any!(@impl AnyEvent, Event, $($v => $module),*);
    };
    (@impl $t:ident, $inner:ident, $($v:ident => $module:ident),*) => {
        impl $t {
            /// The family's number.
            #[must_use]
            pub fn family(&self) -> u16 {
                match self {
                    $(Self::$v(_) => <$module::$inner as Message>::FAMILY,)*
                }
            }

            /// The operation's number.
            #[must_use]
            pub fn op(&self) -> u16 {
                match self {
                    $(Self::$v(m) => m.op(),)*
                }
            }

            /// The body.
            #[must_use]
            pub fn encode(&self) -> Vec<u8> {
                match self {
                    $(Self::$v(m) => m.encode(),)*
                }
            }

            /// The message of `family` and `op` with `body`; `None` when
            /// this side does not know the family or the operation.
            ///
            /// # Errors
            /// The body does not read.
            pub fn decode(family: u16, op: u16, body: &[u8]) -> Result<Option<Self>, Error> {
                $(
                    if family == <$module::$inner as Message>::FAMILY {
                        return Ok(<$module::$inner as Message>::decode(op, body)?.map(Self::$v));
                    }
                )*
                Ok(None)
            }
        }
    };
}

any! {
    /// The base family.
    Base => base,
    /// Pairing.
    Pair => pair,
    /// A host's resources and hardware.
    Host => host,
    /// A host's partitions, read.
    Partitions => partitions,
    /// One partition, changed.
    Partition => partition,
    /// Partition profiles.
    Profiles => profiles,
    /// A partition's control panel.
    Panel => panel,
    /// A partition's reference codes.
    Refcodes => refcodes,
    /// A partition's dumps.
    Dumps => dumps,
    /// A partition's console.
    Console => console,
    /// Virtual resources.
    Vres => vres,
}
