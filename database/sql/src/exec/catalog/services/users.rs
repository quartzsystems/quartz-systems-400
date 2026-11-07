// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `USER_INFO`, `USER_INFO_BASIC` and `GROUP_PROFILE_ENTRIES`: the user
//! profiles as the security layer stores them (`sec::profile`). A
//! profile the asking user may not read is left out, as on the machine.

use sec::profile::{self, UsrPrf};
use store::obj::ops;
use store::txn::Txn;

use super::super::super::SqlContext;
use super::Out;
use crate::error::Result;

/// `NOT NULL` columns QS/400 has no value for.
pub const NO_VALUE: &[&str] = &[
    "NETSERVER_DISABLED",
    "AUTHORITY_COLLECTION_ACTIVE",
    "AUTHORITY_COLLECTION_REPOSITORY_EXISTS",
];

/// Every profile the user can read, by name.
fn profiles(txn: &mut Txn<'_>, ctx: &SqlContext) -> Vec<(store::obj::ObjectId, UsrPrf)> {
    let names = profile::objects::list(txn, ctx.qsys).unwrap_or_default();
    let mut out: Vec<_> = names
        .iter()
        .filter_map(|n| profile::objects::read(txn, ctx.qsys, n).ok())
        .collect();
    out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
    out
}

/// `name` qualified by `lib`, as separate columns; `*LIBL` and blank
/// libraries are null.
fn qualified(out: &mut Out, name_col: &str, lib_col: &str, q: &(String, String)) {
    out.text_or_null(name_col, &q.0);
    if !q.1.trim().is_empty() && q.1 != "*LIBL" {
        out.text(lib_col, q.1.as_str());
    }
}

fn tri(t: profile::Tri) -> &'static str {
    t.name()
}

/// Names in ten-character columns, as IBM lists them.
fn columns<S: AsRef<str>>(names: &[S]) -> String {
    names
        .iter()
        .map(|s| format!("{:<10}", s.as_ref()))
        .collect::<Vec<_>>()
        .concat()
        .trim_end()
        .to_string()
}

/// Unix seconds as a timestamp; days count in seconds too.
const DAY: u64 = 86_400;

