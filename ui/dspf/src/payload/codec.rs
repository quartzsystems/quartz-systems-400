// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The model as bytes: little-endian integers, length-prefixed UTF-8
//! strings, one tag byte per enum, counts before lists, in the model's
//! field order. The source is not here; the payload keeps it as fixed
//! records after this section.

use dds::model::{CompOp, DateFmt, FileRef, Literal, TimeFmt, Validity};
use dds::{Parm, Source};
use qsname::Name;

use crate::cond::{Cond, Ind};
use crate::model::{
    Attr, Buffer, BufferField, Clrl, CmdKey, Constant, DisplayFile, DisplaySize, Edit, ErrMsg,
    Field, FieldType, FileAttrs, KeyName, KeywordUse, Position, Record, RecordKind, RecordLayout,
    SflEnd, SflRole, SflSize, Slno, Subfile, TypeClass, Usage, Window, WindowOrigin,
};

#[derive(Default)]
struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn usize(&mut self, v: usize) {
        self.u32(u32::try_from(v).unwrap_or(u32::MAX));
    }
    fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }
    fn bytes(&mut self, b: &[u8]) {
        self.usize(b.len());
        self.0.extend_from_slice(b);
    }
    fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
    fn opt_str(&mut self, s: Option<&str>) {
        self.bool(s.is_some());
        if let Some(s) = s {
            self.str(s);
        }
    }
    fn opt_u8(&mut self, v: Option<u8>) {
        self.bool(v.is_some());
        self.u8(v.unwrap_or(0));
    }
    fn opt_u16(&mut self, v: Option<u16>) {
        self.bool(v.is_some());
        self.u16(v.unwrap_or(0));
    }
    fn count(&mut self, n: usize) {
        self.usize(n);
    }
    fn ind(&mut self, i: Ind) {
        self.u8(i.number);
        self.bool(i.on);
    }
    fn cond(&mut self, c: &Cond) {
        self.opt_u8(c.size);
        self.count(c.any.len());
        for g in &c.any {
            self.count(g.len());
            for i in g {
                self.ind(*i);
            }
        }
    }
    fn parm(&mut self, p: &Parm) {
        match p {
            Parm::Word(w) => {
                self.u8(0);
                self.str(w);
            }
            Parm::Quoted(q) => {
                self.u8(1);
                self.str(q);
            }
            Parm::Hex(h) => {
                self.u8(2);
                self.bytes(h);
            }
            Parm::Group(g) => {
                self.u8(3);
                self.count(g.len());
                for p in g {
                    self.parm(p);
                }
            }
        }
    }
    fn keyword(&mut self, k: &KeywordUse) {
        self.str(&k.name);
        self.count(k.parms.len());
        for p in &k.parms {
            self.parm(p);
        }
        self.cond(&k.cond);
        self.usize(k.line);
    }
    fn keywords(&mut self, v: &[KeywordUse]) {
        self.count(v.len());
        for k in v {
            self.keyword(k);
        }
    }
    fn key_name(&mut self, k: KeyName) {
        let (tag, n) = match k {
            KeyName::Ca(n) => (0, n),
            KeyName::Cf(n) => (1, n),
            KeyName::RollUp => (2, 0),
            KeyName::RollDown => (3, 0),
            KeyName::PageUp => (4, 0),
            KeyName::PageDown => (5, 0),
            KeyName::Clear => (6, 0),
            KeyName::Home => (7, 0),
            KeyName::Help => (8, 0),
            KeyName::Print => (9, 0),
            KeyName::VldCmdKey => (10, 0),
        };
        self.u8(tag);
        self.u8(n);
    }
    fn cmd_key(&mut self, k: &CmdKey) {
        self.key_name(k.key);
        self.opt_u8(k.ind);
        self.opt_str(k.text.as_deref());
        self.cond(&k.cond);
        self.usize(k.line);
    }
    fn keys(&mut self, v: &[CmdKey]) {
        self.count(v.len());
        for k in v {
            self.cmd_key(k);
        }
    }
    fn responses(&mut self, v: &[(u8, usize)]) {
        self.count(v.len());
        for (i, l) in v {
            self.u8(*i);
            self.usize(*l);
        }
    }
    fn file_ref(&mut self, r: &FileRef) {
        self.str(&r.library.to_string());
        self.str(&r.file.to_string());
        self.opt_str(r.format.as_ref().map(ToString::to_string).as_deref());
    }
    fn literal(&mut self, l: &Literal) {
        match l {
            Literal::Word(w) => {
                self.u8(0);
                self.str(w);
            }
            Literal::Quoted(q) => {
                self.u8(1);
                self.str(q);
            }
            Literal::Hex(h) => {
                self.u8(2);
                self.bytes(h);
            }
        }
    }
    fn attr(&mut self, a: &Attr) {
        self.count(a.values.len());
        for v in &a.values {
            self.str(v);
        }
        self.cond(&a.cond);
        self.usize(a.line);
    }
    fn attrs(&mut self, v: &[Attr]) {
        self.count(v.len());
        for a in v {
            self.attr(a);
        }
    }
    fn buffer(&mut self, b: &Buffer) {
        self.bytes(&b.indicators);
        self.count(b.fields.len());
        for f in &b.fields {
            self.usize(f.field);
            self.u32(f.offset);
            self.u32(f.len);
        }
        self.u32(b.len);
    }
}

