// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample audit journal: 762 entries on 10/29/26, the prototype's 14
//! newest, then prestart job starts back to midnight.

use std::sync::Mutex;

use qsweb::msg::Message;

use crate::env::sec::{AuditEntry, AuditFilter, AuditReceiver, Security};
use crate::util::secs;

/// The sample system's audit journal.
pub struct FixtureSecurity {
    /// The entries, newest first.
    pub entries: Mutex<Vec<AuditEntry>>,
}

impl Default for FixtureSecurity {
    fn default() -> Self {
        Self {
            entries: Mutex::new(entries()),
        }
    }
}

/// The prototype's entries: time, type, user, job, detail.
const NEWEST: &[(u32, u32, u32, &str, &str, &str, &str)] = &[
    (14, 2, 17, "AF", "WEBUSER", "123457/QTMHHTTP/ADMIN", "ADMIN       QSYS       *JOB     Violation A  (not authorized to object)"),
    (13, 50, 1, "PW", "QSECOFR", "123200/QSYS/QINTER", "Sign-on     QPADEV0001 Status P  (password valid)"),
    (13, 5, 0, "CO", "CWELLMAN", "123488/CWELLMAN/QPADEV0003", "INVLOAD     HLINV      *PGM     Create  (replace)"),
    (12, 10, 33, "AF", "QTMHHTTP", "123457/QTMHHTTP/ADMIN", "WEBUSER     QSYS       *USRPRF  Violation A"),
    (11, 42, 17, "AF", "WEBUSER", "123456/QTMHHTTP/ADMIN", "/qs400/httpadmin/instance/WEBSTORE   Violation A"),
    (11, 0, 0, "CP", "QSYS", "123200/QSYS/QINTER", "GUEST       QSYS       *USRPRF  Change  STATUS(*DISABLED)"),
    (10, 59, 58, "PW", "GUEST", "123200/QSYS/QINTER", "Sign-on     QPADEV0007 Status U  (password not valid)"),
    (10, 59, 51, "PW", "GUEST", "123200/QSYS/QINTER", "Sign-on     QPADEV0007 Status U  (password not valid)"),
    (10, 59, 44, "PW", "GUEST", "123200/QSYS/QINTER", "Sign-on     QPADEV0007 Status U  (password not valid)"),
    (9, 30, 12, "SV", "QSECOFR", "123460/QSECOFR/QPADEV0001", "QINACTITV   System value changed  Old 30  New 60"),
    (9, 0, 2, "ST", "QSECOFR", "123350/QSECOFR/SAVDAILY", "SAVLIB      QGPL       Service tools / save"),
    (8, 2, 33, "PW", "JDOE", "123200/QSYS/QINTER", "Sign-on     QPADEV0005 Status P"),
    (6, 0, 1, "JS", "QTMHHTTP", "123456/QTMHHTTP/ADMIN", "Job start   ADMIN      QHTTPSVR"),
    (5, 58, 40, "CA", "QSECOFR", "123001/QSYS/SCPF", "INVDATA     QSYS       *AUTL    Authority change  WAREHOUSE *CHANGE"),
];

/// Every entry, newest first.
pub fn entries() -> Vec<AuditEntry> {
    let mut out: Vec<AuditEntry> = NEWEST
        .iter()
        .map(|(h, m, s, k, u, j, d)| AuditEntry {
            at: secs(2026, 10, 29, *h, *m, *s),
            kind: (*k).into(),
            user: (*u).into(),
            job: (*j).into(),
            detail: (*d).into(),
        })
        .collect();
    let last = secs(2026, 10, 29, 5, 58, 40);
    for i in 0..748u64 {
        let n = 122_999 - i;
        out.push(AuditEntry {
            at: last - 28 * (i + 1),
            kind: "JS".into(),
            user: "QUSER".into(),
            job: format!("{n}/QUSER/QZDASOINIT"),
            detail: "Job start   QZDASOINIT QUSRWRK".into(),
        });
    }
    out
}

impl Security for FixtureSecurity {
    fn audit_entries(&self, _user: &str, f: &AuditFilter) -> Result<Vec<AuditEntry>, Message> {
        Ok(self
            .entries
            .lock()
            .expect("entries")
            .iter()
            .filter(|e| f.kind.is_empty() || e.kind == f.kind)
            .filter(|e| f.user == "*ALL" || f.user.is_empty() || e.user == f.user)
            .filter(|e| e.at >= f.from && e.at <= f.to)
            .cloned()
            .collect())
    }

    fn audit_receiver(&self, _user: &str) -> AuditReceiver {
        AuditReceiver {
            journal: "QSYS/QAUDJRN".into(),
            receiver: "AUDRCV0412".into(),
            attached: secs(2026, 10, 27, 5, 58, 12),
        }
    }
}
