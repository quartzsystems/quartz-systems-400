// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*USRPRF` payload: a fixed part ([`LAYOUT`]) and a tail of the
//! lists and the password hashes. Values that may be special (`*SYSVAL`,
//! `*NOMAX`, `*NONE`) are kept as codes or text as the field says;
//! `docs/SECURITY.md` has the table.

use crate::profile::spcaut::{SpecialAuth, UserClass};
use crate::rec::{Layout, Rec, Tail, field};
use crate::{Result, SecError};

/// The fixed part.
pub const LAYOUT: Layout = Layout {
    name: "UserProfile",
    size: Some(512),
    fields: &[
        field("Magic", 0, 8),
        field("Version", 8, 2),
        field("Status", 10, 1),
        field("UsrCls", 11, 1),
        field("SpcAut", 12, 4),
        field("PwdExp", 16, 1),
        field("PwdNone", 17, 1),
        field("PwdForms", 18, 1),
        field("LmtCpb", 19, 1),
        field("DspSgnInf", 20, 1),
        field("LclPwdMgt", 21, 1),
        field("LmtDevSsn", 22, 1),
        field("PtyLmt", 23, 1),
        field("PwdExpItv", 24, 4),
        field("PwdChgBlk", 28, 4),
        field("MaxStg", 32, 8),
        field("MaxStgLrg", 40, 8),
        field("StgUsed", 48, 8),
        field("Owner", 56, 1),
        field("GrpAutTyp", 57, 1),
        field("Sev", 58, 1),
        field("ObjAud", 59, 1),
        field("Ccsid", 60, 4),
        field("Uid", 64, 4),
        field("Gid", 68, 4),
        field("UsrExpItv", 72, 4),
        field("SignOnInvalid", 76, 4),
        field("Created", 80, 8),
        field("Changed", 88, 8),
        field("LastSignOn", 96, 8),
        field("PrevSignOn", 104, 8),
        field("PwdChanged", 112, 8),
        field("UsrExpDate", 120, 8),
        field("OwnedCount", 128, 8),
        field("Reserved", 136, 8),
        field("Name", 144, 10),
        field("AstLvl", 154, 10),
        field("CurLib", 164, 10),
        field("InlPgm", 174, 10),
        field("InlPgmLib", 184, 10),
        field("InlMnu", 194, 10),
        field("InlMnuLib", 204, 10),
        field("SpcEnv", 214, 10),
        field("KbdBuf", 224, 10),
        field("JobD", 234, 10),
        field("JobDLib", 244, 10),
        field("GrpPrf", 254, 10),
        field("GrpAut", 264, 10),
        field("AcgCde", 274, 15),
        field("Dlvry", 289, 10),
        field("MsgQ", 299, 10),
        field("MsgQLib", 309, 10),
        field("PrtDev", 319, 10),
        field("OutQ", 329, 10),
        field("OutQLib", 339, 10),
        field("AtnPgm", 349, 10),
        field("AtnPgmLib", 359, 10),
        field("SrtSeq", 369, 10),
        field("SrtSeqLib", 379, 10),
        field("LangId", 389, 10),
        field("CntryId", 399, 10),
        field("ChrIdCtl", 409, 10),
        field("EimAssoc", 419, 10),
        field("AuthMth", 429, 10),
        field("DocPwd", 439, 10),
        field("Text", 449, 50),
        field("Reserved2", 499, 13),
    ],
};

const MAGIC: &[u8; 8] = b"QSSECUSR";
const VERSION: u64 = 1;

/// Tail tags.
mod tag {
    pub const SUPGRP: u8 = 1;
    pub const LOCALE: u8 = 2;
    pub const HOMEDIR: u8 = 3;
    pub const SETJOBATR: u8 = 4;
    pub const USROPT: u8 = 5;
    pub const PASSWORD: u8 = 6;
    pub const HISTORY: u8 = 7;
    pub const AUDLVL: u8 = 8;
}

/// Most supplemental groups.
pub const MAX_SUPGRP: usize = 15;

/// `*SYSVAL` in a numeric field that allows it.
pub const SYSVAL: u32 = 0;
/// `*NOMAX` (or `*NONE` for `PWDCHGBLK`) in a numeric field.
pub const NOMAX: u32 = u32::MAX;

