// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Getting in: `ACCSEC`/`ACCSECRD` pick the security mechanism and
//! swap tokens, `SECCHK`/`SECCHKRM` prove the user, `ACCRDB`/`ACCRDBRM`
//! open the relational database with the data representation.

use crate::cp::{cmd, obj, param, rm, rpy};
use crate::wire::{Object, ebcdic_fixed, text};

/// The width RDB names are padded to.
pub const RDBNAM_LEN: usize = 18;

/// An RDB name: 18 bytes blank padded, longer names as they are.
pub fn rdbnam(name: &str) -> Object {
    let n = name.trim().to_ascii_uppercase();
    Object::data(param::RDBNAM, ebcdic_fixed(&n, n.len().max(RDBNAM_LEN)))
}

/// `ACCSEC`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Accsec {
    /// `SECMEC` asked for.
    pub secmec: u16,
    /// `RDBNAM`.
    pub rdbnam: Option<String>,
    /// `SECTKN`: the requester's public key or seed.
    pub sectkn: Option<Vec<u8>>,
    /// `ENCALG`, when not DES.
    pub encalg: Option<u16>,
}

impl Accsec {
    /// The command.
    pub fn to_object(&self) -> Object {
        let mut v = vec![Object::u16(param::SECMEC, self.secmec)];
        if let Some(r) = &self.rdbnam {
            v.push(rdbnam(r));
        }
        if let Some(t) = &self.sectkn {
            v.push(Object::data(param::SECTKN, t.clone()));
        }
        if let Some(a) = self.encalg {
            v.push(Object::u16(param::ENCALG, a));
        }
        Object::coll(cmd::ACCSEC, v)
    }

    /// From the command.
    pub fn from_object(o: &Object) -> Self {
        Self {
            secmec: o.get_u16(param::SECMEC).unwrap_or(0),
            rdbnam: o.get_chars(param::RDBNAM),
            sectkn: o.get(param::SECTKN).map(|t| t.bytes().to_vec()),
            encalg: o.get_u16(param::ENCALG),
        }
    }
}

/// `ACCSECRD`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Accsecrd {
    /// `SECMEC`: the one accepted, or every one offered.
    pub secmec: Vec<u16>,
    /// `SECTKN`: the server's public key or seed.
    pub sectkn: Option<Vec<u8>>,
    /// `SECCHKCD`, when the mechanism was refused.
    pub secchkcd: Option<u8>,
}

impl Accsecrd {
    /// The reply data.
    pub fn to_object(&self) -> Object {
        let mut d = Vec::new();
        for m in &self.secmec {
            d.extend(m.to_be_bytes());
        }
        let mut v = vec![Object::data(param::SECMEC, d)];
        if let Some(t) = &self.sectkn {
            v.push(Object::data(param::SECTKN, t.clone()));
        }
        if let Some(c) = self.secchkcd {
            v.push(Object::byte(param::SECCHKCD, c));
        }
        Object::coll(rpy::ACCSECRD, v)
    }

    /// From the reply data.
    pub fn from_object(o: &Object) -> Self {
        Self {
            secmec: o
                .all(param::SECMEC)
                .flat_map(|m| m.bytes().as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)))
                .collect(),
            sectkn: o.get(param::SECTKN).map(|t| t.bytes().to_vec()),
            secchkcd: o.get(param::SECCHKCD).and_then(|c| c.bytes().first().copied()),
        }
    }
}

/// `SECCHK`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Secchk {
    /// `SECMEC`.
    pub secmec: u16,
    /// `RDBNAM`.
    pub rdbnam: Option<String>,
    /// `USRID` (EBCDIC).
    pub usrid: Option<Vec<u8>>,
    /// `PASSWORD`.
    pub password: Option<Vec<u8>>,
    /// `SECTKN`s.
    pub sectkn: Vec<Vec<u8>>,
}

impl Secchk {
    /// The command.
    pub fn to_object(&self) -> Object {
        let mut v = vec![Object::u16(param::SECMEC, self.secmec)];
        if let Some(r) = &self.rdbnam {
            v.push(rdbnam(r));
        }
        if let Some(u) = &self.usrid {
            v.push(Object::data(param::USRID, u.clone()));
        }
        if let Some(p) = &self.password {
            v.push(Object::data(param::PASSWORD, p.clone()));
        }
        for t in &self.sectkn {
            v.push(Object::data(param::SECTKN, t.clone()));
        }
        Object::coll(cmd::SECCHK, v)
    }

    /// From the command.
    pub fn from_object(o: &Object) -> Self {
        Self {
            secmec: o.get_u16(param::SECMEC).unwrap_or(0),
            rdbnam: o.get_chars(param::RDBNAM),
            usrid: o.get(param::USRID).map(|t| t.bytes().to_vec()),
            password: o.get(param::PASSWORD).map(|t| t.bytes().to_vec()),
            sectkn: o.all(param::SECTKN).map(|t| t.bytes().to_vec()).collect(),
        }
    }
}

