// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RPG data types: their storage sizes (IBM's), their IR types, and
//! the attribute text the cross-reference prints.

/// A date format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum DatFmt {
    /// `*ISO` yyyy-mm-dd.
    #[default]
    Iso,
    /// `*USA` mm/dd/yyyy.
    Usa,
    /// `*EUR` dd.mm.yyyy.
    Eur,
    /// `*JIS` yyyy-mm-dd.
    Jis,
    /// `*MDY` mm/dd/yy.
    Mdy,
    /// `*DMY` dd/mm/yy.
    Dmy,
    /// `*YMD` yy/mm/dd.
    Ymd,
    /// `*JUL` yy/ddd.
    Jul,
}

impl DatFmt {
    /// From `*ISO` etc.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim_start_matches('*') {
            "ISO" => Self::Iso,
            "USA" => Self::Usa,
            "EUR" => Self::Eur,
            "JIS" => Self::Jis,
            "MDY" => Self::Mdy,
            "DMY" => Self::Dmy,
            "YMD" => Self::Ymd,
            "JUL" => Self::Jul,
            _ => return None,
        })
    }

    /// The name with its star.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Iso => "*ISO",
            Self::Usa => "*USA",
            Self::Eur => "*EUR",
            Self::Jis => "*JIS",
            Self::Mdy => "*MDY",
            Self::Dmy => "*DMY",
            Self::Ymd => "*YMD",
            Self::Jul => "*JUL",
        }
    }

    /// The characters a value takes.
    #[must_use]
    pub fn len(self) -> u32 {
        match self {
            Self::Iso | Self::Usa | Self::Eur | Self::Jis => 10,
            Self::Mdy | Self::Dmy | Self::Ymd => 8,
            Self::Jul => 6,
        }
    }

    /// The default separator.
    #[must_use]
    pub fn sep(self) -> char {
        match self {
            Self::Iso | Self::Jis => '-',
            Self::Eur => '.',
            _ => '/',
        }
    }
}

/// A time format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TimFmt {
    /// `*ISO` hh.mm.ss.
    #[default]
    Iso,
    /// `*HMS` hh:mm:ss.
    Hms,
    /// `*USA` hh:mm AM.
    Usa,
    /// `*EUR` hh.mm.ss.
    Eur,
    /// `*JIS` hh:mm:ss.
    Jis,
}

impl TimFmt {
    /// From `*ISO` etc.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim_start_matches('*') {
            "ISO" => Self::Iso,
            "HMS" => Self::Hms,
            "USA" => Self::Usa,
            "EUR" => Self::Eur,
            "JIS" => Self::Jis,
            _ => return None,
        })
    }

    /// The name with its star.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Iso => "*ISO",
            Self::Hms => "*HMS",
            Self::Usa => "*USA",
            Self::Eur => "*EUR",
            Self::Jis => "*JIS",
        }
    }

    /// The default separator.
    #[must_use]
    pub fn sep(self) -> char {
        match self {
            Self::Iso | Self::Eur => '.',
            _ => ':',
        }
    }
}

/// An RPG type.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum Ty {
    /// Not known (after an error).
    #[default]
    Unknown,
    /// `A`: fixed character.
    Char(u32),
    /// `A VARYING`: the maximum length and the prefix bytes (2 or 4).
    VarChar(u32, u8),
    /// `C`: UCS-2.
    Ucs2(u32),
    /// `C VARYING`.
    VarUcs2(u32, u8),
    /// `G`: graphic.
    Graph(u32),
    /// `G VARYING`.
    VarGraph(u32, u8),
    /// `N`.
    Ind,
    /// `P`: digits, decimals.
    Packed(u8, u8),
    /// `S`: digits, decimals.
    Zoned(u8, u8),
    /// `B`: digits, decimals.
    Bin(u8, u8),
    /// `I`: digits (3, 5, 10, 20).
    Int(u8),
    /// `U`: digits (3, 5, 10, 20).
    Uns(u8),
    /// `F`: bytes (4 or 8).
    Float(u8),
    /// `D`.
    Date(DatFmt, char),
    /// `T`.
    Time(TimFmt, char),
    /// `Z`: fractional digits.
    Timestamp(u8),
    /// A duration (`%DAYS(n)`): its unit (`codegen::dates`).
    Dur(u8),
    /// `*`.
    Ptr,
    /// `*` with `PROCPTR`.
    ProcPtr,
    /// `O`.
    Object,
    /// A data structure: its index in the module's structures.
    Ds(usize),
}