fn write_file(w: &mut W, f: &FileAttrs) {
    w.count(f.sizes.len());
    for s in &f.sizes {
        w.u8(s.rows);
        w.u8(s.cols);
        w.str(&s.name);
    }
    w.bool(f.indara);
    w.count(f.indtxt.len());
    for (i, t) in &f.indtxt {
        w.u8(*i);
        w.str(t);
    }
    w.bool(f.chginpdft.is_some());
    if let Some(v) = &f.chginpdft {
        w.count(v.len());
        for s in v {
            w.str(s);
        }
    }
    w.bool(f.ref_file.is_some());
    if let Some(r) = &f.ref_file {
        w.file_ref(r);
    }
    w.opt_u8(f.msgloc);
    for b in [f.rstdsp, f.usrdspmgt, f.csrinponly, f.dsprl, f.errsfl] {
        w.bool(b);
    }
    for k in [f.alt_help, f.alt_pageup, f.alt_pagedwn] {
        w.bool(k.is_some());
        if let Some(k) = k {
            w.key_name(k);
        }
    }
    w.keys(&f.keys);
    w.opt_u16(f.ccsid);
    w.responses(&f.responses);
    w.keywords(&f.keywords);
}

fn write_record(w: &mut W, r: &Record) {
    w.str(&r.name);
    w.opt_str(r.text.as_deref());
    w.u8(match r.kind {
        RecordKind::Normal => 0,
        RecordKind::Subfile => 1,
        RecordKind::SubfileControl => 2,
        RecordKind::MenuBar => 3,
        RecordKind::PullDown => 4,
    });
    w.bool(r.subfile.is_some());
    if let Some(s) = &r.subfile {
        w.str(&s.record);
        w.count(s.sizes.len());
        for z in &s.sizes {
            w.u16(z.size);
            w.u16(z.page);
        }
        w.opt_u16(s.lin);
        w.opt_u8(s.end.map(|e| match e {
            SflEnd::Plus => 0,
            SflEnd::More => 1,
            SflEnd::ScrBar => 2,
            SflEnd::Text => 3,
        }));
        w.bool(s.message);
        w.u8(s.record_lines);
        w.bytes(&s.first_line);
    }
    w.bool(r.window.is_some());
    if let Some(win) = &r.window {
        match &win.origin {
            WindowOrigin::Default => w.u8(0),
            WindowOrigin::At(a, b) => {
                w.u8(1);
                w.u8(*a);
                w.u8(*b);
            }
            WindowOrigin::Vars(a, b) => {
                w.u8(2);
                w.str(a);
                w.str(b);
            }
            WindowOrigin::From(n) => {
                w.u8(3);
                w.str(n);
            }
        }
        w.u8(win.lines);
        w.u8(win.cols);
    }
    w.keys(&r.keys);
    w.bool(r.clrl.is_some());
    if let Some(c) = r.clrl {
        let (tag, n) = match c {
            Clrl::Lines(n) => (0, n),
            Clrl::No => (1, 0),
            Clrl::End => (2, 0),
            Clrl::All => (3, 0),
        };
        w.u8(tag);
        w.u8(n);
    }
    w.bool(r.slno.is_some());
    if let Some(s) = r.slno {
        let (tag, n) = match s {
            Slno::Line(n) => (0, n),
            Slno::Var => (1, 0),
        };
        w.u8(tag);
        w.u8(n);
    }
    w.opt_u8(r.msgrcd);
    w.responses(&r.responses);
    w.count(r.fields.len());
    for f in &r.fields {
        write_field(w, f);
    }
    w.buffer(&r.layout.output);
    w.buffer(&r.layout.input);
    w.bytes(&r.layout.option_inds);
    w.bytes(&r.layout.response_inds);
    w.str(&r.layout.level_id);
    w.keywords(&r.keywords);
    w.usize(r.line);
}

