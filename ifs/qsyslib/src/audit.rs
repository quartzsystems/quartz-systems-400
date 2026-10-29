// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IFS audit events as `QAUDJRN` entries: each event becomes a
//! `sec::audit::Event` of the matching type with the IFS fields (the
//! file ID and the path), recorded for the request; the storage
//! service writes what `QAUDCTL` and `QAUDLVL` select after it answers
//! (`docs/SECURITY.md` §7).

use ifs::ops::audit::{Audit, AuditEvent, AuditKind};
use sec::audit::{Event, Kind};

/// The sink.
#[derive(Debug, Clone, Copy, Default)]
pub struct JournalAudit;

/// The last name of a path.
fn last(path: &str) -> &str {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
}

/// An IFS event as the entry the journal gets.
#[must_use]
pub fn entry(e: &AuditEvent) -> Event {
    let kind = match e.kind {
        AuditKind::Read => Kind::Zr,
        AuditKind::Change => Kind::Zc,
        AuditKind::AuthFail => Kind::Af,
        AuditKind::Create => Kind::Co,
        AuditKind::Delete => Kind::Do,
        AuditKind::Move => Kind::Om,
        AuditKind::AuthChange => Kind::Ca,
        AuditKind::Owner => Kind::Ow,
        AuditKind::Group => Kind::Pg,
        AuditKind::AuditChange => Kind::Ad,
    };
    let new_path = (kind == Kind::Om).then_some(e.detail.as_str());
    let ev = Event::new(kind)
        .object(last(&e.path), "", &e.objtype)
        .ifs(e.file_id.to_bytes(), &e.path, new_path)
        .by(&e.user);
    match kind {
        Kind::Af => ev
            .set(
                "VIOL",
                e.violation.map_or("A".to_string(), |c| c.to_string()),
            )
            .set("OPER", e.oper),
        Kind::Zr | Kind::Zc => ev.set("OPER", e.oper),
        Kind::Co => ev.set("ETYP", "N").set("OWNR", &e.user),
        Kind::Do => ev.set("ETYP", "A"),
        Kind::Om => ev.set("ETYP", if e.oper == "rename" { "R" } else { "M" }),
        Kind::Ca => ev
            .set("ETYP", "S")
            .set("CMDN", e.oper)
            .set("USRP", &e.detail),
        Kind::Ow => ev.set("NOWN", &e.detail),
        Kind::Pg => ev.set("NPGP", &e.detail),
        Kind::Ad => ev
            .set("ETYP", "O")
            .set("NVAL", &e.detail)
            .set("CMDN", e.oper),
        _ => ev,
    }
}

impl Audit for JournalAudit {
    fn event(&self, e: AuditEvent) {
        sec::audit::record(entry(&e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ifs::ops::audit::FileId;

    #[test]
    fn events_become_entries() {
        let e = AuditEvent {
            kind: AuditKind::Move,
            user: "BOB".into(),
            path: "/home/bob/a.txt".into(),
            objtype: "*STMF".into(),
            file_id: FileId {
                fs: 1,
                ino: 2,
                generation: 3,
            },
            oper: "rename",
            violation: None,
            detail: "/home/bob/b.txt".into(),
        };
        let ev = entry(&e);
        assert_eq!(ev.kind, Kind::Om);
        assert_eq!(ev.get("ONAM"), "a.txt");
        assert_eq!(ev.get("OTYP"), "*STMF");
        assert_eq!(ev.get("ETYP"), "R");
        assert_eq!(ev.path(), "/home/bob/a.txt");
        assert_eq!(ev.tail[1].1, "/home/bob/b.txt");
        let b = ev.encode();
        let back = Event::decode(Kind::Om, &b).unwrap();
        assert_eq!(back.path(), "/home/bob/a.txt");
        assert_eq!(
            back.get("OFID"),
            "000000010000000000000002".to_string() + "00000003"
        );
    }
}
