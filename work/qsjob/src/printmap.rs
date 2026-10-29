// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the print writer hands the `print` crate: a destination the
//! service described ([`WtrDest`]) as a [`Target`], and a spooled file's
//! attributes as a [`PrintJob`]. Pure, so it is tested on any host.

use std::sync::Arc;

use print::job::{Duplex, PrintJob};
use print::sink::TlsConnector;
use print::sink::lpr::DestType;
use print::transform::Transform;
use print::wscst::Wscst;
use print::{Driver, Target};
use wm::obj::splf::SplfAttrs;
use wm::wtr::WtrDest;

/// The transform `TRANSFORM` and `MFRTYPMDL` choose; a `*WSCST` model
/// takes the object's payload.
fn transform(on: bool, mfrtypmdl: &str, wscst: &[u8]) -> Transform {
    let w = if wscst.is_empty() {
        None
    } else {
        Wscst::decode(wscst).ok()
    };
    Transform::select(on, mfrtypmdl, w)
}

/// Where a destination prints through the `print` crate; `None` for the
/// capture device.
pub fn target_of(dest: &WtrDest) -> Option<Target> {
    match dest {
        WtrDest::Capture => None,
        WtrDest::Lan {
            host,
            port,
            driver,
            secure,
            transform: on,
            mfrtypmdl,
            wscst,
            ..
        } => Some(Target::Lan {
            host: host.clone(),
            port: *port,
            driver: Driver::parse(driver).unwrap_or_default(),
            secure: *secure,
            transform: transform(*on, mfrtypmdl, wscst),
        }),
        WtrDest::Remote {
            host,
            queue,
            desttype,
            destopt,
            seppage,
            transform: on,
            mfrtypmdl,
            wscst,
            local_host,
        } => Some(Target::Remote {
            host: host.clone(),
            port: 0,
            queue: queue.clone(),
            desttype: DestType::from_code(*desttype),
            destopt: destopt.clone(),
            seppage: *seppage,
            transform: transform(*on, mfrtypmdl, wscst),
            local_host: local_host.clone(),
        }),
    }
}

/// The file as the printer side sees it.
pub fn job_of(attrs: &SplfAttrs, dest: &WtrDest) -> PrintJob {
    let (chrid, system) = match dest {
        WtrDest::Lan { chrid, .. } => (chrid.clone(), "QS400".to_string()),
        WtrDest::Remote { local_host, .. } => ("*DEVD".to_string(), local_host.clone()),
        WtrDest::Capture => ("*DEVD".to_string(), "QS400".to_string()),
    };
    PrintJob {
        file: attrs.name.clone(),
        number: attrs.number,
        job: attrs.job.clone(),
        system,
        usrdta: attrs.usrdta.clone(),
        formtype: attrs.formtype.clone(),
        copies: attrs.copies_left.max(attrs.copies).max(1),
        pages: attrs.pages,
        page_len: attrs.page_len,
        page_width: attrs.page_width,
        lpi: attrs.lpi,
        cpi: attrs.cpi,
        ovrflw: attrs.ovrflw,
        drawer: attrs.drawer,
        duplex: Duplex::from_code(attrs.duplex),
        outpty: attrs.outpty,
        font: if attrs.font.is_empty() {
            "*CPI".into()
        } else {
            attrs.font.clone()
        },
        chrid: if chrid == "*SYSVAL" { "*DEVD".into() } else { chrid },
        prtqlty: ["*STD", "*DRAFT", "*NLQ", "*DEVD"]
            .get(usize::from(attrs.prtqlty))
            .unwrap_or(&"*STD")
            .to_string(),
        align: attrs.align,
        devtype: if attrs.devtype.is_empty() {
            "*SCS".into()
        } else {
            attrs.devtype.clone()
        },
        prtf: attrs.prtf.clone(),
        usrdfndta: attrs.usrdfndta.clone(),
        save: attrs.save,
    }
}

