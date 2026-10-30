// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The application requester: `Connection` connects to an RDB over any
//! byte stream (`EXCSAT`, `ACCSEC`, `SECCHK`, `ACCRDB`), then prepares,
//! opens, fetches and closes queries, runs statements and ends units of
//! work. `CONNECT TO` an `*RDB` entry and DDM files use it.

pub mod file;
pub mod query;

pub use query::{Prepared, Rows};

use std::io::{Read, Write};

use crate::cp::{mgr, rm, rpy, secchkcd, svrcod};
use crate::fdoca::Repr;
use crate::msg::{Accrdb, Accrdbrm, Accsec, Accsecrd, Attributes, Secchk, TypDef};
use crate::sec::{Mechanism, Requester};
use crate::wire::{self, Dss, Object};
use crate::{Error, cp};

/// What a connection needs.
#[derive(Clone, Debug)]
pub struct ConnectOpts {
    /// The RDB name (`*RDB` entry's `RDB`).
    pub rdb: String,
    /// The user.
    pub user: String,
    /// The password.
    pub password: String,
    /// Mechanisms to try, in order; the server's offer picks among them.
    pub mechanisms: Vec<Mechanism>,
    /// `EXTNAM`.
    pub extnam: String,
    /// `SRVNAM` (this system).
    pub srvnam: String,
    /// The SQLAM level asked for.
    pub sqlam: u16,
    /// Use SHA-512 for `USRSSBPWD` (password level 4).
    pub sha512: bool,
    /// The collection of the package dynamic statements use.
    pub collection: String,
    /// The package.
    pub package: String,
    /// Access the RDB (`ACCRDB`); off for record-level access alone.
    pub access_rdb: bool,
}

impl ConnectOpts {
    /// Options with QS/400's defaults.
    pub fn new(rdb: &str, user: &str, password: &str) -> Self {
        Self {
            rdb: rdb.into(),
            user: user.into(),
            password: password.into(),
            mechanisms: Mechanism::ALL.to_vec(),
            extnam: "QS400DDM".into(),
            srvnam: "QS400".into(),
            sqlam: 7,
            sha512: false,
            collection: "NULLID".into(),
            package: "SYSSH200".into(),
            access_rdb: true,
        }
    }
}

/// A connection to one RDB.
#[derive(Debug)]
pub struct Connection<S> {
    stream: S,
    corr: u16,
    /// The RDB.
    pub rdb: String,
    /// The SQLAM level both sides speak.
    pub sqlam: u16,
    /// The data representation.
    pub repr: Repr,
    /// The server's attributes.
    pub server: Attributes,
    /// The mechanism used.
    pub mechanism: Option<Mechanism>,
    /// What `ACCRDB` answered.
    pub accrdbrm: Option<Accrdbrm>,
    next_section: u16,
    /// The collection and package dynamic statements are prepared in.
    pub package: (String, String),
}

impl<S: Read + Write> Connection<S> {
    /// A connection not yet started (for `exchange`).
    pub fn raw(stream: S, rdb: &str) -> Self {
        Self {
            stream,
            corr: 0,
            rdb: rdb.trim().to_ascii_uppercase(),
            sqlam: 7,
            repr: Repr::default(),
            server: Attributes::default(),
            mechanism: None,
            accrdbrm: None,
            next_section: 1,
            package: ("NULLID".into(), "SYSSH200".into()),
        }
    }

