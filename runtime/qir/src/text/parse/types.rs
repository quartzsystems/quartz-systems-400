// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Type syntax.

use super::Parser;
use crate::error::Result;
use crate::text::lex::Tok;
use crate::types::{DateFmt, Shape, StructType, TimeFmt, Type};

impl Parser {
    /// A type.
    pub(crate) fn ty(&mut self) -> Result<Type> {
        let word = self.ident()?;
        self.ty_from_word(&word)
    }

    /// A type whose first word has been read. The word may carry
    /// trailing field names (`Rec.amt` lexes as one word); the caller
    /// that allows them uses [`Parser::ty_and_fields`].
    pub(crate) fn ty_from_word(&mut self, word: &str) -> Result<Type> {
        let t = match word {
            "i8" => Type::I8,
            "i16" => Type::I16,
            "i32" => Type::I32,
            "i64" => Type::I64,
            "u8" => Type::U8,
            "u16" => Type::U16,
            "u32" => Type::U32,
            "u64" => Type::U64,
            "f32" => Type::F32,
            "f64" => Type::F64,
            "ind" => Type::Ind,
            "ptr" => Type::Ptr,
            "procptr" => Type::ProcPtr,
            "packed" => Type::Packed(self.shape()?),
            "zoned" => Type::Zoned(self.shape()?),
            "bin" => Type::Bin(self.shape()?),
            "char" => Type::Char(self.paren(Self::u32)?),
            "varchar" => Type::VarChar(self.paren(Self::u32)?),
            "ucs2" => Type::Ucs2(self.paren(Self::u32)?),
            "varucs2" => Type::VarUcs2(self.paren(Self::u32)?),
            "graphic" => Type::Graphic(self.paren(Self::u32)?),
            "bytes" => Type::Bytes(self.paren(Self::u32)?),
            "timestamp" => {
                let p = self.paren(Self::u32)?;
                Type::Timestamp(u8::try_from(p).map_err(|_| self.err("precision is 0 to 12"))?)
            }
            "date" => {
                let w = self.paren(Self::special_word)?;
                Type::Date(
                    DateFmt::from_word(&format!("*{w}"))
                        .ok_or_else(|| self.err(format!("*{w} is not a date format")))?,
                )
            }
            "time" => {
                let w = self.paren(Self::special_word)?;
                Type::Time(
                    TimeFmt::from_word(&format!("*{w}"))
                        .ok_or_else(|| self.err(format!("*{w} is not a time format")))?,
                )
            }
            "array" => {
                self.expect_sym("[")?;
                let len = self.u32()?;
                self.expect_sym("]")?;
                self.expect_word("of")?;
                let elem = self.ty()?;
                Type::Array {
                    len,
                    elem: Box::new(elem),
                }
            }
            "struct" => self.struct_type()?,
            name => self
                .m
                .type_named(name)
                .cloned()
                .ok_or_else(|| self.err(format!("{name} is not a type")))?,
        };
        t.check().map_err(|e| self.err(e.to_string()))?;
        Ok(t)
    }

    /// A type followed by `.field` steps that the lexer folded into the
    /// type's word. Returns the type and the field names.
    pub(crate) fn ty_and_fields(&mut self) -> Result<(Type, Vec<String>)> {
        let word = self.ident()?;
        let (head, rest) = match word.split_once('.') {
            Some((h, r)) => (h.to_string(), r.split('.').map(str::to_string).collect()),
            None => (word, Vec::new()),
        };
        let t = self.ty_from_word(&head)?;
        Ok((t, rest))
    }

    /// A `*WORD` without its star.
    pub(crate) fn special_word(&mut self) -> Result<String> {
        match self.peek() {
            Tok::Special(s) => {
                let s = s.clone();
                self.bump();
                Ok(s)
            }
            _ => Err(self.unexpected("a *WORD")),
        }
    }

    fn shape(&mut self) -> Result<Shape> {
        self.paren(|p| {
            let d = p.u32()?;
            p.expect_sym(",")?;
            let s = p.u32()?;
            let d = u8::try_from(d).map_err(|_| p.err("digits are 1 to 63"))?;
            let s = u8::try_from(s).map_err(|_| p.err("decimal positions are 0 to 63"))?;
            Shape::new(d, s).map_err(|e| p.err(e.to_string()))
        })
    }

    fn struct_type(&mut self) -> Result<Type> {
        let size = if self.is_sym("(") {
            Some(self.paren(Self::u32)?)
        } else {
            None
        };
        self.expect_sym("{")?;
        let mut fields = Vec::new();
        loop {
            self.skip_newlines();
            if self.eat_sym("}") {
                break;
            }
            let name = self.ident()?;
            self.expect_sym(":")?;
            let ty = self.ty()?;
            let offset = if self.eat_sym("@") {
                Some(self.u32()?)
            } else {
                None
            };
            fields.push((name, offset, ty));
            self.skip_newlines();
            if !self.eat_sym(",") {
                self.skip_newlines();
                self.expect_sym("}")?;
                break;
            }
        }
        let s = StructType::new(fields, size).map_err(|e| self.err(e.to_string()))?;
        Ok(Type::Struct(Box::new(s)))
    }
}