/// The rows of `USER_INFO` (`basic`: `USER_INFO_BASIC`, the same rows
/// with fewer columns).
#[allow(clippy::too_many_lines, clippy::unnecessary_wraps)] // one column a line
pub fn info(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let now = ctx.clock();
    for (id, p) in profiles(txn, ctx) {
        out.row();
        out.text("AUTHORIZATION_NAME", p.name.as_str());
        out.stamp("PREVIOUS_SIGNON", p.last_signon);
        out.int("SIGN_ON_ATTEMPTS_NOT_VALID", p.signon_invalid);
        out.text("MAXIMUM_SIGN_ON_ATTEMPTS", "*SYSVAL");
        out.text("STATUS", if p.disabled { "*DISABLED" } else { "*ENABLED" });
        out.stamp("PASSWORD_CHANGE_DATE", p.pwd_changed);
        out.yes("NO_PASSWORD_INDICATOR", p.pwd_none);
        let has = |f: profile::PwdForm| p.hash(f).is_some();
        out.yes("PASSWORD_LEVEL_0_1", has(profile::PwdForm::Short));
        // Levels 2 to 4 share the long form's hash here.
        out.yes("PASSWORD_LEVEL_2_3", has(profile::PwdForm::Long));
        out.yes("PASSWORD_LEVEL_4", has(profile::PwdForm::Long));
        match p.pwdexpitv {
            profile::payload::SYSVAL => out.int("PASSWORD_EXPIRATION_INTERVAL", 0),
            profile::payload::NOMAX => out.int("PASSWORD_EXPIRATION_INTERVAL", -1),
            n => {
                out.int("PASSWORD_EXPIRATION_INTERVAL", n);
                let expires = p.pwd_changed + u64::from(n) * DAY;
                out.stamp("DATE_PASSWORD_EXPIRES", expires - expires % DAY);
                out.int(
                    "DAYS_UNTIL_PASSWORD_EXPIRES",
                    (expires.saturating_sub(now)) / DAY,
                );
            }
        }
        out.yes("SET_PASSWORD_TO_EXPIRE", p.pwdexp);
        out.text("USER_CLASS_NAME", p.usrcls.name());
        let sp = p.spcaut.names();
        if !sp.is_empty() {
            out.text("SPECIAL_AUTHORITIES", columns(&sp));
        }
        out.text(
            "GROUP_PROFILE_NAME",
            if p.grpprf.trim().is_empty() {
                "*NONE"
            } else {
                p.grpprf.as_str()
            },
        );
        out.int("SUPPLEMENTAL_GROUP_COUNT", p.supgrpprf.len());
        if !p.supgrpprf.is_empty() {
            out.text("SUPPLEMENTAL_GROUP_LIST", columns(&p.supgrpprf));
        }
        out.text("OWNER", if p.owner_grpprf { "*GRPPRF" } else { "*USRPRF" });
        out.text_or_null("GROUP_AUTHORITY", &p.grpaut);
        out.text_or_null("ASSISTANCE_LEVEL", &p.astlvl);
        out.text_or_null("CURRENT_LIBRARY_NAME", &p.curlib);
        qualified(
            out,
            "INITIAL_MENU_NAME",
            "INITIAL_MENU_LIBRARY_NAME",
            &p.inlmnu,
        );
        qualified(
            out,
            "INITIAL_PROGRAM_NAME",
            "INITIAL_PROGRAM_LIBRARY_NAME",
            &p.inlpgm,
        );
        out.text("LIMIT_CAPABILITIES", p.lmtcpb.name());
        out.text_or_null("TEXT_DESCRIPTION", &p.text);
        out.text("DISPLAY_SIGNON_INFORMATION", tri(p.dspsgninf));
        out.text("LIMIT_DEVICE_SESSIONS", tri(p.lmtdevssn));
        out.text_or_null("KEYBOARD_BUFFERING", &p.kbdbuf);
        if p.maxstg >= 0 {
            out.int("MAXIMUM_ALLOWED_STORAGE", p.maxstg);
        } else {
            out.int("MAXIMUM_ALLOWED_STORAGE", -1);
        }
        out.int("STORAGE_USED", p.stg_used);
        out.text("HIGHEST_SCHEDULING_PRIORITY", p.ptylmt.to_string());
        qualified(
            out,
            "JOB_DESCRIPTION_NAME",
            "JOB_DESCRIPTION_LIBRARY_NAME",
            &p.jobd,
        );
        out.text_or_null("ACCOUNTING_CODE", &p.acgcde);
        let msgq = if p.msgq.0 == "*USRPRF" {
            (p.name.clone(), "QUSRSYS".to_string())
        } else {
            p.msgq.clone()
        };
        qualified(
            out,
            "MESSAGE_QUEUE_NAME",
            "MESSAGE_QUEUE_LIBRARY_NAME",
            &msgq,
        );
        out.text_or_null("MESSAGE_QUEUE_DELIVERY_METHOD", &p.dlvry);
        out.int("MESSAGE_QUEUE_SEVERITY", p.sev);
        qualified(
            out,
            "OUTPUT_QUEUE_NAME",
            "OUTPUT_QUEUE_LIBRARY_NAME",
            &p.outq,
        );
        out.text_or_null("PRINT_DEVICE", &p.prtdev);
        out.text_or_null("SPECIAL_ENVIRONMENT", &p.spcenv);
        qualified(
            out,
            "ATTENTION_KEY_HANDLING_PROGRAM_NAME",
            "ATTENTION_KEY_HANDLING_PROGRAM_LIBRARY_NAME",
            &p.atnpgm,
        );
        out.text_or_null("LANGUAGE_ID", &p.langid);
        out.text_or_null("COUNTRY_OR_REGION_ID", &p.cntryid);
        out.text(
            "CHARACTER_CODE_SET_ID",
            if p.ccsid == 0 {
                "QCCSID".to_string()
            } else {
                p.ccsid.to_string()
            },
        );
        if !p.usropt.is_empty() {
            out.text("USER_OPTIONS", columns(&p.usropt));
        }
        qualified(
            out,
            "SORT_SEQUENCE_TABLE_NAME",
            "SORT_SEQUENCE_TABLE_LIBRARY_NAME",
            &p.srtseq,
        );
        out.text_or_null("OBJECT_AUDITING_VALUE", &p.objaud);
        if !p.audlvl.is_empty() {
            out.text("USER_ACTION_AUDIT_LEVEL", columns(&p.audlvl));
        }
        out.text(
            "GROUP_AUTHORITY_TYPE",
            if p.grpauttyp_pgp { "*PGP" } else { "*PRIVATE" },
        );
        out.int("USER_ID_NUMBER", p.uid);
        out.int("GROUP_ID_NUMBER", p.gid);
        if !p.setjobatr.is_empty() {
            out.text("LOCALE_JOB_ATTRIBUTES", p.setjobatr.join(" "));
        }
        let member = !profile::groups::members(txn, ctx.qsys, &p.name)
            .unwrap_or_default()
            .is_empty();
        out.yes("GROUP_MEMBER_INDICATOR", member);
        out.yes("DIGITAL_CERTIFICATE_INDICATOR", false);
        out.text_or_null("CHARACTER_IDENTIFIER_CONTROL", &p.chridctl);
        out.yes("LOCAL_PASSWORD_MANAGEMENT", p.lclpwdmgt);
        match p.pwdchgblk {
            profile::payload::SYSVAL => out.text("BLOCK_PASSWORD_CHANGE", "*SYSVAL"),
            profile::payload::NOMAX => out.text("BLOCK_PASSWORD_CHANGE", "*NONE"),
            n => out.text("BLOCK_PASSWORD_CHANGE", n.to_string()),
        }
        out.int("USER_EXPIRATION_INTERVAL", p.usrexpitv);
        out.stamp("USER_EXPIRATION_DATE", p.usrexpdate);
        out.text(
            "USER_EXPIRATION_ACTION",
            if p.usrexpdate == 0 && p.usrexpitv == 0 {
                "*NONE"
            } else {
                "*DISABLE"
            },
        );
        out.text_or_null("HOME_DIRECTORY", &p.homedir);
        out.text_or_null("LOCALE_PATH_NAME", &p.locale);
        if let Ok((_, h)) = ops::read_header(txn, id) {
            out.text_or_null("USER_OWNER", &ccsid::decode_trimmed(&h.owner));
            out.text_or_null("USER_CREATOR", &ccsid::decode_trimmed(&h.creator));
            out.int("SIZE", (h.size_pages + 1) * 4096);
            out.stamp("CREATION_TIMESTAMP", h.created);
            out.stamp("LAST_USED_TIMESTAMP", h.last_used);
            out.int("DAYS_USED_COUNT", h.days_used);
            out.stamp("LAST_RESET_TIMESTAMP", h.reset_date);
        }
    }
    Ok(())
}

/// `GROUP_PROFILE_ENTRIES`: each group's members, primary and
/// supplemental.
#[allow(clippy::unnecessary_wraps)] // a view's rows may fail
pub fn groups(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let all = profiles(txn, ctx);
    let mut pairs: Vec<(String, String, String)> = Vec::new();
    for (_, p) in &all {
        for g in p.groups() {
            pairs.push((g, p.name.clone(), p.text.clone()));
        }
    }
    pairs.sort();
    for (g, u, text) in pairs {
        out.row();
        out.text("GROUP_PROFILE_NAME", g);
        out.text("USER_PROFILE_NAME", u);
        out.text_or_null("USER_TEXT", &text);
    }
    Ok(())
}
