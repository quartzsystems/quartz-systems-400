// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sinks against in-process fake servers on loopback: a raw port
//! that keeps what it is sent, an IPP printer answering `Print-Job`,
//! and an LPD that acknowledges each step and keeps the files. The LPD
//! checks the RFC 1179 framing and the control file's IBM entries.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use print::deliver::{Driver, Target, deliver};
use print::job::{Duplex, PrintJob};
use print::sink::ipp::{self, Attr, Group, Message, tag};
use print::sink::lpr::DestType;
use print::sink::lpr::control::ControlFile;
use print::sink::{PlainConnector, SinkErrorKind};
use print::transform::{Model, Transform};
use spool::{PageFormat, Printer};

fn scs() -> Vec<u8> {
    let mut p = Printer::new(PageFormat {
        width: 40,
        ..PageFormat::default()
    });
    p.write_line(&ccsid::to_ebcdic("HELLO FROM QS/400").unwrap());
    p.take()
}

fn job() -> PrintJob {
    PrintJob {
        file: "QPRINT".into(),
        number: 3,
        job: ("REPORTS".into(), "ANNE".into(), 4711),
        copies: 2,
        page_width: 40,
        usrdta: "monthly".into(),
        duplex: Duplex::Yes,
        ..PrintJob::default()
    }
}

/// A listener on loopback and its port.
fn listen() -> (TcpListener, u16) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    (l, port)
}

#[test]
fn raw_port_gets_a_pjl_job() {
    let (l, port) = listen();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut got = Vec::new();
        s.read_to_end(&mut got).unwrap();
        tx.send(got).unwrap();
    });
    let t = Target::Lan {
        host: "127.0.0.1".into(),
        port,
        driver: Driver::HpPjl,
        secure: false,
        transform: Transform::Pcl(Model::Hp4),
    };
    let d = deliver(&t, &job(), &scs(), Arc::new(PlainConnector)).unwrap();
    let got = rx.recv().unwrap();
    assert_eq!(d.receipt.bytes, got.len() as u64);
    let text = String::from_utf8_lossy(&got);
    assert!(text.starts_with("\x1b%-12345X@PJL JOB NAME=\"QPRINT.004711.3\"\r\n@PJL SET COPIES=2\r\n@PJL ENTER LANGUAGE=PCL\r\n\x1bE"));
    assert!(text.contains("HELLO FROM QS/400\r\n"));
    assert!(text.ends_with("@PJL EOJ NAME=\"QPRINT.004711.3\"\r\n\x1b%-12345X"));
}

#[test]
fn snmp_driver_sends_bare_data() {
    let (l, port) = listen();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut got = Vec::new();
        s.read_to_end(&mut got).unwrap();
        tx.send(got).unwrap();
    });
    let t = Target::Lan {
        host: "127.0.0.1".into(),
        port,
        driver: Driver::IbmSnmp,
        secure: false,
        transform: Transform::Ascii,
    };
    deliver(&t, &job(), &scs(), Arc::new(PlainConnector)).unwrap();
    assert_eq!(rx.recv().unwrap(), b"HELLO FROM QS/400\r\n\x0c");
}

/// One HTTP request: the request line, the headers and the body.
#[allow(clippy::many_single_char_names)]
fn http_request(s: &mut TcpStream) -> (String, Vec<(String, String)>, Vec<u8>) {
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).unwrap();
    let mut headers = Vec::new();
    let mut len = 0;
    loop {
        let mut h = String::new();
        r.read_line(&mut h).unwrap();
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        let (k, v) = h.split_once(':').unwrap();
        if k.eq_ignore_ascii_case("content-length") {
            len = v.trim().parse().unwrap();
        }
        headers.push((k.to_string(), v.trim().to_string()));
    }
    let mut body = vec![0; len];
    r.read_exact(&mut body).unwrap();
    (line.trim_end().to_string(), headers, body)
}

fn ipp_reply(status: u16, request_id: u32, job_id: i32) -> Vec<u8> {
    let m = Message {
        version: (1, 1),
        code: status,
        request_id,
        groups: vec![
            Group {
                tag: tag::OPERATION,
                attrs: vec![
                    Attr::text(tag::CHARSET, "attributes-charset", "utf-8"),
                    Attr::text(tag::LANGUAGE, "attributes-natural-language", "en"),
                ],
            },
            Group {
                tag: tag::JOB,
                attrs: vec![Attr::int(tag::INTEGER, "job-id", job_id)],
            },
        ],
        data: Vec::new(),
    }
    .encode();
    // Chunked, as many printers answer.
    let mut out = b"HTTP/1.1 200 OK\r\nContent-Type: application/ipp\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
    out.extend_from_slice(format!("{:x}\r\n", m.len()).as_bytes());
    out.extend_from_slice(&m);
    out.extend_from_slice(b"\r\n0\r\n\r\n");
    out
}