#[allow(clippy::too_many_lines)] // one line per model field
fn write_field(w: &mut W, f: &Field) {
    w.opt_str(f.name.as_deref());
    w.bool(f.constant.is_some());
    if let Some(c) = &f.constant {
        match c {
            Constant::Text(t) => {
                w.u8(0);
                w.str(t);
            }
            Constant::Date { sys, year4 } => {
                w.u8(1);
                w.bool(*sys);
                w.bool(*year4);
            }
            Constant::Time { sys } => {
                w.u8(2);
                w.bool(*sys);
            }
            Constant::User => w.u8(3),
            Constant::SysName => w.u8(4),
            Constant::Message { len, id, file } => {
                w.u8(5);
                w.u32(*len);
                w.str(id);
                w.file_ref(file);
            }
        }
    }
    w.u8(u8::try_from(u32::from(f.ty.letter)).unwrap_or(b'A'));
    w.u8(match f.ty.class {
        TypeClass::Char => 0,
        TypeClass::Graphic => 1,
        TypeClass::Zoned => 2,
        TypeClass::Packed => 3,
        TypeClass::Binary => 4,
        TypeClass::Float => 5,
        TypeClass::Date => 6,
        TypeClass::Time => 7,
        TypeClass::Timestamp => 8,
    });
    w.u32(f.length);
    w.u8(f.decimals);
    w.u8(u8::try_from(u32::from(f.usage.letter())).unwrap_or(b'O'));
    w.count(f.positions.len());
    for p in &f.positions {
        w.u8(p.row);
        w.u8(p.col);
    }
    w.bool(f.relative);
    w.u32(f.screen_len);
    w.u32(f.bytes);
    w.cond(&f.cond);
    w.attrs(&f.dspatr);
    w.attrs(&f.color);
    match &f.edit {
        Edit::None => w.u8(0),
        Edit::Code(c, m) => {
            w.u8(1);
            w.str(&c.to_string());
            w.opt_str(m.as_deref());
        }
        Edit::Word(t) => {
            w.u8(2);
            w.str(t);
        }
    }
    w.count(f.check.len());
    for c in &f.check {
        w.str(c);
    }
    w.count(f.validity.len());
    for v in &f.validity {
        match v {
            Validity::Check(c) => {
                w.u8(0);
                w.count(c.len());
                for s in c {
                    w.str(s);
                }
            }
            Validity::Values(v) => {
                w.u8(1);
                w.count(v.len());
                for l in v {
                    w.literal(l);
                }
            }
            Validity::Range(a, b) => {
                w.u8(2);
                w.literal(a);
                w.literal(b);
            }
            Validity::Comp(op, l) => {
                w.u8(3);
                w.str(op.name());
                w.literal(l);
            }
        }
    }
    w.count(f.errmsg.len());
    for e in &f.errmsg {
        w.opt_str(e.text.as_deref());
        w.bool(e.id.is_some());
        if let Some((id, file)) = &e.id {
            w.str(id);
            w.file_ref(file);
        }
        w.opt_u8(e.ind);
        w.cond(&e.cond);
        w.usize(e.line);
    }
    w.opt_str(f.dft.as_deref());
    w.opt_str(f.text.as_deref());
    w.bool(f.reference.is_some());
    if let Some((field, file)) = &f.reference {
        w.str(field);
        w.bool(file.is_some());
        if let Some(fr) = file {
            w.file_ref(fr);
        }
    }
    w.opt_str(f.date_fmt.map(DateFmt::name));
    w.opt_str(f.date_sep.map(String::from).as_deref());
    w.opt_str(f.time_fmt.map(TimeFmt::name));
    w.opt_str(f.time_sep.map(String::from).as_deref());
    w.bool(f.sfl_role.is_some());
    if let Some(r) = &f.sfl_role {
        match r {
            SflRole::RcdNbr { cursor, top } => {
                w.u8(0);
                w.bool(*cursor);
                w.bool(*top);
            }
            SflRole::PgmQ => w.u8(1),
            SflRole::MsgKey => w.u8(2),
            SflRole::RolVal => w.u8(3),
        }
    }
    w.responses(&f.responses);
    w.keywords(&f.keywords);
    w.usize(f.line);
}

