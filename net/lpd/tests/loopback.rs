// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `work/print`'s LPR client (what `SNDTCPSPLF` and remote writers use)
//! against this LPD over loopback TCP, into an output queue in memory.

use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use lpd::session::{Job, Limits, Spooler};
use print::job::PrintJob;
use print::sink::PlainConnector;
use print::sink::lpr::DestType;
use print::transform::Transform;
use print::Target;

/// An output queue in memory.
#[derive(Clone, Default)]
struct Outq(Arc<Mutex<Vec<lpd::Spooled>>>);

impl Spooler for Outq {
    fn accepts(&mut self, queue: &str) -> bool {
        queue.eq_ignore_ascii_case("PRT01")
    }
    fn submit(&mut self, job: Job) -> Result<(), String> {
        self.0.lock().unwrap().extend(lpd::convert::spooled(&job));
        Ok(())
    }
    fn state(&mut self, _: &str, _: bool, _: &[String]) -> String {
        String::new()
    }
    fn remove(&mut self, _: &str, _: &str, _: &[String]) -> String {
        String::new()
    }
}

/// Serve `n` connections on a fresh port.
fn daemon(q: &Outq, n: usize) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    let mut q = q.clone();
    std::thread::spawn(move || {
        for _ in 0..n {
            let (mut s, _) = l.accept().unwrap();
            lpd::serve(&mut s, &mut q, Limits::default()).unwrap();
        }
    });
    port
}

fn scs(text: &str) -> Vec<u8> {
    lpd::convert::text_to_scs(text.as_bytes(), false).0
}

#[test]
fn lpr_into_an_output_queue() {
    let q = Outq::default();
    let port = daemon(&q, 3);
    let job = PrintJob {
        file: "INVOICE".into(),
        job: ("BILLING".into(), "ANNE".into(), 4711),
        copies: 2,
        page_len: 60,
        ..PrintJob::default()
    };
    let data = scs("Invoice 42\nTotal 17.00\n");
    // Another LPD's way: text, one job per copy.
    let target = |desttype| Target::Remote {
        host: "127.0.0.1".into(),
        port,
        queue: "prt01".into(),
        desttype,
        destopt: String::new(),
        seppage: false,
        transform: Transform::select(true, "*IBM42011", None),
        local_host: "QS400A".into(),
    };
    let other = target(DestType::Other);
    let d = print::deliver(&other, &job, &data, Arc::new(PlainConnector)).unwrap();
    assert_eq!(d.receipt.detail, "queue prt01 on 127.0.0.1, 2 jobs");
    // A QS/400 receiver: SCS and the attributes.
    let ibm = target(DestType::Os400);
    print::deliver(&ibm, &job, &data, Arc::new(PlainConnector)).unwrap();
    let got = q.0.lock().unwrap().clone();
    assert_eq!(got.len(), 3);
    for s in &got[..2] {
        assert_eq!((s.attrs.file.as_str(), s.attrs.devtype.as_str()), ("INVOICE", "*SCS"));
        assert_eq!((s.user.as_str(), s.host.as_str()), ("ANNE", "QS400A"));
        let (pages, _) = spool::scs::decode(&s.data);
        let text = spool::view::text(&pages);
        assert!(text.contains("Invoice 42") && text.contains("Total 17.00"), "{text}");
    }
    let s = &got[2];
    assert_eq!((s.attrs.file.as_str(), s.attrs.copies, s.attrs.page_len), ("INVOICE", 2, 60));
    assert_eq!(s.data, data, "SCS kept as sent");
}

#[test]
fn an_unknown_queue_is_refused() {
    let q = Outq::default();
    let port = daemon(&q, 1);
    let t = Target::Remote {
        host: "127.0.0.1".into(),
        port,
        queue: "NOSUCH".into(),
        desttype: DestType::Other,
        destopt: String::new(),
        seppage: true,
        transform: Transform::Raw,
        local_host: "h".into(),
    };
    let e = print::deliver(&t, &PrintJob::default(), &scs("x"), Arc::new(PlainConnector)).unwrap_err();
    assert!(e.detail.contains("refused"), "{}", e.detail);
    assert!(q.0.lock().unwrap().is_empty());
}