    /// Connect and access the RDB.
    #[allow(clippy::too_many_lines)] // the four steps in order
    pub fn connect(stream: S, o: &ConnectOpts) -> Result<Self, Error> {
        let mut c = Self::raw(stream, &o.rdb);
        c.package = (o.collection.clone(), o.package.clone());
        let ask = Attributes {
            extnam: Some(o.extnam.clone()),
            srvnam: Some(o.srvnam.clone()),
            srvclsnm: Some(server_class()),
            srvrlslv: Some(release_level()),
            managers: Attributes::requester_managers()
                .into_iter()
                .map(|(m, l)| if m == mgr::SQLAM { (m, o.sqlam) } else { (m, l) })
                .collect(),
        };
        let first = o.mechanisms.first().copied().unwrap_or(Mechanism::UsrIdPwd);
        let mut req = Requester::new(first);
        let corr = c.next_corr();
        let replies = c.exchange(vec![
            Dss::request(corr, ask.to_excsat()),
            Dss::request(corr + 1, c.accsec(&req, o)),
        ])?;
        c.corr = corr + 1;
        let mut accsecrd = None;
        for d in &replies {
            if let Some(obj) = d.first() {
                match obj.cp {
                    rpy::EXCSATRD => c.server = Attributes::from_object(obj),
                    rpy::ACCSECRD => accsecrd = Some(Accsecrd::from_object(obj)),
                    _ => return Err(reply_error(obj)),
                }
            }
        }
        c.sqlam = c.server.level(mgr::SQLAM).unwrap_or(o.sqlam).min(o.sqlam);
        let mut rd = accsecrd.ok_or(Error::Missing("ACCSECRD"))?;
        if rd.secchkcd.is_some() || !rd.secmec.contains(&req.mech.code()) {
            // Refused: take the first of ours the server offers.
            let pick = o
                .mechanisms
                .iter()
                .copied()
                .find(|m| rd.secmec.contains(&m.code()))
                .ok_or_else(|| Error::Reply {
                    name: format!("no common security mechanism (server offers {:?})", rd.secmec),
                    svrcod: svrcod::ERROR,
                })?;
            req = Requester::new(pick);
            let corr = c.next_corr();
            let r = c.exchange(vec![Dss::request(corr, c.accsec(&req, o))])?;
            let obj = r.first().and_then(Dss::first).ok_or(Error::Missing("ACCSECRD"))?;
            if obj.cp != rpy::ACCSECRD {
                return Err(reply_error(obj));
            }
            rd = Accsecrd::from_object(obj);
            if rd.secchkcd.is_some() {
                return Err(Error::Reply {
                    name: format!("ACCSEC refused, SECCHKCD {:?}", rd.secchkcd),
                    svrcod: svrcod::ERROR,
                });
            }
        }
        c.mechanism = Some(req.mech);
        let data = req.secchk(&o.user, &o.password, rd.sectkn.as_deref(), o.sha512)?;
        let secchk = Secchk {
            secmec: req.mech.code(),
            rdbnam: Some(c.rdb.clone()),
            usrid: data.usrid,
            password: data.password,
            sectkn: data.sectkn,
        };
        let accrdb = Accrdb {
            rdbnam: c.rdb.clone(),
            rdbacccl: mgr::SQLAM,
            prdid: product_id(),
            typdef: TypDef::default(),
            crrtkn: Some(crate::wire::ebcdic("QS400.DDM.000001")),
            prddta: None,
        };
        let corr = c.next_corr();
        let mut list = vec![Dss::request(corr, secchk.to_object())];
        if o.access_rdb {
            list.push(Dss::request(corr + 1, accrdb.to_object()));
        }
        let replies = c.exchange(list)?;
        c.corr = corr + u16::from(o.access_rdb);
        for d in &replies {
            let Some(obj) = d.first() else { continue };
            match obj.cp {
                rm::SECCHKRM => {
                    let code = obj
                        .get(cp::param::SECCHKCD)
                        .and_then(|x| x.bytes().first().copied())
                        .unwrap_or(0);
                    if u16::from(code) != secchkcd::OK {
                        return Err(Error::Reply {
                            name: format!("SECCHKRM, SECCHKCD {code:#04x}"),
                            svrcod: obj.svrcod(),
                        });
                    }
                }
                rm::ACCRDBRM => {
                    let a = Accrdbrm::from_object(obj);
                    c.repr = Repr {
                        little: a.typdef.little_endian(),
                        ccsid: a.typdef.sbc.unwrap_or(37),
                    };
                    c.accrdbrm = Some(a);
                }
                cp::obj::SQLCARD => {}
                _ => return Err(reply_error(obj)),
            }
        }
        if o.access_rdb && c.accrdbrm.is_none() {
            return Err(Error::Missing("ACCRDBRM"));
        }
        Ok(c)
    }