/// The model section.
pub(super) fn write(df: &DisplayFile) -> Vec<u8> {
    let mut w = W::default();
    write_file(&mut w, &df.file);
    w.count(df.records.len());
    for r in &df.records {
        write_record(&mut w, r);
    }
    w.0
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
    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn usize(&mut self) -> Option<usize> {
        self.u32().map(|n| n as usize)
    }
    fn bool(&mut self) -> Option<bool> {
        self.u8().map(|b| b != 0)
    }
    /// A count, refused when it cannot fit in the bytes left.
    fn count(&mut self) -> Option<usize> {
        let n = self.usize()?;
        (n <= self.0.len()).then_some(n)
    }
    fn bytes(&mut self) -> Option<Vec<u8>> {
        let n = self.count()?;
        self.take(n).map(<[u8]>::to_vec)
    }
    fn str(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?).ok()
    }
    #[allow(clippy::option_option)] // damaged, or absent
    fn opt_str(&mut self) -> Option<Option<String>> {
        if self.bool()? {
            Some(Some(self.str()?))
        } else {
            Some(None)
        }
    }
    #[allow(clippy::option_option)]
    fn opt_u8(&mut self) -> Option<Option<u8>> {
        let some = self.bool()?;
        let v = self.u8()?;
        Some(some.then_some(v))
    }
    #[allow(clippy::option_option)]
    fn opt_u16(&mut self) -> Option<Option<u16>> {
        let some = self.bool()?;
        let v = self.u16()?;
        Some(some.then_some(v))
    }
    fn cond(&mut self) -> Option<Cond> {
        let size = self.opt_u8()?;
        let mut any = Vec::new();
        for _ in 0..self.count()? {
            let mut g = Vec::new();
            for _ in 0..self.count()? {
                let number = self.u8()?;
                let on = self.bool()?;
                g.push(Ind { number, on });
            }
            any.push(g);
        }
        Some(Cond { size, any })
    }
    fn parm(&mut self) -> Option<Parm> {
        Some(match self.u8()? {
            0 => Parm::Word(self.str()?),
            1 => Parm::Quoted(self.str()?),
            2 => Parm::Hex(self.bytes()?),
            3 => {
                let mut g = Vec::new();
                for _ in 0..self.count()? {
                    g.push(self.parm()?);
                }
                Parm::Group(g)
            }
            _ => return None,
        })
    }
    fn keyword(&mut self) -> Option<KeywordUse> {
        let name = self.str()?;
        let mut parms = Vec::new();
        for _ in 0..self.count()? {
            parms.push(self.parm()?);
        }
        Some(KeywordUse {
            name,
            parms,
            cond: self.cond()?,
            line: self.usize()?,
        })
    }
    fn keywords(&mut self) -> Option<Vec<KeywordUse>> {
        let mut v = Vec::new();
        for _ in 0..self.count()? {
            v.push(self.keyword()?);
        }
        Some(v)
    }
    fn key_name(&mut self) -> Option<KeyName> {
        let tag = self.u8()?;
        let n = self.u8()?;
        Some(match tag {
            0 => KeyName::Ca(n),
            1 => KeyName::Cf(n),
            2 => KeyName::RollUp,
            3 => KeyName::RollDown,
            4 => KeyName::PageUp,
            5 => KeyName::PageDown,
            6 => KeyName::Clear,
            7 => KeyName::Home,
            8 => KeyName::Help,
            9 => KeyName::Print,
            10 => KeyName::VldCmdKey,
            _ => return None,
        })
    }
    fn keys(&mut self) -> Option<Vec<CmdKey>> {
        let mut v = Vec::new();
        for _ in 0..self.count()? {
            v.push(CmdKey {
                key: self.key_name()?,
                ind: self.opt_u8()?,
                text: self.opt_str()?,
                cond: self.cond()?,
                line: self.usize()?,
            });
        }
        Some(v)
    }
    fn responses(&mut self) -> Option<Vec<(u8, usize)>> {
        let mut v = Vec::new();
        for _ in 0..self.count()? {
            let i = self.u8()?;
            let l = self.usize()?;
            v.push((i, l));
        }
        Some(v)
    }
    fn file_ref(&mut self) -> Option<FileRef> {
        let lib = self.str()?;
        let file = self.str()?;
        let fmt = self.opt_str()?;
        let mut r = FileRef::parse(&format!("{lib}/{file}")).ok()?;
        if let Some(f) = fmt {
            r = r.with_format(Name::parse(&f).ok()?);
        }
        Some(r)
    }
    fn literal(&mut self) -> Option<Literal> {
        Some(match self.u8()? {
            0 => Literal::Word(self.str()?),
            1 => Literal::Quoted(self.str()?),
            2 => Literal::Hex(self.bytes()?),
            _ => return None,
        })
    }
    fn attrs(&mut self) -> Option<Vec<Attr>> {
        let mut v = Vec::new();
        for _ in 0..self.count()? {
            let mut values = Vec::new();
            for _ in 0..self.count()? {
                values.push(self.str()?);
            }
            v.push(Attr {
                values,
                cond: self.cond()?,
                line: self.usize()?,
            });
        }
        Some(v)
    }
    fn buffer(&mut self) -> Option<Buffer> {
        let indicators = self.bytes()?;
        let mut fields = Vec::new();
        for _ in 0..self.count()? {
            fields.push(BufferField {
                field: self.usize()?,
                offset: self.u32()?,
                len: self.u32()?,
            });
        }
        Some(Buffer {
            indicators,
            fields,
            len: self.u32()?,
        })
    }
    fn char(&mut self) -> Option<char> {
        self.str()?.chars().next()
    }
}

