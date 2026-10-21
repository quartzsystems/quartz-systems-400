// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `wire_struct!` and `wire_enum!`.

/// Implement [`Wire`](crate::Wire) for a struct with named fields, in
/// the order given: `wire_struct!(Point { x, y });`.
#[macro_export]
macro_rules! wire_struct {
    ($t:ty { $($f:ident),* $(,)? }) => {
        impl $crate::Wire for $t {
            fn put(&self, out: &mut Vec<u8>) {
                $( $crate::Wire::put(&self.$f, out); )*
                let _ = out;
            }
            fn get(d: &mut $crate::Dec<'_>) -> Result<Self, $crate::WireError> {
                let _ = &d;
                Ok(Self { $( $f: $crate::Wire::get(d)?, )* })
            }
        }
    };
}

/// Implement [`Wire`](crate::Wire) for an enum of unit and struct-like
/// variants, each with a fixed tag:
/// `wire_enum!(Shape { 0 => Empty, 1 => Dot { at } });`.
#[macro_export]
macro_rules! wire_enum {
    ($t:ident { $($tag:literal => $v:ident $({ $($f:ident),* $(,)? })?),* $(,)? }) => {
        impl $crate::Wire for $t {
            fn put(&self, out: &mut Vec<u8>) {
                match self {
                    $( $t::$v $({ $($f),* })? => {
                        out.push($tag);
                        $($( $crate::Wire::put($f, out); )*)?
                    } )*
                }
            }
            fn get(d: &mut $crate::Dec<'_>) -> Result<Self, $crate::WireError> {
                let tag = <u8 as $crate::Wire>::get(d)?;
                match tag {
                    $( $tag => Ok($t::$v $({ $($f: $crate::Wire::get(d)?),* })?), )*
                    tag => Err($crate::WireError::Tag { what: stringify!($t), tag }),
                }
            }
        }
    };
}
