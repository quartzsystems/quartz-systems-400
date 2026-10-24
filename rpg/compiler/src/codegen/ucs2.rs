// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! UCS-2 and graphic data beside character data: the three text
//! classes, conversions between them (the IR's `cvt` through the
//! CCSID), `U'…'` literals, and `%UCS2`, `%GRAPH` and `%CHAR` of
//! UCS-2. An operation over two classes works in the wider one: UCS-2
//! over graphic over character.

use super::Gen;
use super::expr::{V, Vk};
use crate::types::Ty;

/// A text class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextClass {
    /// `char` and `varchar`.
    Char,
    /// Graphic (DBCS).
    Graph,
    /// UCS-2.
    Ucs2,
}

/// The text class of a type, if it is text.
#[must_use]
pub fn class(t: &Ty) -> Option<TextClass> {
    Some(match t {
        Ty::Char(_) | Ty::VarChar(..) => TextClass::Char,
        Ty::Graph(_) | Ty::VarGraph(..) => TextClass::Graph,
        Ty::Ucs2(_) | Ty::VarUcs2(..) => TextClass::Ucs2,
        _ => return None,
    })
}

/// The text type of a class: `n` characters, varying or fixed.
#[must_use]
pub fn text_ty(c: TextClass, n: u32, varying: bool) -> Ty {
    match (c, varying) {
        (TextClass::Char, false) => Ty::Char(n),
        (TextClass::Char, true) => Ty::varchar(n),
        (TextClass::Ucs2, false) => Ty::Ucs2(n),
        (TextClass::Ucs2, true) => Ty::varucs2(n),
        (TextClass::Graph, false) => Ty::Graph(n),
        (TextClass::Graph, true) => Ty::vargraph(n),
    }
}

impl Gen<'_> {
    /// A text value in another class, converted into a temporary of that
    /// class (varying when the value is), the same length in characters.
    pub fn to_class(&mut self, v: &V, to: TextClass) -> Option<V> {
        let Some(from) = class(&v.ty) else {
            return Some(v.clone());
        };
        if from == to {
            return Some(v.clone());
        }
        let n = v.ty.str_len().unwrap_or(1).max(1);
        let ty = text_ty(to, n, v.ty.is_varying());
        // A literal goes through a place of its own type first.
        let src = if v.kind == Vk::Place {
            v.op.clone()
        } else {
            let t = self.b.temp(&self.ir_ty(&v.ty));
            self.b.ins(format!("mov {t}, {}", v.op));
            t
        };
        let t = self.b.temp(&self.ir_ty(&ty));
        self.b.ins(format!("cvt {t}, {src}"));
        Some(V::place(ty, t))
    }

    /// `U'hhhh…'`: four hexadecimal digits a character, in a place.
    pub fn ucs2_literal(&mut self, hex: &str) -> V {
        let n = u32::try_from(hex.len() / 4).unwrap_or(0).max(1);
        let ty = Ty::Ucs2(n);
        let t = self.b.temp(&self.ir_ty(&ty));
        if hex.is_empty() {
            self.b.ins(format!("mov {t}, *BLANKS"));
        } else {
            self.b.ins(format!("mov {t}, x\"{}\"", hex.to_ascii_uppercase()));
        }
        V::place(ty, t)
    }

    /// The class two text operands meet in: the wider.
    #[must_use]
    pub fn meet(a: &V, b: &V) -> Option<TextClass> {
        Some(class(&a.ty)?.max(class(&b.ty)?))
    }
}