fn read_file(r: &mut R<'_>) -> Option<FileAttrs> {
    let mut sizes = Vec::new();
    for _ in 0..r.count()? {
        sizes.push(DisplaySize {
            rows: r.u8()?,
            cols: r.u8()?,
            name: r.str()?,
        });
    }
    if sizes.is_empty() {
        return None;
    }
    let indara = r.bool()?;
    let mut indtxt = Vec::new();
    for _ in 0..r.count()? {
        let i = r.u8()?;
        let t = r.str()?;
        indtxt.push((i, t));
    }
    let chginpdft = if r.bool()? {
        let mut v = Vec::new();
        for _ in 0..r.count()? {
            v.push(r.str()?);
        }
        Some(v)
    } else {
        None
    };
    let ref_file = if r.bool()? { Some(r.file_ref()?) } else { None };
    let msgloc = r.opt_u8()?;
    let rstdsp = r.bool()?;
    let usrdspmgt = r.bool()?;
    let csrinponly = r.bool()?;
    let dsprl = r.bool()?;
    let errsfl = r.bool()?;
    let mut alts = [None, None, None];
    for a in &mut alts {
        if r.bool()? {
            *a = Some(r.key_name()?);
        }
    }
    Some(FileAttrs {
        sizes,
        indara,
        indtxt,
        chginpdft,
        ref_file,
        msgloc,
        rstdsp,
        usrdspmgt,
        csrinponly,
        dsprl,
        errsfl,
        alt_help: alts[0],
        alt_pageup: alts[1],
        alt_pagedwn: alts[2],
        keys: r.keys()?,
        ccsid: r.opt_u16()?,
        responses: r.responses()?,
        keywords: r.keywords()?,
    })
}