    fn accsec(&self, req: &Requester, o: &ConnectOpts) -> Object {
        let _ = o;
        Accsec {
            secmec: req.mech.code(),
            rdbnam: Some(self.rdb.clone()),
            sectkn: req.accsec_token(),
            encalg: None,
        }
        .to_object()
    }

    /// The next correlation ID.
    pub fn next_corr(&mut self) -> u16 {
        self.corr = self.corr.wrapping_add(1).max(1);
        self.corr
    }

    /// The package statement `section` is prepared in.
    pub fn pkg(&self, section: u16) -> crate::msg::Pkgnamcsn {
        crate::msg::Pkgnamcsn {
            collection: self.package.0.clone(),
            package: self.package.1.clone(),
            ..crate::msg::Pkgnamcsn::dynamic(&self.rdb, section)
        }
    }

    /// The next free section number.
    pub fn next_section(&mut self) -> u16 {
        let s = self.next_section;
        self.next_section += 1;
        s
    }

    /// Send a chain and read the replies, one chain per correlation ID
    /// that wants a reply.
    pub fn exchange(&mut self, mut list: Vec<Dss>) -> Result<Vec<Dss>, Error> {
        wire::chain(&mut list);
        self.stream.write_all(&wire::encode_chain(&list))?;
        self.stream.flush()?;
        let mut want: Vec<u16> = list
            .iter()
            .filter(|d| d.ty == wire::DssType::Request)
            .map(|d| d.corr)
            .collect();
        want.dedup();
        let mut out = Vec::new();
        while !want.is_empty() {
            let chain = wire::read_chain(&mut self.stream)?;
            for d in &chain {
                want.retain(|&c| c != d.corr);
            }
            // A failed check ends the chain: the server drops the rest.
            let failed = chain.iter().any(|d| {
                d.first()
                    .is_some_and(|o| o.cp == rm::SECCHKRM && o.svrcod() >= svrcod::ERROR)
            });
            out.extend(chain);
            if failed {
                break;
            }
        }
        Ok(out)
    }

    /// Commit (`RDBCMM`).
    pub fn commit(&mut self) -> Result<crate::sql::Sqlca, Error> {
        let corr = self.next_corr();
        let r = self.exchange(vec![Dss::request(corr, crate::msg::sqlcmd::rdbcmm())])?;
        query::sqlca_of(self, &r)
    }

    /// Roll back (`RDBRLLBCK`).
    pub fn rollback(&mut self) -> Result<crate::sql::Sqlca, Error> {
        let corr = self.next_corr();
        let r = self.exchange(vec![Dss::request(corr, crate::msg::sqlcmd::rdbrllbck())])?;
        query::sqlca_of(self, &r)
    }

    /// The stream back.
    pub fn into_inner(self) -> S {
        self.stream
    }
}

/// The error a reply message we did not want stands for.
pub fn reply_error(o: &Object) -> Error {
    Error::Reply {
        name: cp::name(o.cp).map_or_else(|| format!("{:04X}", o.cp), str::to_string),
        svrcod: o.svrcod(),
    }
}

/// `SRVRLSLV`: QS/400's product and release.
pub fn release_level() -> String {
    product_id()
}

/// `SRVCLSNM`: QS/400's server class. Not `QAS`: an IBM i server
/// answers a `QAS` requester with its private SQLCA objects.
pub fn server_class() -> String {
    "QS400/QSD".into()
}

/// `PRDID`: a product prefix, version, release, modification. QS/400
/// does not claim `QSQ` (DB2 for i).
pub fn product_id() -> String {
    "QSD01000".into()
}