/// Which password form a hash is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PwdForm {
    /// Levels 0 and 1: at most ten characters, upper case.
    Short,
    /// Levels 2 and 3: at most 128 characters, case-sensitive.
    Long,
}

/// One stored hash: its form and its Argon2id PHC string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PwdHash {
    /// The form.
    pub form: PwdForm,
    /// `$argon2id$...`.
    pub phc: String,
}

impl PwdHash {
    fn encode(&self) -> Vec<u8> {
        let mut b = vec![match self.form {
            PwdForm::Short => 0,
            PwdForm::Long => 1,
        }];
        b.extend_from_slice(self.phc.as_bytes());
        b
    }

    fn decode(b: &[u8]) -> Result<Self> {
        let (&f, rest) = b
            .split_first()
            .ok_or_else(|| SecError::Corrupt("empty password item".into()))?;
        Ok(Self {
            form: if f == 0 {
                PwdForm::Short
            } else {
                PwdForm::Long
            },
            phc: String::from_utf8_lossy(rest).into_owned(),
        })
    }
}

/// `LMTCPB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LmtCpb {
    /// `*NO`.
    #[default]
    No,
    /// `*PARTIAL`: the initial menu may not be changed at sign-on.
    Partial,
    /// `*YES`.
    Yes,
}

impl LmtCpb {
    /// `*NO` ...
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::No => "*NO",
            Self::Partial => "*PARTIAL",
            Self::Yes => "*YES",
        }
    }

    /// By name.
    #[must_use]
    pub fn by_name(n: &str) -> Option<Self> {
        [Self::No, Self::Partial, Self::Yes]
            .into_iter()
            .find(|v| v.name().eq_ignore_ascii_case(n))
    }
}

/// A three-way value: `*SYSVAL`, `*NO` or `*YES` (`DSPSGNINF`, `LMTDEVSSN`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tri {
    /// `*SYSVAL`.
    #[default]
    SysVal,
    /// `*NO`.
    No,
    /// `*YES`.
    Yes,
}

impl Tri {
    /// The stored code.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::SysVal => 0,
            Self::No => 1,
            Self::Yes => 2,
        }
    }

    /// From the stored code.
    #[must_use]
    pub fn from_code(c: u8) -> Self {
        match c {
            1 => Self::No,
            2 => Self::Yes,
            _ => Self::SysVal,
        }
    }

    /// `*SYSVAL` ...
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::SysVal => "*SYSVAL",
            Self::No => "*NO",
            Self::Yes => "*YES",
        }
    }

    /// By name.
    #[must_use]
    pub fn by_name(n: &str) -> Option<Self> {
        [Self::SysVal, Self::No, Self::Yes]
            .into_iter()
            .find(|v| v.name().eq_ignore_ascii_case(n))
    }

    /// Resolved against the system value's "on" state.
    #[must_use]
    pub fn resolve(self, sysval_on: bool) -> bool {
        match self {
            Self::SysVal => sysval_on,
            Self::No => false,
            Self::Yes => true,
        }
    }
}

