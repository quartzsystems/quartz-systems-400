// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The committed pub400 traces: every DSS decodes and re-encodes byte
//! for byte, and the SQL and file objects in them decode.

use std::path::PathBuf;

use ddm::cp::{obj, param, rm};
use ddm::fdoca::desc::{self, Field};
use ddm::fdoca::rows::{self, Row};
use ddm::fdoca::{Reader, Repr, sqlca, sqlda};
use ddm::file::{self, Iofb, OpenFeedback, Ufcb};
use ddm::trace::Trace;
use ddm::{Dir, Dss};

fn traces() -> Vec<(String, Trace)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/ddm/traces");
    let mut out = Vec::new();
    for e in std::fs::read_dir(&dir).expect("traces directory") {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "trace") {
            let text = std::fs::read_to_string(&p).unwrap();
            let t = Trace::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            assert_eq!(t.write(), text.replace("\r\n", "\n"), "{} writes back", p.display());
            out.push((p.file_name().unwrap().to_string_lossy().into_owned(), t));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(out.len() >= 6, "the six scenarios");
    out
}

#[test]
fn every_dss_re_encodes() {
    for (name, t) in traces() {
        for c in &t.connections {
            for ch in c.chunks() {
                assert!(ch.framed, "{name} {}: raw bytes", ch.seq);
                let (d, n) = Dss::decode(&ch.bytes).unwrap_or_else(|e| panic!("{name} {}: {e}", ch.seq));
                assert_eq!(n, ch.bytes.len(), "{name} {}", ch.seq);
                assert_eq!(d.encode(), ch.bytes, "{name} {} re-encodes", ch.seq);
                assert!(!d.objects().is_empty(), "{name} {}: objects", ch.seq);
            }
        }
    }
}

#[test]
fn sql_objects_decode() {
    let mut rows_seen = Vec::new();
    let mut codes = Vec::new();
    for (name, t) in traces() {
        for c in &t.connections {
            let mut fields: Vec<Field> = Vec::new();
            for ch in c.chunks().filter(|c| c.dir == Dir::Reply) {
                let (d, _) = Dss::decode(&ch.bytes).unwrap();
                for o in d.objects() {
                    let mut r = Reader::new(o.bytes(), Repr::default());
                    match o.cp {
                        obj::SQLCARD => {
                            let ca = sqlca::read(&mut r, 7).unwrap_or_else(|e| panic!("{name} {}: {e}", ch.seq));
                            assert!(r.done(), "{name} {}: SQLCARD read to its end", ch.seq);
                            if let Some(ca) = ca {
                                codes.push(ca.sqlcode);
                            }
                        }
                        obj::SQLDARD => {
                            sqlda::read(&mut r, 7).unwrap_or_else(|e| panic!("{name} {}: {e}", ch.seq));
                            assert!(r.done(), "{name} {}: SQLDARD read to its end", ch.seq);
                        }
                        obj::QRYDSC => fields = desc::fields(o.bytes()).unwrap(),
                        obj::QRYDTA => {
                            for row in rows::read_rows(&mut r, &fields, 7).unwrap() {
                                if let Row::Values(v) = row {
                                    rows_seen.push(v.iter().map(ddm::sql::Value::render).collect::<Vec<_>>().join("|"));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    assert!(codes.contains(&-204), "the SQL error");
    assert!(codes.contains(&100), "the end of data");
    assert!(rows_seen.iter().any(|r| r.starts_with("AJ_DMART_INFO|11|")), "{rows_seen:?}");
    assert!(
        rows_seen.contains(&"12.50|42|7|2026-01-02|10.11.12|-|-3|1.5|2026-01-02-03.04.05.123456".to_string()),
        "{rows_seen:?}"
    );
    assert!(rows_seen.contains(&"2|-".to_string()));
}

#[test]
fn file_objects_decode() {
    let mut opens = Vec::new();
    let mut records = 0;
    let mut eof = 0;
    for (name, t) in traces() {
        for c in &t.connections {
            for ch in c.chunks() {
                let (d, _) = Dss::decode(&ch.bytes).unwrap();
                let objs = d.objects();
                for o in objs {
                    match o.cp {
                        ddm::cp::s38::S38OPEN => {
                            let u = Ufcb::decode(o.get(param::S38UFCB).unwrap().bytes()).unwrap();
                            assert_eq!(u.encode(), o.get(param::S38UFCB).unwrap().bytes(), "{name} {}", ch.seq);
                            opens.push(u);
                        }
                        param::S38OPNFB => {
                            let f = OpenFeedback::decode(o.bytes()).unwrap();
                            assert!(f.reclen > 0, "{name} {}", ch.seq);
                        }
                        param::S38BUF if ch.dir == Dir::Reply => {
                            let iofb = objs.iter().find(|x| x.cp == param::S38IOFB).unwrap();
                            let iofb = Iofb::decode(iofb.bytes()).unwrap();
                            let n = usize::from(iofb.count);
                            let each = o.bytes().len() / n;
                            let reclen = iofb.reclen as usize;
                            let fields = each - reclen - file::TRAILER;
                            let recs = file::decode_buf(o.bytes(), reclen, fields).unwrap();
                            assert_eq!(recs.len(), n);
                            assert_eq!(recs.last().unwrap().rrn, iofb.rrn, "{name} {}", ch.seq);
                            assert_eq!(file::encode_buf(&recs), o.bytes());
                            records += n;
                        }
                        rm::S38MSGRM if file::message_of(o).id == "CPF5001" => {
                            eof += 1;
                            assert_eq!(file::eof_msgrm().encode(), o.encode());
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    assert!(opens.iter().any(|u| u.library == "QIWS" && u.file == "QCUSTCDT" && u.flags == file::ufcb::INPUT));
    assert!(opens.iter().any(|u| u.file == "QSDDMT" && !u.arrival));
    assert!(records > 10);
    assert!(eof >= 3);
}