#[allow(clippy::too_many_lines)] // one line per model field
fn read_record(r: &mut R<'_>) -> Option<Record> {
    let name = r.str()?;
    let text = r.opt_str()?;
    let kind = match r.u8()? {
        0 => RecordKind::Normal,
        1 => RecordKind::Subfile,
        2 => RecordKind::SubfileControl,
        3 => RecordKind::MenuBar,
        4 => RecordKind::PullDown,
        _ => return None,
    };
    let subfile = if r.bool()? {
        let record = r.str()?;
        let mut sizes = Vec::new();
        for _ in 0..r.count()? {
            sizes.push(SflSize {
                size: r.u16()?,
                page: r.u16()?,
            });
        }
        let lin = r.opt_u16()?;
        let end = match r.opt_u8()? {
            None => None,
            Some(0) => Some(SflEnd::Plus),
            Some(1) => Some(SflEnd::More),
            Some(2) => Some(SflEnd::ScrBar),
            Some(3) => Some(SflEnd::Text),
            Some(_) => return None,
        };
        Some(Subfile {
            record,
            sizes,
            lin,
            end,
            message: r.bool()?,
            record_lines: r.u8()?,
            first_line: r.bytes()?,
        })
    } else {
        None
    };
    let window = if r.bool()? {
        let origin = match r.u8()? {
            0 => WindowOrigin::Default,
            1 => WindowOrigin::At(r.u8()?, r.u8()?),
            2 => WindowOrigin::Vars(r.str()?, r.str()?),
            3 => WindowOrigin::From(r.str()?),
            _ => return None,
        };
        Some(Window {
            origin,
            lines: r.u8()?,
            cols: r.u8()?,
        })
    } else {
        None
    };
    let keys = r.keys()?;
    let clrl = if r.bool()? {
        let tag = r.u8()?;
        let n = r.u8()?;
        Some(match tag {
            0 => Clrl::Lines(n),
            1 => Clrl::No,
            2 => Clrl::End,
            3 => Clrl::All,
            _ => return None,
        })
    } else {
        None
    };
    let slno = if r.bool()? {
        let tag = r.u8()?;
        let n = r.u8()?;
        Some(match tag {
            0 => Slno::Line(n),
            1 => Slno::Var,
            _ => return None,
        })
    } else {
        None
    };
    let msgrcd = r.opt_u8()?;
    let responses = r.responses()?;
    let mut fields = Vec::new();
    for _ in 0..r.count()? {
        fields.push(read_field(r)?);
    }
    let layout = RecordLayout {
        output: r.buffer()?,
        input: r.buffer()?,
        option_inds: r.bytes()?,
        response_inds: r.bytes()?,
        level_id: r.str()?,
    };
    Some(Record {
        name,
        text,
        kind,
        subfile,
        window,
        keys,
        clrl,
        slno,
        msgrcd,
        responses,
        fields,
        layout,
        keywords: r.keywords()?,
        line: r.usize()?,
    })
}

