// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CPYTOSTMF` and `CPYFRMSTMF`: the member through the store's
//! `/QSYS.LIB` view, the stream file through the job's IFS, the work in
//! `ifscmd::stmf`. Both run inside one IFS call, so the store is lent
//! once and the member's transaction is the view's own.

use cmd::block::Parms;
use ifs::client::IfsClient;
use ifscmd::stmf::{self, Common, Cvt, EndLin, FrmStmf, Opt, StmfAut, StmfCcsid, ToStmf};
use qsyslib::StoreQsys;
use sec::auth::principal::Principal;
use stg::proto::{CmdMsg, CmdReply};

use crate::ipl::system::System;

/// Run `name`, or `None` when it is not one of the two.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    match name {
        "CPYTOSTMF" => {
            let a = ToStmf {
                frommbr: parms.text("FROMMBR"),
                tostmf: parms.text("TOSTMF"),
                stmfopt: Opt::of(parms.int("STMFOPT").unwrap_or(1)),
                common: common(parms, EndLin::Crlf),
                aut: StmfAut::of(parms.int("AUT").unwrap_or(1)),
            };
            let r = with_job_ifs(system, |c, p| {
                stmf::cpytostmf(c, &StoreQsys::new(crate::stgsvc::ifs::Lent), p, &a)
            });
            let text = if r.failed() {
                String::new()
            } else {
                format!("Member {} copied to {}.\n", a.frommbr, a.tostmf)
            };
            Some(reply(r, &text))
        }
        "CPYFRMSTMF" => {
            let a = FrmStmf {
                fromstmf: parms.text("FROMSTMF"),
                tombr: parms.text("TOMBR"),
                mbropt: Opt::of(parms.int("MBROPT").unwrap_or(1)),
                common: common(parms, EndLin::All),
                tabexpn: parms.int("TABEXPN").unwrap_or(1) == 1,
            };
            let r = with_job_ifs(system, |c, p| {
                stmf::cpyfrmstmf(c, &StoreQsys::new(crate::stgsvc::ifs::Lent), p, &a)
            });
            let text = if r.failed() {
                String::new()
            } else {
                format!("Stream file {} copied to {}.\n", a.fromstmf, a.tombr)
            };
            Some(reply(r, &text))
        }
        _ => None,
    }
}

/// The parameters both commands share.
fn common(parms: &Parms, end: EndLin) -> Common {
    let ccsid = |k: &str| parms.int(k).filter(|n| *n > 0).and_then(|n| u16::try_from(n).ok());
    let tbl = parms.text("TBL");
    Common {
        cvtdta: Cvt::of(parms.int("CVTDTA").unwrap_or(1)),
        dbfccsid: ccsid("DBFCCSID"),
        stmfccsid: StmfCcsid::of(parms.int("STMFCCSID").unwrap_or(-1)),
        stmfcodpag: parms
            .int("STMFCODPAG")
            .map(StmfCcsid::of)
            .filter(|c| *c != StmfCcsid::Stmf),
        tbl: (!tbl.is_empty()).then_some(tbl),
        endlinfmt: parms.int("ENDLINFMT").map_or(end, EndLin::of),
    }
}

/// Run `f` against the job's IFS, as the job, with its descriptors.
fn with_job_ifs<T>(system: &mut System, f: impl FnOnce(&dyn IfsClient, &Principal) -> T) -> T {
    let p = crate::svc::ifs::principal();
    super::stmf::with_job(system, |c| f(c, &p))
}

/// The library's reply as the service's.
pub(super) fn reply(r: ifscmd::Reply, text: &str) -> CmdReply {
    let msg = |kind: u8, m: ifscmd::Msg| CmdMsg {
        msgid: m.id,
        kind,
        data: m.data,
    };
    CmdReply {
        text: format!("{}{text}", r.text),
        messages: r.messages.into_iter().map(|(k, m)| msg(k, m)).collect(),
        escape: r.escape.map(|m| msg(ifscmd::msg::ESCAPE, m)),
        ..CmdReply::default()
    }
}