/// A user profile. Names are upper case; `lib` halves are `*LIBL` when
/// a qualified value has no library.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct UsrPrf {
    /// `USRPRF`.
    pub name: String,
    /// `STATUS(*DISABLED)`.
    pub disabled: bool,
    /// `USRCLS`.
    pub usrcls: UserClass,
    /// `SPCAUT`, resolved: `*USRCLS` is stored as the class defaults.
    pub spcaut: SpecialAuth,
    /// `PWDEXP(*YES)`.
    pub pwdexp: bool,
    /// `PASSWORD(*NONE)`.
    pub pwd_none: bool,
    /// The current password's hashes.
    pub hashes: Vec<PwdHash>,
    /// Previous passwords' hashes, newest first (`QPWDRQDDIF`).
    pub history: Vec<PwdHash>,
    /// `LMTCPB`.
    pub lmtcpb: LmtCpb,
    /// `DSPSGNINF`.
    pub dspsgninf: Tri,
    /// `LCLPWDMGT(*YES)`.
    pub lclpwdmgt: bool,
    /// `LMTDEVSSN`.
    pub lmtdevssn: Tri,
    /// `PTYLMT` 0-9.
    pub ptylmt: u8,
    /// `PWDEXPITV` days; [`SYSVAL`] or [`NOMAX`].
    pub pwdexpitv: u32,
    /// `PWDCHGBLK` hours; [`SYSVAL`] or [`NOMAX`] for `*NONE`.
    pub pwdchgblk: u32,
    /// `MAXSTG` kilobytes, -1 for `*NOMAX`.
    pub maxstg: i64,
    /// `MAXSTGLRG` kilobytes, -1 for `*NOMAX`.
    pub maxstglrg: i64,
    /// Storage the profile's objects use, kilobytes.
    pub stg_used: u64,
    /// `OWNER(*GRPPRF)`.
    pub owner_grpprf: bool,
    /// `GRPAUTTYP(*PGP)`.
    pub grpauttyp_pgp: bool,
    /// `SEV` 0-99.
    pub sev: u8,
    /// `OBJAUD`: `*NONE`, `*CHANGE`, `*ALL`.
    pub objaud: String,
    /// `CCSID`, 0 for `*SYSVAL`.
    pub ccsid: u32,
    /// `UID`.
    pub uid: u32,
    /// `GID`, 0 for `*NONE`.
    pub gid: u32,
    /// `USREXPITV` days, 0 for none.
    pub usrexpitv: u32,
    /// Sign-on attempts not valid since the last good one.
    pub signon_invalid: u32,
    /// Unix seconds.
    pub created: u64,
    /// Unix seconds.
    pub changed: u64,
    /// The last sign-on, Unix seconds, 0 for never.
    pub last_signon: u64,
    /// The sign-on before it.
    pub prev_signon: u64,
    /// When the password was last set, Unix seconds.
    pub pwd_changed: u64,
    /// `USREXPDATE`, Unix seconds at midnight, 0 for `*NONE`.
    pub usrexpdate: u64,
    /// Objects the profile owns.
    pub owned_count: u64,
    /// `ASTLVL`.
    pub astlvl: String,
    /// `CURLIB` or `*CRTDFT`.
    pub curlib: String,
    /// `INLPGM`: name (`*NONE`) and library.
    pub inlpgm: (String, String),
    /// `INLMNU`: name (`MAIN`, `*SIGNOFF`) and library.
    pub inlmnu: (String, String),
    /// `SPCENV`.
    pub spcenv: String,
    /// `KBDBUF`.
    pub kbdbuf: String,
    /// `JOBD`.
    pub jobd: (String, String),
    /// `GRPPRF` or `*NONE`.
    pub grpprf: String,
    /// `GRPAUT`.
    pub grpaut: String,
    /// `ACGCDE`.
    pub acgcde: String,
    /// `DLVRY`.
    pub dlvry: String,
    /// `MSGQ`: `*USRPRF` or a name, and library.
    pub msgq: (String, String),
    /// `PRTDEV`.
    pub prtdev: String,
    /// `OUTQ`.
    pub outq: (String, String),
    /// `ATNPGM`.
    pub atnpgm: (String, String),
    /// `SRTSEQ`.
    pub srtseq: (String, String),
    /// `LANGID`.
    pub langid: String,
    /// `CNTRYID`.
    pub cntryid: String,
    /// `CHRIDCTL`.
    pub chridctl: String,
    /// `EIMASSOC`, stored only.
    pub eimassoc: String,
    /// `AUTHMTH`, stored only.
    pub authmth: String,
    /// `DOCPWD`, stored only.
    pub docpwd: String,
    /// `TEXT`.
    pub text: String,
    /// `SUPGRPPRF`, at most fifteen.
    pub supgrpprf: Vec<String>,
    /// `LOCALE`.
    pub locale: String,
    /// `HOMEDIR`.
    pub homedir: String,
    /// `SETJOBATR`.
    pub setjobatr: Vec<String>,
    /// `USROPT`.
    pub usropt: Vec<String>,
    /// `AUDLVL` (`CHGUSRAUD`).
    pub audlvl: Vec<String>,
}