#[allow(clippy::too_many_lines)] // one line per model field
fn read_field(r: &mut R<'_>) -> Option<Field> {
    let name = r.opt_str()?;
    let constant = if r.bool()? {
        Some(match r.u8()? {
            0 => Constant::Text(r.str()?),
            1 => Constant::Date {
                sys: r.bool()?,
                year4: r.bool()?,
            },
            2 => Constant::Time { sys: r.bool()? },
            3 => Constant::User,
            4 => Constant::SysName,
            5 => Constant::Message {
                len: r.u32()?,
                id: r.str()?,
                file: r.file_ref()?,
            },
            _ => return None,
        })
    } else {
        None
    };
    let letter = char::from(r.u8()?);
    let class = match r.u8()? {
        0 => TypeClass::Char,
        1 => TypeClass::Graphic,
        2 => TypeClass::Zoned,
        3 => TypeClass::Packed,
        4 => TypeClass::Binary,
        5 => TypeClass::Float,
        6 => TypeClass::Date,
        7 => TypeClass::Time,
        8 => TypeClass::Timestamp,
        _ => return None,
    };
    let length = r.u32()?;
    let decimals = r.u8()?;
    let usage = Usage::from_letter(Some(char::from(r.u8()?)))?;
    let mut f = Field::new(name, FieldType { letter, class }, length, decimals, usage);
    f.constant = constant;
    for _ in 0..r.count()? {
        f.positions.push(Position {
            row: r.u8()?,
            col: r.u8()?,
        });
    }
    f.relative = r.bool()?;
    f.screen_len = r.u32()?;
    f.bytes = r.u32()?;
    f.cond = r.cond()?;
    f.dspatr = r.attrs()?;
    f.color = r.attrs()?;
    f.edit = match r.u8()? {
        0 => Edit::None,
        1 => Edit::Code(r.char()?, r.opt_str()?),
        2 => Edit::Word(r.str()?),
        _ => return None,
    };
    for _ in 0..r.count()? {
        f.check.push(r.str()?);
    }
    for _ in 0..r.count()? {
        f.validity.push(match r.u8()? {
            0 => {
                let mut v = Vec::new();
                for _ in 0..r.count()? {
                    v.push(r.str()?);
                }
                Validity::Check(v)
            }
            1 => {
                let mut v = Vec::new();
                for _ in 0..r.count()? {
                    v.push(r.literal()?);
                }
                Validity::Values(v)
            }
            2 => Validity::Range(r.literal()?, r.literal()?),
            3 => Validity::Comp(CompOp::from_word(&r.str()?)?, r.literal()?),
            _ => return None,
        });
    }
    for _ in 0..r.count()? {
        let text = r.opt_str()?;
        let id = if r.bool()? {
            Some((r.str()?, r.file_ref()?))
        } else {
            None
        };
        f.errmsg.push(ErrMsg {
            text,
            id,
            ind: r.opt_u8()?,
            cond: r.cond()?,
            line: r.usize()?,
        });
    }
    f.dft = r.opt_str()?;
    f.text = r.opt_str()?;
    if r.bool()? {
        let field = r.str()?;
        let file = if r.bool()? { Some(r.file_ref()?) } else { None };
        f.reference = Some((field, file));
    }
    f.date_fmt = match r.opt_str()? {
        Some(s) => Some(DateFmt::from_word(&s)?),
        None => None,
    };
    f.date_sep = r.opt_str()?.and_then(|s| s.chars().next());
    f.time_fmt = match r.opt_str()? {
        Some(s) => Some(TimeFmt::from_word(&s)?),
        None => None,
    };
    f.time_sep = r.opt_str()?.and_then(|s| s.chars().next());
    if r.bool()? {
        f.sfl_role = Some(match r.u8()? {
            0 => SflRole::RcdNbr {
                cursor: r.bool()?,
                top: r.bool()?,
            },
            1 => SflRole::PgmQ,
            2 => SflRole::MsgKey,
            3 => SflRole::RolVal,
            _ => return None,
        });
    }
    f.responses = r.responses()?;
    f.keywords = r.keywords()?;
    f.line = r.usize()?;
    Some(f)
}

/// The model back from its section, with an empty source.
pub(super) fn read(b: &[u8]) -> Option<DisplayFile> {
    let mut r = R(b);
    let file = read_file(&mut r)?;
    let mut records = Vec::new();
    for _ in 0..r.count()? {
        records.push(read_record(&mut r)?);
    }
    if !r.0.is_empty() {
        return None;
    }
    Some(DisplayFile {
        file,
        records,
        source: Source::default(),
    })
}