/// Print one file at a network destination; on failure, the message
/// the writer sends and whether the file is held.
pub fn send_network(
    target: &Target,
    attrs: &SplfAttrs,
    dest: &WtrDest,
    data: &[u8],
    tls: Arc<dyn TlsConnector>,
) -> Result<String, (String, String, bool)> {
    let job = job_of(attrs, dest);
    match print::deliver(target, &job, data, tls) {
        Ok(d) => Ok(d.receipt.detail),
        Err(e) => Err((e.kind.msgid().to_string(), e.detail, !e.kind.retryable())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use print::sink::{Receipt, Sink, SinkError, SinkErrorKind};
    use print::transform::{Model, Output};

    /// A sink that keeps what it is given, or fails.
    struct Fake {
        got: Vec<(PrintJob, Output)>,
        fail: Option<SinkErrorKind>,
    }

    impl Sink for Fake {
        fn send(&mut self, job: &PrintJob, doc: &Output) -> Result<Receipt, SinkError> {
            if let Some(k) = self.fail {
                return Err(SinkError::new(k, "fake"));
            }
            self.got.push((job.clone(), doc.clone()));
            Ok(Receipt::default())
        }
    }

    fn attrs() -> SplfAttrs {
        SplfAttrs {
            id: 7,
            number: 2,
            name: "QPRINT".into(),
            job: ("REPORTS".into(), "ANNE".into(), 42),
            copies: 2,
            copies_left: 2,
            page_len: 66,
            page_width: 80,
            lpi: 6,
            cpi: 10,
            ovrflw: 60,
            drawer: 1,
            duplex: 1,
            ..SplfAttrs::default()
        }
    }

    fn scs() -> Vec<u8> {
        let mut p = spool::Printer::new(spool::PageFormat {
            width: 80,
            ..spool::PageFormat::default()
        });
        p.write_line(&ccsid::to_ebcdic("WRITER TEST").unwrap());
        p.take()
    }

    #[test]
    fn lan_printer_through_a_fake_sink() {
        let dest = WtrDest::Lan {
            host: "lj4".into(),
            port: 9100,
            driver: "*HPPJLDRV".into(),
            secure: false,
            transform: true,
            mfrtypmdl: "*HP4".into(),
            wscst: Vec::new(),
            chrid: "697 850".into(),
        };
        let t = target_of(&dest).unwrap();
        assert_eq!(t.transform(), Transform::Pcl(Model::Hp4));
        let mut f = Fake {
            got: Vec::new(),
            fail: None,
        };
        let job = job_of(&attrs(), &dest);
        print::deliver::deliver_with(&mut f, &t.transform(), &job, &scs()).unwrap();
        let (j, doc) = &f.got[0];
        assert_eq!((j.copies, j.duplex, j.chrid.as_str()), (2, Duplex::Yes, "697 850"));
        assert!(String::from_utf8_lossy(&doc.data).contains("WRITER TEST\r\n"));
        assert!(doc.data.starts_with(b"\x1bE"));
    }

    #[test]
    fn remote_queue_and_failures() {
        let w = Wscst::parse(Wscst::sample_pcl()).unwrap();
        let dest = WtrDest::Remote {
            host: "10.0.0.9".into(),
            queue: String::new(),
            desttype: 0,
            destopt: "XAIX".into(),
            seppage: false,
            transform: true,
            mfrtypmdl: "*WSCST".into(),
            wscst: w.encode(),
            local_host: "QS400A".into(),
        };
        let t = target_of(&dest).unwrap();
        assert!(matches!(t.transform(), Transform::Custom(_)));
        let mut f = Fake {
            got: Vec::new(),
            fail: Some(SinkErrorKind::Refused),
        };
        let job = job_of(&attrs(), &dest);
        assert_eq!(job.system, "QS400A");
        let e = print::deliver::deliver_with(&mut f, &t.transform(), &job, &scs()).unwrap_err();
        assert_eq!((e.kind.msgid(), e.kind.retryable()), ("TCP3427", true));
        assert!(target_of(&WtrDest::Capture).is_none());
    }

    #[test]
    fn sends_to_a_raw_port_and_reports_a_dead_one() {
        use std::io::Read;
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut got = Vec::new();
            s.read_to_end(&mut got).unwrap();
            got
        });
        let dest = WtrDest::Lan {
            host: "127.0.0.1".into(),
            port,
            driver: "*IBMSNMPDRV".into(),
            secure: false,
            transform: true,
            mfrtypmdl: "*NONE".into(),
            wscst: Vec::new(),
            chrid: "*SYSVAL".into(),
        };
        let tls: Arc<dyn TlsConnector> = Arc::new(print::sink::PlainConnector);
        let t = target_of(&dest).unwrap();
        send_network(&t, &attrs(), &dest, &scs(), tls.clone()).unwrap();
        assert_eq!(server.join().unwrap(), b"WRITER TEST\r\n\x0c");
        // Nobody listens now: the writer gets the message and waits.
        let (msgid, _, hold) = send_network(&t, &attrs(), &dest, &scs(), tls).unwrap_err();
        assert_eq!((msgid.as_str(), hold), ("TCP3427", false));
    }
}
