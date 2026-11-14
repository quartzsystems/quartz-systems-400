// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The projected backend's profiles over `sec` (docs/DIRSRV.md §9): the
//! `os400-*` attributes of each profile, who sees every profile
//! (`*ALLOBJ`, `*SECADM`) and who has `*READ` to one.

use dirsrv::backend::projected::{ProfileInfo, Profiles};
use dirsrv::ops::clock::gentime;
use dirsrv::ops::error::{OpError, OpResult};
use ldap::ResultCode;
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::auth::required::Need;
use sec::profile::payload::UsrPrf;
use sec::profile::spcaut::SpecialAuth;
use sec::profile::{groups, objects};
use store::obj::ObjectId;
use store::obj::header::Authority;
use store::txn::Txn;

/// The profiles of the store whose `QSYS` is `qsys`.
pub struct SecProfiles {
    /// `QSYS`.
    pub qsys: ObjectId,
    /// `QSECURITY`, for the authority search.
    pub qsecurity: u8,
}

fn store_err(e: &sec::SecError) -> OpError {
    OpError::new(ResultCode::OperationsError, e.to_string())
}

fn qual((n, l): &(String, String)) -> String {
    if n.starts_with('*') || l.is_empty() || l == "*LIBL" {
        n.clone()
    } else {
        format!("{l}/{n}")
    }
}

fn yes(b: bool) -> String {
    if b { "*YES" } else { "*NO" }.into()
}

fn when(secs: u64) -> Option<String> {
    (secs > 0).then(|| gentime(secs.saturating_mul(1_000_000)))
}