impl UsrPrf {
    /// `CRTUSRPRF`'s defaults for `name` at a security level.
    #[must_use]
    pub fn new(name: &str, qsecurity: u8) -> Self {
        let s = |v: &str| v.to_string();
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        Self {
            name: name.to_ascii_uppercase(),
            disabled: false,
            usrcls: UserClass::User,
            spcaut: UserClass::User.defaults(qsecurity),
            pwdexp: false,
            pwd_none: true,
            hashes: Vec::new(),
            history: Vec::new(),
            lmtcpb: LmtCpb::No,
            dspsgninf: Tri::SysVal,
            lclpwdmgt: true,
            lmtdevssn: Tri::SysVal,
            ptylmt: 3,
            pwdexpitv: SYSVAL,
            pwdchgblk: SYSVAL,
            maxstg: -1,
            maxstglrg: -1,
            stg_used: 0,
            owner_grpprf: false,
            grpauttyp_pgp: false,
            sev: 0,
            objaud: s("*NONE"),
            ccsid: 0,
            uid: 0,
            gid: 0,
            usrexpitv: 0,
            signon_invalid: 0,
            created: 0,
            changed: 0,
            last_signon: 0,
            prev_signon: 0,
            pwd_changed: 0,
            usrexpdate: 0,
            owned_count: 0,
            astlvl: s("*SYSVAL"),
            curlib: s("*CRTDFT"),
            inlpgm: pair("*NONE", ""),
            inlmnu: pair("MAIN", "*LIBL"),
            spcenv: s("*SYSVAL"),
            kbdbuf: s("*SYSVAL"),
            jobd: pair("QDFTJOBD", "*LIBL"),
            grpprf: s("*NONE"),
            grpaut: s("*NONE"),
            acgcde: String::new(),
            dlvry: s("*NOTIFY"),
            msgq: pair("*USRPRF", ""),
            prtdev: s("*WRKSTN"),
            outq: pair("*WRKSTN", ""),
            atnpgm: pair("*SYSVAL", ""),
            srtseq: pair("*SYSVAL", ""),
            langid: s("*SYSVAL"),
            cntryid: s("*SYSVAL"),
            chridctl: s("*SYSVAL"),
            eimassoc: s("*NOCHG"),
            authmth: s("*REGFAC"),
            docpwd: s("*NONE"),
            text: String::new(),
            supgrpprf: Vec::new(),
            locale: s("*SYSVAL"),
            homedir: format!("/home/{}", name.to_ascii_uppercase()),
            setjobatr: vec![s("*SYSVAL")],
            usropt: vec![s("*NONE")],
            audlvl: vec![s("*NONE")],
        }
    }

    /// The group profiles in search order: `GRPPRF`, then `SUPGRPPRF`.
    #[must_use]
    pub fn groups(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.grpprf != "*NONE" && !self.grpprf.is_empty() {
            v.push(self.grpprf.clone());
        }
        v.extend(self.supgrpprf.iter().cloned());
        v
    }

    /// The hash of `form`, if the current password has one.
    #[must_use]
    pub fn hash(&self, form: PwdForm) -> Option<&PwdHash> {
        self.hashes.iter().find(|h| h.form == form)
    }

