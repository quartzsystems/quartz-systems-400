// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The application server as library code: a `Session` takes each
//! chain of DSSes a requester sends and answers it, driving the SQL
//! engine (`SqlBackend`), the files (`FileBackend`) and the password
//! check (`Authenticator`). The TCP listener (`STRTCPSVR *DDM`) feeds it
//! chains read with `wire::read_chain` and writes back what it returns.

pub mod backend;
mod file;
mod sql;

pub use backend::{
    Authenticator, FileBackend, FileError, FileInfo, FileOpen, FileRecord, Nothing, ReadAt, Section,
    SqlBackend, SqlResult,
};

use std::collections::HashMap;

use crate::cp::{cmd, mgr, rm, s38, secchkcd, svrcod};
use crate::fdoca::Repr;
use crate::msg::access::secchkrm;
use crate::msg::{Accrdb, Accrdbrm, Accsec, Accsecrd, Attributes, Secchk, TypDef, reply};
use crate::sec::{Mechanism, Server};
use crate::wire::{self, Dss, DssType, Object};

/// The server's configuration.
#[derive(Clone, Debug)]
pub struct Config {
    /// The local RDB name (`*LOCAL` entry).
    pub rdb: String,
    /// `SRVNAM`: the system name.
    pub srvnam: String,
    /// `EXTNAM`: the server job (`nnnnnn/QUSER/QRWTSRVR`).
    pub extnam: String,
    /// The mechanisms accepted, in the order offered (`CHGDDMTCPA
    /// PWDRQD`).
    pub mechanisms: Vec<Mechanism>,
    /// The SQLAM level offered.
    pub sqlam: u16,
    /// Rows per `QRYDTA` block.
    pub block_rows: usize,
}

impl Config {
    /// A configuration with QS/400's defaults (`PWDRQD(*USRENCPWD)`).
    pub fn new(rdb: &str, srvnam: &str) -> Self {
        Self {
            rdb: rdb.trim().to_ascii_uppercase(),
            srvnam: srvnam.into(),
            extnam: "000000/QUSER/QRWTSRVR".into(),
            mechanisms: crate::objects::DdmTcpAttrs::default().mechanisms(),
            sqlam: 7,
            block_rows: 64,
        }
    }
}

/// Where a session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Waiting for `EXCSAT`.
    Start,
    /// Attributes exchanged; waiting for `ACCSEC`.
    Attributes,
    /// Mechanism chosen; waiting for `SECCHK`.
    Security,
    /// The user is known; waiting for `ACCRDB` (or file commands).
    Authenticated,
    /// The RDB is accessed.
    Accessed,
}

/// One requester's conversation.
pub struct Session {
    /// The configuration.
    pub cfg: Config,
    /// Where it is.
    pub state: State,
    /// The signed-on user.
    pub user: Option<String>,
    /// The requester's attributes.
    pub requester: Attributes,
    /// The SQLAM level agreed.
    pub sqlam: u16,
    /// The requester's data representation.
    pub their_repr: Repr,
    sec: Option<Server>,
    auth: Box<dyn Authenticator>,
    sql: Box<dyn SqlBackend>,
    files: Box<dyn FileBackend>,
    queries: HashMap<Section, sql::Query>,
    opened: HashMap<String, file::Open>,
    next_qryinsid: u64,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("state", &self.state)
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

impl Session {
    /// A session over the backends.
    pub fn new(
        cfg: Config,
        auth: Box<dyn Authenticator>,
        sql: Box<dyn SqlBackend>,
        files: Box<dyn FileBackend>,
    ) -> Self {
        let sqlam = cfg.sqlam;
        Self {
            cfg,
            state: State::Start,
            user: None,
            requester: Attributes::default(),
            sqlam,
            their_repr: Repr::default(),
            sec: None,
            auth,
            sql,
            files,
            queries: HashMap::new(),
            opened: HashMap::new(),
            next_qryinsid: 1,
        }
    }

    /// Answer one chain. The replies are chained as one.
    pub fn handle(&mut self, chain: &[Dss]) -> Vec<Dss> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < chain.len() {
            let d = &chain[i];
            i += 1;
            if !matches!(d.ty, DssType::Request | DssType::RequestNoReply) {
                continue;
            }
            // The command's data objects follow with the same correlation.
            let mut data = Vec::new();
            while i < chain.len() && chain[i].ty == DssType::Object && chain[i].corr == d.corr {
                data.extend(chain[i].objects().iter().cloned());
                i += 1;
            }
            let Some(command) = d.first() else { continue };
            let (replies, stop) = self.command(command, &data);
            if d.ty == DssType::Request {
                for (ty, o) in replies {
                    out.push(Dss::new(ty, d.corr, o));
                }
            }
            if stop {
                break;
            }
        }
        wire::chain(&mut out);
        out
    }