/// The `os400-*` attributes of profile `p` (its password and document
/// password never: they are write-only and secret).
#[must_use]
pub fn attributes(p: &UsrPrf) -> Vec<(String, Vec<String>)> {
    let one = |v: String| {
        if v.trim().is_empty() {
            Vec::new()
        } else {
            vec![v.trim().to_string()]
        }
    };
    let many = |v: &[String]| -> Vec<String> {
        v.iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let spcaut: Vec<String> = SpecialAuth::NAMES
        .iter()
        .filter(|(_, a)| p.spcaut.has(*a))
        .map(|(n, _)| (*n).to_string())
        .collect();
    let mut v: Vec<(&str, Vec<String>)> = vec![
        ("os400-text", one(p.text.clone())),
        (
            "os400-status",
            vec![if p.disabled { "*DISABLED" } else { "*ENABLED" }.into()],
        ),
        ("os400-usrcls", vec![p.usrcls.name().into()]),
        ("os400-spcaut", spcaut),
        ("os400-pwdexp", vec![yes(p.pwdexp)]),
        ("os400-lmtcpb", vec![p.lmtcpb.name().into()]),
        ("os400-dspsgninf", vec![p.dspsgninf.name().into()]),
        ("os400-lmtdevssn", vec![p.lmtdevssn.name().into()]),
        ("os400-lclpwdmgt", vec![yes(p.lclpwdmgt)]),
        ("os400-ptylmt", vec![p.ptylmt.to_string()]),
        ("os400-pwdexpitv", vec![p.pwdexpitv.to_string()]),
        (
            "os400-maxstg",
            vec![if p.maxstg < 0 {
                "*NOMAX".into()
            } else {
                p.maxstg.to_string()
            }],
        ),
        ("os400-storageUsed", vec![p.stg_used.to_string()]),
        (
            "os400-owner",
            vec![if p.owner_grpprf { "*GRPPRF" } else { "*USRPRF" }.into()],
        ),
        (
            "os400-grpauttyp",
            vec![if p.grpauttyp_pgp { "*PGP" } else { "*PRIVATE" }.into()],
        ),
        ("os400-sev", vec![p.sev.to_string()]),
        ("os400-objaud", one(p.objaud.clone())),
        ("os400-ccsid", vec![p.ccsid.to_string()]),
        ("os400-uid", vec![p.uid.to_string()]),
        ("os400-gid", vec![p.gid.to_string()]),
        (
            "os400-invalidSignonCount",
            vec![p.signon_invalid.to_string()],
        ),
        (
            "os400-previousSignon",
            when(p.prev_signon).into_iter().collect(),
        ),
        (
            "os400-passwordLastChanged",
            when(p.pwd_changed).into_iter().collect(),
        ),
        ("os400-astlvl", one(p.astlvl.clone())),
        ("os400-curlib", one(p.curlib.clone())),
        ("os400-inlpgm", one(qual(&p.inlpgm))),
        ("os400-inlmnu", one(qual(&p.inlmnu))),
        ("os400-spcenv", one(p.spcenv.clone())),
        ("os400-kbdbuf", one(p.kbdbuf.clone())),
        ("os400-jobd", one(qual(&p.jobd))),
        ("os400-grpprf", one(p.grpprf.clone())),
        ("os400-grpaut", one(p.grpaut.clone())),
        ("os400-acgcde", one(p.acgcde.clone())),
        ("os400-dlvry", one(p.dlvry.clone())),
        ("os400-msgq", one(qual(&p.msgq))),
        ("os400-prtdev", one(p.prtdev.clone())),
        ("os400-outq", one(qual(&p.outq))),
        ("os400-atnpgm", one(qual(&p.atnpgm))),
        ("os400-srtseq", one(qual(&p.srtseq))),
        ("os400-langid", one(p.langid.clone())),
        ("os400-cntryid", one(p.cntryid.clone())),
        ("os400-chridctl", one(p.chridctl.clone())),
        ("os400-eimassoc", one(p.eimassoc.clone())),
        ("os400-supgrpprf", many(&p.supgrpprf)),
        ("os400-locale", one(p.locale.clone())),
        ("os400-homedir", one(p.homedir.clone())),
        ("os400-setjobatr", many(&p.setjobatr)),
        ("os400-usropt", many(&p.usropt)),
        ("os400-audlvl", many(&p.audlvl)),
    ];
    v.retain(|(_, vals)| !vals.is_empty());
    v.into_iter()
        .map(|(t, vals)| (t.to_string(), vals))
        .collect()
}

impl SecProfiles {
    fn principal(&self, txn: &mut Txn<'_>, user: &str) -> Option<Principal> {
        Principal::load(txn, self.qsys, user, self.qsecurity).ok()
    }
}

impl Profiles for SecProfiles {
    fn names(&self, txn: &mut Txn<'_>) -> OpResult<Vec<String>> {
        objects::list(txn, self.qsys).map_err(|e| store_err(&e))
    }

    fn profile(&self, txn: &mut Txn<'_>, name: &str) -> OpResult<Option<ProfileInfo>> {
        let Some(id) = objects::find(txn, self.qsys, name).map_err(|e| store_err(&e))? else {
            return Ok(None);
        };
        let p = objects::read_id(txn, id).map_err(|e| store_err(&e))?;
        let group = groups::is_group(txn, self.qsys, &p.name).unwrap_or(false);
        let members = if group {
            groups::members(txn, self.qsys, &p.name).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(Some(ProfileInfo {
            name: p.name.clone(),
            group,
            members,
            attrs: attributes(&p),
        }))
    }

    fn sees_all(&self, txn: &mut Txn<'_>, viewer: &str) -> bool {
        self.principal(txn, viewer)
            .is_some_and(|p| p.has(SpecialAuth::ALLOBJ) || p.has(SpecialAuth::SECADM))
    }

    fn may_read(&self, txn: &mut Txn<'_>, viewer: &str, name: &str) -> bool {
        let Some(me) = self.principal(txn, viewer) else {
            return false;
        };
        let Ok(Some(id)) = objects::find(txn, self.qsys, name) else {
            return false;
        };
        let Ok(h) = sec::auth::grant::header(txn, id) else {
            return false;
        };
        sec::auth::search::check(txn, self.qsys, &me, &h, Need::Bits(Auth(Authority::READ)))
            .is_ok_and(|d| d.allowed)
    }
}