impl Ty {
    /// The storage size in bytes; a structure's comes from `ds_len`.
    #[must_use]
    pub fn size(&self, ds_len: &dyn Fn(usize) -> u32) -> u32 {
        match self {
            Self::Unknown => 0,
            Self::Char(n) | Self::Graph(n) => *n * if matches!(self, Self::Graph(_)) { 2 } else { 1 },
            Self::VarChar(n, p) => n + u32::from(*p),
            Self::Ucs2(n) => 2 * n,
            Self::VarUcs2(n, p) | Self::VarGraph(n, p) => 2 * n + u32::from(*p),
            Self::Ind => 1,
            Self::Packed(d, _) => u32::from(*d) / 2 + 1,
            Self::Zoned(d, _) => u32::from(*d),
            Self::Bin(d, _) => {
                if *d <= 4 {
                    2
                } else if *d <= 9 {
                    4
                } else {
                    8
                }
            }
            Self::Int(d) | Self::Uns(d) => match d {
                3 => 1,
                5 => 2,
                10 => 4,
                _ => 8,
            },
            Self::Float(b) => u32::from(*b),
            Self::Date(f, _) => f.len(),
            Self::Time(..) => 8,
            Self::Timestamp(p) => {
                if *p == 0 {
                    19
                } else {
                    20 + u32::from(*p)
                }
            }
            Self::Ptr | Self::ProcPtr | Self::Object => 16,
            Self::Dur(_) => 8,
            Self::Ds(i) => ds_len(*i),
        }
    }

