// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A compiled printer file as bytes, for the `*PRTF` object: a version
//! byte, then little-endian integers and length-prefixed UTF-8 strings
//! in the model's field order.

use super::model::{Edit, PrtField, PrtFormat, PrtKind, PrtfDefinition, Spacing};

const VERSION: u8 = 1;

#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u16(u16::try_from(s.len()).unwrap_or(u16::MAX));
        self.0
            .extend_from_slice(&s.as_bytes()[..s.len().min(usize::from(u16::MAX))]);
    }
    fn opt(&mut self, s: Option<&String>) {
        self.u8(u8::from(s.is_some()));
        self.str(s.map_or("", String::as_str));
    }
    fn spacing(&mut self, s: Spacing) {
        self.u16(s.skipb);
        self.u16(s.spaceb);
        self.u16(s.spacea);
        self.u16(s.skipa);
    }
}

struct R<'a>(&'a [u8]);

impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let (a, b) = (self.0.get(..n)?, self.0.get(n..)?);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn str(&mut self) -> Option<String> {
        let n = usize::from(self.u16()?);
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
    #[allow(clippy::option_option)] // damaged, or absent
    fn opt(&mut self) -> Option<Option<String>> {
        let some = self.u8()? != 0;
        let s = self.str()?;
        Some(some.then_some(s))
    }
    fn spacing(&mut self) -> Option<Spacing> {
        Some(Spacing {
            skipb: self.u16()?,
            spaceb: self.u16()?,
            spacea: self.u16()?,
            skipa: self.u16()?,
        })
    }
}

/// The bytes of a definition.
#[must_use]
pub fn encode(d: &PrtfDefinition) -> Vec<u8> {
    let mut w = W::default();
    w.u8(VERSION);
    w.u8(u8::from(d.indara));
    w.opt(d.text.as_ref());
    w.u16(u16::try_from(d.formats.len()).unwrap_or(0));
    for f in &d.formats {
        w.str(&f.name);
        w.opt(f.text.as_ref());
        w.spacing(f.spacing);
        w.u16(u16::try_from(f.fields.len()).unwrap_or(0));
        for x in &f.fields {
            w.str(&x.name);
            match &x.kind {
                PrtKind::Char(n) => {
                    w.u8(0);
                    w.u16(*n);
                }
                PrtKind::Zoned(d, s) => {
                    w.u8(1);
                    w.u8(*d);
                    w.u8(*s);
                }
                PrtKind::Constant(t) => {
                    w.u8(2);
                    w.str(t);
                }
                PrtKind::Date => w.u8(3),
                PrtKind::Time => w.u8(4),
                PrtKind::PagNbr => w.u8(5),
            }
            w.u16(x.line);
            w.u16(x.pos);
            w.u8(u8::from(x.relative));
            w.u8(u8::try_from(x.cond.len()).unwrap_or(0));
            for (n, on) in &x.cond {
                w.u8(*n);
                w.u8(u8::from(*on));
            }
            match &x.edit {
                Edit::None => w.u8(0),
                Edit::Code(c, fill) => {
                    w.u8(1);
                    w.str(&c.to_string());
                    w.str(&fill.map(String::from).unwrap_or_default());
                }
                Edit::Word(t) => {
                    w.u8(2);
                    w.str(t);
                }
            }
            w.u8(u8::from(x.underline));
            w.u8(u8::from(x.highlight));
            w.spacing(x.spacing);
        }
    }
    w.0
}

/// A definition back from its bytes; `None` when they are damaged.
#[must_use]
pub fn decode(b: &[u8]) -> Option<PrtfDefinition> {
    let mut r = R(b);
    if r.u8()? != VERSION {
        return None;
    }
    let indara = r.u8()? != 0;
    let text = r.opt()?;
    let mut formats = Vec::new();
    for _ in 0..r.u16()? {
        let name = r.str()?;
        let ftext = r.opt()?;
        let spacing = r.spacing()?;
        let mut fields = Vec::new();
        for _ in 0..r.u16()? {
            let fname = r.str()?;
            let kind = match r.u8()? {
                0 => PrtKind::Char(r.u16()?),
                1 => PrtKind::Zoned(r.u8()?, r.u8()?),
                2 => PrtKind::Constant(r.str()?),
                3 => PrtKind::Date,
                4 => PrtKind::Time,
                5 => PrtKind::PagNbr,
                _ => return None,
            };
            let line = r.u16()?;
            let pos = r.u16()?;
            let relative = r.u8()? != 0;
            let mut cond = Vec::new();
            for _ in 0..r.u8()? {
                cond.push((r.u8()?, r.u8()? != 0));
            }
            let edit = match r.u8()? {
                0 => Edit::None,
                1 => {
                    let c = r.str()?.chars().next()?;
                    Edit::Code(c, r.str()?.chars().next())
                }
                2 => Edit::Word(r.str()?),
                _ => return None,
            };
            fields.push(PrtField {
                name: fname,
                kind,
                line,
                pos,
                relative,
                cond,
                edit,
                underline: r.u8()? != 0,
                highlight: r.u8()? != 0,
                spacing: r.spacing()?,
            });
        }
        formats.push(PrtFormat {
            name,
            text: ftext,
            spacing,
            fields,
        });
    }
    Some(PrtfDefinition {
        indara,
        text,
        formats,
    })
}
