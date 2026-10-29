// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Fixed templates: the bytes between the header and the parameters,
//! declared once as a struct of big-endian fields. Each struct knows its
//! size and its layout (frame offsets), which `docs/HOSTSVR.md` lists.

/// A fixed-size big-endian field.
pub trait Field: Sized + Copy {
    /// Its size in bytes.
    const SIZE: usize;
    /// All zeros.
    fn zero() -> Self;
    /// Read it from exactly `SIZE` bytes.
    fn read(b: &[u8]) -> Self;
    /// Append it.
    fn write(&self, out: &mut Vec<u8>);
}

impl Field for u8 {
    const SIZE: usize = 1;
    fn zero() -> Self {
        0
    }
    fn read(b: &[u8]) -> Self {
        b[0]
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.push(*self);
    }
}

impl Field for u16 {
    const SIZE: usize = 2;
    fn zero() -> Self {
        0
    }
    fn read(b: &[u8]) -> Self {
        u16::from_be_bytes([b[0], b[1]])
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }
}

impl Field for u32 {
    const SIZE: usize = 4;
    fn zero() -> Self {
        0
    }
    fn read(b: &[u8]) -> Self {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.to_be_bytes());
    }
}

impl<const N: usize> Field for [u8; N] {
    const SIZE: usize = N;
    fn zero() -> Self {
        [0; N]
    }
    fn read(b: &[u8]) -> Self {
        let mut a = [0u8; N];
        a.copy_from_slice(&b[..N]);
        a
    }
    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self);
    }
}

/// One field of a layout: frame offset, size, name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    /// Offset from the start of the frame.
    pub offset: usize,
    /// Size in bytes.
    pub size: usize,
    /// The field's name.
    pub name: &'static str,
}

/// What every template struct offers.
pub trait Template: Sized {
    /// Its size in bytes.
    const SIZE: usize;
    /// Its name in the docs.
    const NAME: &'static str;
    /// Its fields at frame offsets.
    fn layout() -> Vec<Slot>;
    /// Decode exactly `SIZE` bytes.
    fn decode(b: &[u8]) -> Option<Self>;
    /// The bytes.
    fn encode(&self) -> Vec<u8>;
}

/// Declare a template struct (layout at frame offsets), or with `base
/// 0;` a fixed record inside a parameter (layout at data offsets).
macro_rules! template {
    ($(#[$m:meta])* pub struct $name:ident { $( $(#[$fm:meta])* $f:ident : $t:ty ),* $(,)? }) => {
        $crate::template::template! { base $crate::frame::HEADER_LEN; $(#[$m])* pub struct $name { $( $(#[$fm])* $f : $t ),* } }
    };
    (base $base:expr; $(#[$m:meta])* pub struct $name:ident { $( $(#[$fm:meta])* $f:ident : $t:ty ),* $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name { $( $(#[$fm])* pub $f: $t ),* }

        impl Default for $name {
            fn default() -> Self {
                Self { $( $f: <$t as $crate::template::Field>::zero() ),* }
            }
        }

        impl $crate::template::Template for $name {
            const SIZE: usize = 0 $( + <$t as $crate::template::Field>::SIZE )*;
            const NAME: &'static str = stringify!($name);
            #[allow(unused_mut, unused_variables, unused_assignments)]
            fn layout() -> Vec<$crate::template::Slot> {
                let mut at = $base;
                let mut v = Vec::new();
                $(
                    let size = <$t as $crate::template::Field>::SIZE;
                    v.push($crate::template::Slot { offset: at, size, name: stringify!($f) });
                    at += size;
                )*
                let _ = at;
                v
            }
            #[allow(unused_mut, unused_variables, unused_assignments)]
            fn decode(b: &[u8]) -> Option<Self> {
                if b.len() != Self::SIZE {
                    return None;
                }
                let mut at = 0;
                $(
                    let size = <$t as $crate::template::Field>::SIZE;
                    let $f = <$t as $crate::template::Field>::read(&b[at..at + size]);
                    at += size;
                )*
                Some(Self { $( $f ),* })
            }
            #[allow(unused_mut)]
            fn encode(&self) -> Vec<u8> {
                let mut out = Vec::with_capacity(Self::SIZE);
                $( $crate::template::Field::write(&self.$f, &mut out); )*
                out
            }
        }
    };
}

pub(crate) use template;

/// Declare a server's template enum: each variant is a template struct
/// chosen by direction and request or reply ID; anything else (or a
/// template of an unexpected size) stays `Other`.
macro_rules! templates {
    ($(#[$m:meta])* pub enum $name:ident { $( $(#[$vm:meta])* $var:ident ( $ty:ty ) = $dir:ident $id:path ),* $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub enum $name {
            $( $(#[$vm])* $var($ty), )*
            /// A template this codec does not type (kept as bytes).
            Other(Vec<u8>),
        }

        impl $name {
            /// Decode a template by direction and ID.
            pub fn decode(dir: $crate::Dir, id: u16, b: &[u8]) -> Self {
                $(
                    if dir == $crate::Dir::$dir && id == $id {
                        if let Some(t) = <$ty as $crate::template::Template>::decode(b) {
                            return Self::$var(t);
                        }
                    }
                )*
                let _ = (dir, id);
                Self::Other(b.to_vec())
            }

            /// Whether `decode` would type this direction and ID.
            pub fn knows(dir: $crate::Dir, id: u16) -> bool {
                $( if dir == $crate::Dir::$dir && id == $id { return true; } )*
                let _ = (dir, id);
                false
            }

            /// The bytes.
            pub fn encode(&self) -> Vec<u8> {
                match self {
                    $( Self::$var(t) => $crate::template::Template::encode(t), )*
                    Self::Other(b) => b.clone(),
                }
            }

            /// Whether the template was typed.
            pub fn is_typed(&self) -> bool {
                !matches!(self, Self::Other(_))
            }

            /// Every typed template: its direction, ID, name and layout.
            pub fn layouts() -> Vec<($crate::Dir, u16, &'static str, Vec<$crate::template::Slot>)> {
                vec![ $( (
                    $crate::Dir::$dir,
                    $id,
                    <$ty as $crate::template::Template>::NAME,
                    <$ty as $crate::template::Template>::layout(),
                ), )* ]
            }
        }
    };
}

pub(crate) use templates;