    /// One command and its data: the replies, and whether the rest of
    /// the chain is dropped.
    fn command(&mut self, c: &Object, data: &[Object]) -> (Vec<(DssType, Object)>, bool) {
        let rpy = |o: Object| (DssType::Reply, o);
        let obj = |o: Object| (DssType::Object, o);
        match (self.state, c.cp) {
            (_, cmd::EXCSAT) => {
                self.requester = Attributes::from_object(c);
                if let Some(l) = self.requester.level(mgr::SQLAM) {
                    self.sqlam = l.min(self.cfg.sqlam);
                }
                if self.state == State::Start {
                    self.state = State::Attributes;
                }
                (vec![obj(self.attributes().to_excsatrd())], false)
            }
            (State::Attributes | State::Security, cmd::ACCSEC) => {
                let a = Accsec::from_object(c);
                let m = Mechanism::from_code(a.secmec).filter(|m| self.cfg.mechanisms.contains(m));
                let Some(m) = m else {
                    let rd = Accsecrd {
                        secmec: self.cfg.mechanisms.iter().map(|m| m.code()).collect(),
                        sectkn: None,
                        secchkcd: None,
                    };
                    return (vec![obj(rd.to_object())], false);
                };
                let s = Server::new(m, a.sectkn);
                let rd = Accsecrd {
                    secmec: vec![m.code()],
                    sectkn: s.accsecrd_token(),
                    secchkcd: None,
                };
                self.sec = Some(s);
                self.state = State::Security;
                (vec![obj(rd.to_object())], false)
            }
            (State::Security, cmd::SECCHK) => {
                let code = self.secchk(&Secchk::from_object(c));
                if code == 0 {
                    self.state = State::Authenticated;
                    (vec![rpy(secchkrm(svrcod::INFO, 0))], false)
                } else {
                    (vec![rpy(secchkrm(svrcod::ERROR, code))], true)
                }
            }
            (State::Authenticated, cmd::ACCRDB) => {
                let a = Accrdb::from_object(c);
                if a.rdbnam.trim() != self.cfg.rdb {
                    return (vec![rpy(reply::rdbnfnrm(&a.rdbnam))], false);
                }
                self.their_repr = Repr {
                    little: a.typdef.little_endian(),
                    ccsid: a.typdef.sbc.unwrap_or(37),
                };
                self.state = State::Accessed;
                let r = Accrdbrm {
                    svrcod: svrcod::INFO,
                    prdid: crate::client::product_id(),
                    typdef: TypDef::default(),
                    crrtkn: a.crrtkn,
                };
                (vec![rpy(r.to_object())], false)
            }
            (State::Accessed, code) if sql::handles(code) => (self.sql_command(c, data), false),
            (State::Authenticated | State::Accessed, code) if code >> 8 == 0xD0 => {
                (self.file_command(c, data), false)
            }
            (State::Accessed, cmd::ACCRDB) => (vec![rpy(reply::message(rm::RDBACCRM, svrcod::ERROR, vec![]))], false),
            (State::Authenticated, code) if sql::handles(code) => {
                (vec![rpy(reply::rdbnacrm(&self.cfg.rdb))], false)
            }
            (State::Start | State::Attributes | State::Security, _) => {
                (vec![rpy(reply::prccnvrm(0x01))], true)
            }
            (_, code) => (vec![rpy(reply::cmdnsprm(code))], false),
        }
    }

    fn attributes(&self) -> Attributes {
        let level = |m: u16, l: u16| {
            self.requester
                .level(m)
                .map_or(l, |r| r.min(l))
        };
        let mut managers = vec![
            (mgr::AGENT, level(mgr::AGENT, 7)),
            (mgr::SQLAM, self.sqlam),
            (mgr::RDB, level(mgr::RDB, 7)),
            (mgr::SECMGR, level(mgr::SECMGR, 7)),
            (mgr::CMNTCPIP, level(mgr::CMNTCPIP, 5)),
        ];
        managers.retain(|(m, _)| self.requester.managers.is_empty() || self.requester.level(*m).is_some());
        Attributes {
            extnam: Some(self.cfg.extnam.clone()),
            srvnam: Some(self.cfg.srvnam.clone()),
            srvclsnm: Some(crate::client::server_class()),
            srvrlslv: Some(crate::client::release_level()),
            managers,
        }
    }

    fn secchk(&mut self, s: &Secchk) -> u8 {
        let Some(server) = &self.sec else {
            return u8::try_from(secchkcd::SECMEC_NOT_SUPPORTED).unwrap_or(1);
        };
        if s.secmec != server.mech.code() {
            return u8::try_from(secchkcd::SECMEC_NOT_SUPPORTED).unwrap_or(1);
        }
        let tk: Vec<&[u8]> = s.sectkn.iter().map(Vec::as_slice).collect();
        match server.open(s.usrid.as_deref(), s.password.as_deref(), &tk) {
            Ok((user, proof)) => match self.auth.check(&user, &proof) {
                Ok(()) => {
                    self.user = Some(user);
                    0
                }
                Err(c) => c,
            },
            Err(c) => u8::try_from(c).unwrap_or(0x0F),
        }
    }

    /// Whether the command is one of the System/38 file commands.
    pub fn is_file_command(code: u16) -> bool {
        s38::ALL.iter().any(|&(_, v)| v == code)
    }
}
