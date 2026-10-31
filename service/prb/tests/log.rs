// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The problem log through its life: detected, analyzed, noted,
//! reported, closed, deleted, and stored.

use prb::analyze::{self, Answers, Area};
use prb::{Filter, PrbStatus, PrbType, ProblemLog};

const NOW: u64 = 1_793_318_400;

#[test]
fn a_problem_from_detection_to_deletion() {
    let mut log = ProblemLog::default();
    assert!(analyze::from_message("CPF0001", 40, "ordinary").is_none());
    let d = analyze::from_message("CPF8136", 99, "A file is damaged").unwrap();
    let id = log.create(PrbType::Machine, &d, "QS400", NOW);
    assert_eq!(id, "2630300000");
    // The same failure again is a note on the open problem.
    let again = log.create(PrbType::Machine, &d, "QS400", NOW + 5);
    assert_eq!(again, id);
    assert_eq!(log.get(&id).unwrap().notes.len(), 1);
    let h = analyze::from_hardware("TAP01", "3FFF4A2B", true);
    let hw = log.create(PrbType::Machine, &h, "QS400", NOW + 10);
    let user = analyze::guided(&Answers {
        area: Area::Communications,
        msgid: "TCP2617".into(),
        object: "ETHLINE".into(),
        stops_work: true,
        description: "Users cannot reach the system".into(),
    });
    assert_eq!(user.symptoms, "LIND/ETHLINE MSG/TCP2617");
    assert_eq!(user.severity, 1);
    let u = log.create(PrbType::User, &user, "QS400", NOW + 20);
    // WRKPRB views: all, newest first; hardware only.
    let all: Vec<&str> = log.list(&Filter::default()).iter().map(|p| p.id.as_str()).collect();
    assert_eq!(all, [u.as_str(), hw.as_str(), id.as_str()]);
    let hwonly = log.list(&Filter {
        hardware: true,
        ..Filter::default()
    });
    assert_eq!(hwonly.len(), 1);
    // Report, answer, close.
    let report = log.prepare(&u, NOW + 30).unwrap();
    assert!(report.contains("LIND/ETHLINE"));
    assert_eq!(log.get(&u).unwrap().status, PrbStatus::Prepared);
    log.get_mut(&u).unwrap().status = PrbStatus::Closed;
    // Stored and read back.
    let back = ProblemLog::decode(&log.encode());
    assert_eq!(back, log);
    // DLTPRB STATUS(*CLOSED).
    let n = log.delete(&Filter {
        status: vec![PrbStatus::Closed],
        ..Filter::default()
    });
    assert_eq!(n, 1);
    assert_eq!(log.problems.len(), 2);
}

#[test]
fn a_report_is_kept_without_a_provider() {
    let mut log = ProblemLog::default();
    let d = analyze::from_dump("QSD0000001", "System dump taken");
    let id = log.create(PrbType::Machine, &d, "QS400", NOW);
    assert!(log.analyzed(&id, NOW + 1));
    assert_eq!(log.get(&id).unwrap().status, PrbStatus::Ready);
    let r = log.report(&id, &mut prb::Unconnected, NOW + 2).unwrap();
    assert!(matches!(r, prb::Reported::Kept(_)));
    assert_eq!(log.get(&id).unwrap().status, PrbStatus::Prepared);
    let back = ProblemLog::decode(&log.encode());
    assert!(back.reports[&id].contains("DUMP/QSD0000001"));
    assert_eq!(back, log);
}