/// `SECCHKRM`.
pub fn secchkrm(svrcod: u16, secchkcd: u8) -> Object {
    Object::coll(
        rm::SECCHKRM,
        vec![
            Object::u16(param::SVRCOD, svrcod),
            Object::byte(param::SECCHKCD, secchkcd),
        ],
    )
}

/// The data representation: `TYPDEFNAM` and the CCSIDs of `TYPDEFOVR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypDef {
    /// `QTDSQL400`, `QTDSQL370`, `QTDSQLX86`, `QTDSQLASC`.
    pub name: String,
    /// `CCSIDSBC`.
    pub sbc: Option<u16>,
    /// `CCSIDDBC`.
    pub dbc: Option<u16>,
    /// `CCSIDMBC`.
    pub mbc: Option<u16>,
}

impl Default for TypDef {
    fn default() -> Self {
        Self {
            name: "QTDSQL400".into(),
            sbc: Some(37),
            dbc: None,
            mbc: None,
        }
    }
}

impl TypDef {
    /// Whether integers are little-endian.
    pub fn little_endian(&self) -> bool {
        matches!(self.name.as_str(), "QTDSQLX86" | "QTDSQLVAX")
    }

    fn push(&self, v: &mut Vec<Object>) {
        v.push(Object::chars(obj::TYPDEFNAM, &self.name));
        let mut o = Vec::new();
        for (cp, c) in [
            (param::CCSIDSBC, self.sbc),
            (param::CCSIDDBC, self.dbc),
            (param::CCSIDMBC, self.mbc),
        ] {
            if let Some(c) = c {
                o.push(Object::u16(cp, c));
            }
        }
        v.push(Object::coll(obj::TYPDEFOVR, o));
    }

    fn from(o: &Object) -> Self {
        let ovr = o.get(obj::TYPDEFOVR);
        let c = |cp| ovr.and_then(|x| x.get_u16(cp));
        Self {
            name: o.get_chars(obj::TYPDEFNAM).unwrap_or_else(|| "QTDSQL400".into()),
            sbc: c(param::CCSIDSBC),
            dbc: c(param::CCSIDDBC),
            mbc: c(param::CCSIDMBC),
        }
    }
}

/// `ACCRDB`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accrdb {
    /// `RDBNAM`.
    pub rdbnam: String,
    /// `RDBACCCL`: the access manager class (`SQLAM`).
    pub rdbacccl: u16,
    /// `PRDID`: the requester's product (`QSQ07050` style).
    pub prdid: String,
    /// The data representation.
    pub typdef: TypDef,
    /// `CRRTKN`.
    pub crrtkn: Option<Vec<u8>>,
    /// `PRDDTA`.
    pub prddta: Option<Vec<u8>>,
}

impl Accrdb {
    /// The command.
    pub fn to_object(&self) -> Object {
        let mut v = vec![
            rdbnam(&self.rdbnam),
            Object::u16(param::RDBACCCL, self.rdbacccl),
            Object::chars(param::PRDID, &self.prdid),
        ];
        self.typdef.push(&mut v);
        if let Some(t) = &self.crrtkn {
            v.push(Object::data(param::CRRTKN, t.clone()));
        }
        if let Some(t) = &self.prddta {
            v.push(Object::data(param::PRDDTA, t.clone()));
        }
        Object::coll(cmd::ACCRDB, v)
    }

    /// From the command.
    pub fn from_object(o: &Object) -> Self {
        Self {
            rdbnam: o.get_chars(param::RDBNAM).unwrap_or_default(),
            rdbacccl: o.get_u16(param::RDBACCCL).unwrap_or(0),
            prdid: o.get_chars(param::PRDID).unwrap_or_default(),
            typdef: TypDef::from(o),
            crrtkn: o.get(param::CRRTKN).map(|t| t.bytes().to_vec()),
            prddta: o.get(param::PRDDTA).map(|t| t.bytes().to_vec()),
        }
    }
}

/// `ACCRDBRM`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accrdbrm {
    /// `SVRCOD`.
    pub svrcod: u16,
    /// `PRDID`: the server's product.
    pub prdid: String,
    /// The server's data representation.
    pub typdef: TypDef,
    /// `CRRTKN`, echoed or assigned.
    pub crrtkn: Option<Vec<u8>>,
}

impl Accrdbrm {
    /// The reply message.
    pub fn to_object(&self) -> Object {
        let mut v = vec![
            Object::u16(param::SVRCOD, self.svrcod),
            Object::chars(param::PRDID, &self.prdid),
        ];
        self.typdef.push(&mut v);
        if let Some(t) = &self.crrtkn {
            v.push(Object::data(param::CRRTKN, t.clone()));
        }
        Object::coll(rm::ACCRDBRM, v)
    }

    /// From the reply message.
    pub fn from_object(o: &Object) -> Self {
        Self {
            svrcod: o.svrcod(),
            prdid: o.get(param::PRDID).map(|p| text(p.bytes())).unwrap_or_default(),
            typdef: TypDef::from(o),
            crrtkn: o.get(param::CRRTKN).map(|t| t.bytes().to_vec()),
        }
    }
}