#[test]
fn ipp_print_job() {
    let (l, port) = listen();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let (line, headers, body) = http_request(&mut s);
        let m = Message::parse(&body).unwrap();
        s.write_all(&ipp_reply(0x0000, m.request_id, 42)).unwrap();
        tx.send((line, headers, m)).unwrap();
    });
    let t = Target::Lan {
        host: format!("ipp://127.0.0.1:{port}/printers/lj"),
        port: 0,
        driver: Driver::Ipp,
        secure: false,
        transform: Transform::Pdf,
    };
    let d = deliver(&t, &job(), &scs(), Arc::new(PlainConnector)).unwrap();
    assert_eq!(d.receipt.job_id.as_deref(), Some("42"));
    let (line, headers, m) = rx.recv().unwrap();
    assert_eq!(line, "POST /printers/lj HTTP/1.1");
    assert!(headers.iter().any(|(k, v)| k == "Content-Type" && v == "application/ipp"));
    assert_eq!((m.version, m.code), ((1, 1), ipp::PRINT_JOB));
    assert_eq!(m.groups[0].attrs[0].name, "attributes-charset");
    assert_eq!(m.groups[0].attrs[1].name, "attributes-natural-language");
    assert_eq!(m.attr("printer-uri").unwrap().as_str(), format!("ipp://127.0.0.1:{port}/printers/lj"));
    assert_eq!(m.attr("requesting-user-name").unwrap().as_str(), "ANNE");
    assert_eq!(m.attr("document-format").unwrap().as_str(), "application/pdf");
    assert_eq!(m.attr("copies").unwrap().as_int(), Some(2));
    assert_eq!(m.attr("sides").unwrap().as_str(), "two-sided-long-edge");
    assert!(m.data.starts_with(b"%PDF-1.4"));
}

#[test]
fn ipp_refusal_is_rejected() {
    let (l, port) = listen();
    thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let (_, _, body) = http_request(&mut s);
        let m = Message::parse(&body).unwrap();
        // client-error-document-format-not-supported
        s.write_all(&ipp_reply(0x040A, m.request_id, 0)).unwrap();
    });
    let t = Target::Lan {
        host: "127.0.0.1".into(),
        port,
        driver: Driver::Ipp,
        secure: false,
        transform: Transform::Raw,
    };
    let e = deliver(&t, &job(), &scs(), Arc::new(PlainConnector)).unwrap_err();
    assert_eq!(e.kind, SinkErrorKind::Rejected);
    assert!(e.detail.contains("0x040A"));
}

/// What a fake LPD saw of one job.
#[derive(Debug, Default)]
struct LpdJob {
    queue: String,
    /// `(subcommand, name, bytes)` in arrival order.
    files: Vec<(u8, String, Vec<u8>)>,
}

/// Serve `jobs` connections as an LPD that accepts everything.
fn fake_lpd(l: TcpListener, jobs: usize, tx: mpsc::Sender<LpdJob>) {
    thread::spawn(move || {
        for _ in 0..jobs {
            let (s, _) = l.accept().unwrap();
            let mut w = s.try_clone().unwrap();
            let mut r = BufReader::new(s);
            let mut job = LpdJob::default();
            let mut line = Vec::new();
            r.read_until(b'\n', &mut line).unwrap();
            assert_eq!(line[0], 2, "receive a printer job");
            job.queue = String::from_utf8_lossy(&line[1..line.len() - 1]).to_string();
            w.write_all(&[0]).unwrap();
            loop {
                line.clear();
                if r.read_until(b'\n', &mut line).unwrap() == 0 {
                    break;
                }
                let code = line[0];
                let text = String::from_utf8_lossy(&line[1..line.len() - 1]).to_string();
                let (count, name) = text.split_once(' ').unwrap();
                let count: usize = count.parse().unwrap();
                w.write_all(&[0]).unwrap();
                let mut data = vec![0; count + 1];
                r.read_exact(&mut data).unwrap();
                assert_eq!(data.pop(), Some(0), "a file ends with a zero octet");
                w.write_all(&[0]).unwrap();
                job.files.push((code, name.to_string(), data));
            }
            tx.send(job).unwrap();
        }
    });
}