    /// Whether the type is numeric.
    #[must_use]
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Self::Packed(..) | Self::Zoned(..) | Self::Bin(..) | Self::Int(_) | Self::Uns(_) | Self::Float(_)
        )
    }

    /// Whether the type is integer (`I` or `U`).
    #[must_use]
    pub fn is_integer(&self) -> bool {
        matches!(self, Self::Int(_) | Self::Uns(_))
    }

    /// Whether the type is character (fixed or varying).
    #[must_use]
    pub fn is_char(&self) -> bool {
        matches!(self, Self::Char(_) | Self::VarChar(..))
    }

    /// Whether the type is a string type of any class.
    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(
            self,
            Self::Char(_) | Self::VarChar(..) | Self::Ucs2(_) | Self::VarUcs2(..) | Self::Graph(_) | Self::VarGraph(..)
        )
    }

    /// Whether the type is varying.
    #[must_use]
    pub fn is_varying(&self) -> bool {
        matches!(self, Self::VarChar(..) | Self::VarUcs2(..) | Self::VarGraph(..))
    }

    /// Whether the type is a date, time or timestamp.
    #[must_use]
    pub fn is_datetime(&self) -> bool {
        matches!(self, Self::Date(..) | Self::Time(..) | Self::Timestamp(_))
    }

    /// The decimal shape of a numeric type (integers as their digits,
    /// floats as 63 digits for want of better); `None` otherwise.
    #[must_use]
    pub fn shape(&self) -> Option<(u8, u8)> {
        Some(match self {
            Self::Packed(d, s) | Self::Zoned(d, s) | Self::Bin(d, s) => (*d, *s),
            Self::Int(d) | Self::Uns(d) => (*d, 0),
            _ => return None,
        })
    }

    /// The string length of a string type (characters).
    #[must_use]
    pub fn str_len(&self) -> Option<u32> {
        Some(match self {
            Self::Char(n) | Self::VarChar(n, _) | Self::Ucs2(n) | Self::VarUcs2(n, _) | Self::Graph(n) | Self::VarGraph(n, _) => {
                *n
            }
            _ => return None,
        })
    }

    /// The IR type text. A structure's comes from `ds_ir`.
    #[must_use]
    pub fn ir(&self, ds_ir: &dyn Fn(usize) -> String) -> String {
        match self {
            Self::Unknown => "char(1)".into(),
            Self::Char(n) => format!("char({n})"),
            Self::VarChar(n, _) => format!("varchar({n})"),
            Self::Ucs2(n) => format!("ucs2({n})"),
            Self::VarUcs2(n, _) => format!("varucs2({n})"),
            Self::Graph(n) => format!("graphic({n})"),
            Self::VarGraph(n, _) => format!("bytes({})", 2 * n + 2),
            Self::Ind => "ind".into(),
            Self::Packed(d, s) => format!("packed({d},{s})"),
            Self::Zoned(d, s) => format!("zoned({d},{s})"),
            // Binary keeps its decimals: the IR's binary decimal.
            Self::Bin(d, s) => format!("bin({d},{s})"),
            Self::Int(d) => int_ir('i', *d),
            Self::Uns(d) => int_ir('u', *d),
            Self::Float(b) => format!("f{}", u32::from(*b) * 8),
            // Dates are their characters (`codegen::dates`).
            Self::Date(..) | Self::Time(..) | Self::Timestamp(_) => format!("char({})", self.size(&|_| 0)),
            Self::Dur(_) => "i64".into(),
            Self::Ptr | Self::Object => "ptr".into(),
            Self::ProcPtr => "procptr".into(),
            Self::Ds(i) => ds_ir(*i),
        }
    }

    /// The cross-reference attribute: `A(10)`, `P(7,2)`, `N(1)`,
    /// `DS(132)`, ...
    #[must_use]
    pub fn xref(&self, ds_len: &dyn Fn(usize) -> u32) -> String {
        match self {
            Self::Unknown => "**UNDEF**".into(),
            Self::Char(n) => format!("A({n})"),
            Self::VarChar(n, _) => format!("A({n})"),
            Self::Ucs2(n) | Self::VarUcs2(n, _) => format!("C({n})"),
            Self::Graph(n) | Self::VarGraph(n, _) => format!("G({n})"),
            Self::Ind => "N(1)".into(),
            Self::Packed(d, s) => format!("P({d},{s})"),
            Self::Zoned(d, s) => format!("S({d},{s})"),
            Self::Bin(d, s) => format!("B({d},{s})"),
            Self::Int(d) => format!("I({d},0)"),
            Self::Uns(d) => format!("U({d},0)"),
            Self::Float(b) => format!("F({b})"),
            Self::Date(f, s) => format!("D({}{}{s})", f.len(), f.name()),
            Self::Time(f, s) => format!("T(8{}{s})", f.name()),
            Self::Timestamp(p) => format!("Z({})", self.size(&|_| 0).max(19 + u32::from(*p))),
            Self::Ptr | Self::ProcPtr => "*(16)".into(),
            Self::Object => "O(16)".into(),
            Self::Dur(_) => "**UNDEF**".into(),
            Self::Ds(i) => format!("DS({})", ds_len(*i)),
        }
    }
}

fn int_ir(c: char, d: u8) -> String {
    let bits = match d {
        3 => 8,
        5 => 16,
        10 => 32,
        _ => 64,
    };
    format!("{c}{bits}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_ibms() {
        let none = |_| 0;
        assert_eq!(Ty::Packed(7, 2).size(&none), 4);
        assert_eq!(Ty::Packed(8, 0).size(&none), 5);
        assert_eq!(Ty::Zoned(5, 0).size(&none), 5);
        assert_eq!(Ty::Bin(4, 0).size(&none), 2);
        assert_eq!(Ty::Bin(9, 2).size(&none), 4);
        assert_eq!(Ty::Int(10).size(&none), 4);
        assert_eq!(Ty::Uns(3).size(&none), 1);
        assert_eq!(Ty::VarChar(10, 2).size(&none), 12);
        assert_eq!(Ty::Date(DatFmt::Mdy, '/').size(&none), 8);
        assert_eq!(Ty::Timestamp(6).size(&none), 26);
        assert_eq!(Ty::Timestamp(0).size(&none), 19);
        assert_eq!(Ty::Ucs2(5).size(&none), 10);
        assert_eq!(Ty::Packed(7, 2).ir(&|_| String::new()), "packed(7,2)");
        assert_eq!(Ty::Int(20).ir(&|_| String::new()), "i64");
        assert_eq!(Ty::Char(10).xref(&none), "A(10)");
    }
}