    /// Encode.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn encode(&self) -> Vec<u8> {
        let mut r = Rec::new(&LAYOUT);
        r.set_bytes("Magic", MAGIC);
        r.set_uint("Version", VERSION);
        r.set_uint("Status", u64::from(self.disabled));
        r.set_uint("UsrCls", u64::from(self.usrcls.code()));
        r.set_uint("SpcAut", u64::from(self.spcaut.0));
        r.set_flag("PwdExp", self.pwdexp);
        r.set_flag("PwdNone", self.pwd_none);
        let forms = self.hashes.iter().fold(0u64, |a, h| {
            a | match h.form {
                PwdForm::Short => 1,
                PwdForm::Long => 2,
            }
        });
        r.set_uint("PwdForms", forms);
        r.set_uint(
            "LmtCpb",
            match self.lmtcpb {
                LmtCpb::No => 0,
                LmtCpb::Partial => 1,
                LmtCpb::Yes => 2,
            },
        );
        r.set_uint("DspSgnInf", u64::from(self.dspsgninf.code()));
        r.set_flag("LclPwdMgt", self.lclpwdmgt);
        r.set_uint("LmtDevSsn", u64::from(self.lmtdevssn.code()));
        r.set_uint("PtyLmt", u64::from(self.ptylmt));
        r.set_uint("PwdExpItv", u64::from(self.pwdexpitv));
        r.set_uint("PwdChgBlk", u64::from(self.pwdchgblk));
        r.set_int("MaxStg", self.maxstg);
        r.set_int("MaxStgLrg", self.maxstglrg);
        r.set_uint("StgUsed", self.stg_used);
        r.set_flag("Owner", self.owner_grpprf);
        r.set_flag("GrpAutTyp", self.grpauttyp_pgp);
        r.set_uint("Sev", u64::from(self.sev));
        r.set_uint(
            "ObjAud",
            match self.objaud.as_str() {
                "*CHANGE" => 1,
                "*ALL" => 2,
                _ => 0,
            },
        );
        r.set_uint("Ccsid", u64::from(self.ccsid));
        r.set_uint("Uid", u64::from(self.uid));
        r.set_uint("Gid", u64::from(self.gid));
        r.set_uint("UsrExpItv", u64::from(self.usrexpitv));
        r.set_uint("SignOnInvalid", u64::from(self.signon_invalid));
        r.set_uint("Created", self.created);
        r.set_uint("Changed", self.changed);
        r.set_uint("LastSignOn", self.last_signon);
        r.set_uint("PrevSignOn", self.prev_signon);
        r.set_uint("PwdChanged", self.pwd_changed);
        r.set_uint("UsrExpDate", self.usrexpdate);
        r.set_uint("OwnedCount", self.owned_count);
        for (f, v) in [
            ("Name", &self.name),
            ("AstLvl", &self.astlvl),
            ("CurLib", &self.curlib),
            ("InlPgm", &self.inlpgm.0),
            ("InlPgmLib", &self.inlpgm.1),
            ("InlMnu", &self.inlmnu.0),
            ("InlMnuLib", &self.inlmnu.1),
            ("SpcEnv", &self.spcenv),
            ("KbdBuf", &self.kbdbuf),
            ("JobD", &self.jobd.0),
            ("JobDLib", &self.jobd.1),
            ("GrpPrf", &self.grpprf),
            ("GrpAut", &self.grpaut),
            ("AcgCde", &self.acgcde),
            ("Dlvry", &self.dlvry),
            ("MsgQ", &self.msgq.0),
            ("MsgQLib", &self.msgq.1),
            ("PrtDev", &self.prtdev),
            ("OutQ", &self.outq.0),
            ("OutQLib", &self.outq.1),
            ("AtnPgm", &self.atnpgm.0),
            ("AtnPgmLib", &self.atnpgm.1),
            ("SrtSeq", &self.srtseq.0),
            ("SrtSeqLib", &self.srtseq.1),
            ("LangId", &self.langid),
            ("CntryId", &self.cntryid),
            ("ChrIdCtl", &self.chridctl),
            ("EimAssoc", &self.eimassoc),
            ("AuthMth", &self.authmth),
            ("DocPwd", &self.docpwd),
            ("Text", &self.text),
        ] {
            r.set_text(f, v);
        }
        let mut t = Tail::default();
        for g in &self.supgrpprf {
            t.put_str(tag::SUPGRP, g);
        }
        t.put_str(tag::LOCALE, &self.locale);
        t.put_str(tag::HOMEDIR, &self.homedir);
        for v in &self.setjobatr {
            t.put_str(tag::SETJOBATR, v);
        }
        for v in &self.usropt {
            t.put_str(tag::USROPT, v);
        }
        for v in &self.audlvl {
            t.put_str(tag::AUDLVL, v);
        }
        for h in &self.hashes {
            t.put(tag::PASSWORD, &h.encode());
        }
        for h in &self.history {
            t.put(tag::HISTORY, &h.encode());
        }
        r.buf.extend(t.encode());
        r.buf
    }

    /// Decode.
    #[allow(clippy::too_many_lines)]
    pub fn decode(b: &[u8]) -> Result<Self> {
        let r = Rec::read(&LAYOUT, b)?;
        if r.bytes("Magic") != MAGIC || r.uint("Version") != VERSION {
            return Err(SecError::Corrupt("not a user profile payload".into()));
        }
        let t = Tail::decode(r.tail())?;
        let u = |f: &str| u32::try_from(r.uint(f)).unwrap_or(u32::MAX);
        let byte = |f: &str| u8::try_from(r.uint(f)).unwrap_or(0);
        let pair = |a: &str, b: &str| (r.text(a), r.text(b));
        let hashes = t
            .all(tag::PASSWORD)
            .map(PwdHash::decode)
            .collect::<Result<_>>()?;
        let history = t
            .all(tag::HISTORY)
            .map(PwdHash::decode)
            .collect::<Result<_>>()?;
        Ok(Self {
            name: r.text("Name"),
            disabled: r.flag("Status"),
            usrcls: UserClass::from_code(byte("UsrCls"))
                .ok_or_else(|| SecError::Corrupt("user class".into()))?,
            spcaut: SpecialAuth(u("SpcAut")),
            pwdexp: r.flag("PwdExp"),
            pwd_none: r.flag("PwdNone"),
            hashes,
            history,
            lmtcpb: match r.uint("LmtCpb") {
                1 => LmtCpb::Partial,
                2 => LmtCpb::Yes,
                _ => LmtCpb::No,
            },
            dspsgninf: Tri::from_code(byte("DspSgnInf")),
            lclpwdmgt: r.flag("LclPwdMgt"),
            lmtdevssn: Tri::from_code(byte("LmtDevSsn")),
            ptylmt: byte("PtyLmt"),
            pwdexpitv: u("PwdExpItv"),
            pwdchgblk: u("PwdChgBlk"),
            maxstg: r.int("MaxStg"),
            maxstglrg: r.int("MaxStgLrg"),
            stg_used: r.uint("StgUsed"),
            owner_grpprf: r.flag("Owner"),
            grpauttyp_pgp: r.flag("GrpAutTyp"),
            sev: byte("Sev"),
            objaud: match r.uint("ObjAud") {
                1 => "*CHANGE",
                2 => "*ALL",
                _ => "*NONE",
            }
            .into(),
            ccsid: u("Ccsid"),
            uid: u("Uid"),
            gid: u("Gid"),
            usrexpitv: u("UsrExpItv"),
            signon_invalid: u("SignOnInvalid"),
            created: r.uint("Created"),
            changed: r.uint("Changed"),
            last_signon: r.uint("LastSignOn"),
            prev_signon: r.uint("PrevSignOn"),
            pwd_changed: r.uint("PwdChanged"),
            usrexpdate: r.uint("UsrExpDate"),
            owned_count: r.uint("OwnedCount"),
            astlvl: r.text("AstLvl"),
            curlib: r.text("CurLib"),
            inlpgm: pair("InlPgm", "InlPgmLib"),
            inlmnu: pair("InlMnu", "InlMnuLib"),
            spcenv: r.text("SpcEnv"),
            kbdbuf: r.text("KbdBuf"),
            jobd: pair("JobD", "JobDLib"),
            grpprf: r.text("GrpPrf"),
            grpaut: r.text("GrpAut"),
            acgcde: r.text("AcgCde"),
            dlvry: r.text("Dlvry"),
            msgq: pair("MsgQ", "MsgQLib"),
            prtdev: r.text("PrtDev"),
            outq: pair("OutQ", "OutQLib"),
            atnpgm: pair("AtnPgm", "AtnPgmLib"),
            srtseq: pair("SrtSeq", "SrtSeqLib"),
            langid: r.text("LangId"),
            cntryid: r.text("CntryId"),
            chridctl: r.text("ChrIdCtl"),
            eimassoc: r.text("EimAssoc"),
            authmth: r.text("AuthMth"),
            docpwd: r.text("DocPwd"),
            text: r.text("Text"),
            supgrpprf: t.strs(tag::SUPGRP),
            locale: t.str(tag::LOCALE),
            homedir: t.str(tag::HOMEDIR),
            setjobatr: t.strs(tag::SETJOBATR),
            usropt: t.strs(tag::USROPT),
            audlvl: t.strs(tag::AUDLVL),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut p = UsrPrf::new("bob", 40);
        p.usrcls = UserClass::Pgmr;
        p.spcaut = SpecialAuth::JOBCTL;
        p.pwd_none = false;
        p.hashes.push(PwdHash {
            form: PwdForm::Long,
            phc: "$argon2id$v=19$x".into(),
        });
        p.history.push(PwdHash {
            form: PwdForm::Short,
            phc: "$argon2id$v=19$y".into(),
        });
        p.supgrpprf = vec!["DEVS".into(), "OPS".into()];
        p.maxstg = 5000;
        p.pwdexpitv = NOMAX;
        p.text = "Bob the programmer".into();
        p.objaud = "*ALL".into();
        p.audlvl = vec!["*CMD".into(), "*CREATE".into()];
        let back = UsrPrf::decode(&p.encode()).unwrap();
        assert_eq!(back, p);
        assert_eq!(back.name, "BOB");
        assert_eq!(back.groups(), vec!["DEVS", "OPS"]);
        assert!(UsrPrf::decode(&[0; 600]).is_err());
    }
}