#[test]
fn lpr_to_another_lpd_with_xaix() {
    let (l, port) = listen();
    let (tx, rx) = mpsc::channel();
    fake_lpd(l, 1, tx);
    let t = Target::Remote {
        host: "127.0.0.1".into(),
        port,
        queue: "lp0".into(),
        desttype: DestType::Other,
        destopt: "XAIX".into(),
        seppage: false,
        transform: Transform::Pcl(Model::Hp5),
        local_host: "QS400A".into(),
    };
    deliver(&t, &job(), &scs(), Arc::new(PlainConnector)).unwrap();
    let got = rx.recv().unwrap();
    assert_eq!(got.queue, "lp0");
    assert_eq!(got.files.len(), 2);
    let (dcode, dname, data) = &got.files[0];
    assert_eq!((*dcode, dname.as_str()), (3, "dfA113QS400A"));
    assert!(data.starts_with(b"\x1bE"), "PCL, transformed");
    let (ccode, cname, cf) = &got.files[1];
    assert_eq!((*ccode, cname.as_str()), (2, "cfA113QS400A"));
    let cf = ControlFile::parse(cf);
    assert_eq!(cf.get(b'H'), Some("QS400A"));
    assert_eq!(cf.get(b'P'), Some("ANNE"));
    assert_eq!(cf.get(b'J'), Some("QPRINT.004711.3"));
    assert_eq!(cf.get(b'N'), Some("QPRINT"));
    assert_eq!(cf.get(b'U'), Some("dfA113QS400A"));
    assert!(cf.get(b'L').is_none(), "SEPPAGE(*NO): no banner");
    // XAIX: both copies from one control file.
    assert_eq!(cf.prints(), vec![(b'l', "dfA113QS400A"), (b'l', "dfA113QS400A")]);
    assert!(cf.extension().is_none());
}

#[test]
fn lpr_without_xaix_sends_each_copy() {
    let (l, port) = listen();
    let (tx, rx) = mpsc::channel();
    fake_lpd(l, 2, tx);
    let mut s = print::sink::lpr::LprSink::new("127.0.0.1", "lp0", "QS400A");
    s.endpoint.port = port;
    let doc = Transform::Ascii.apply(&job(), &scs());
    let r = print::sink::Sink::send(&mut s, &job(), &doc).unwrap();
    assert!(r.detail.contains("2 jobs"));
    for _ in 0..2 {
        let got = rx.recv().unwrap();
        let cf = ControlFile::parse(&got.files[1].2);
        assert_eq!(cf.prints().len(), 1);
        assert_eq!(cf.get(b'L'), Some("ANNE"), "SEPPAGE(*YES): a banner");
    }
}

#[test]
fn ibm_to_ibm_carries_the_spooled_file() {
    let (l, port) = listen();
    let (tx, rx) = mpsc::channel();
    fake_lpd(l, 1, tx);
    let t = Target::Remote {
        host: "127.0.0.1".into(),
        port,
        queue: "QGPL/RECEIVED".into(),
        desttype: DestType::Os400,
        destopt: String::new(),
        seppage: true,
        transform: Transform::Pcl(Model::Hp4),
        local_host: "QS400A".into(),
    };
    let data = scs();
    deliver(&t, &job(), &data, Arc::new(PlainConnector)).unwrap();
    let got = rx.recv().unwrap();
    assert_eq!(got.queue, "QGPL/RECEIVED");
    assert_eq!(got.files[0].2, data, "the SCS, untransformed");
    let cf = ControlFile::parse(&got.files[1].2);
    let back = cf.job().expect("the QS/400 extension");
    assert_eq!(back, job());
    // One transfer: the copies travel as an attribute.
    assert_eq!(cf.prints().len(), 1);
}

#[test]
fn lpd_refusal_and_dead_host() {
    let (l, port) = listen();
    thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut b = [0u8; 64];
        let _ = s.read(&mut b).unwrap();
        s.write_all(&[1]).unwrap();
    });
    let mut s = print::sink::lpr::LprSink::new("127.0.0.1", "nope", "QS400A");
    s.endpoint.port = port;
    let doc = Transform::Ascii.apply(&job(), &scs());
    let e = print::sink::Sink::send(&mut s, &job(), &doc).unwrap_err();
    assert_eq!(e.kind, SinkErrorKind::Rejected);
    assert_eq!(e.kind.msgid(), "TCP3701");
    let (l, port) = listen();
    drop(l);
    let mut s = print::sink::lpr::LprSink::new("127.0.0.1", "lp0", "QS400A");
    s.endpoint.port = port;
    let e = print::sink::Sink::send(&mut s, &job(), &doc).unwrap_err();
    assert_eq!(e.kind, SinkErrorKind::Refused);
}
