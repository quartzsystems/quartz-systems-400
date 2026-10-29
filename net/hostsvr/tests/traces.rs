// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every committed trace parses, writes back to the same text, and every
//! frame in it decodes into a typed message that encodes to the same
//! bytes. Typed views of the parameters decode too.

use std::path::{Path, PathBuf};

use hostsvr::cp::{db_reply, rmtcmd as rc_cp, signon as so_cp};
use hostsvr::database::{data::ResultData, format::SuperFormat, param as dbp, sqlca::Sqlca};
use hostsvr::template::Template as _;
use hostsvr::trace::{Chunk, Trace};
use hostsvr::{Body, Dir, Message, rmtcmd, signon, svrmap};

fn traces_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/hostsvr/traces")
}

fn traces() -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for server in std::fs::read_dir(traces_dir()).expect("traces folder") {
        let server = server.unwrap().path();
        for f in std::fs::read_dir(&server).unwrap() {
            let f = f.unwrap().path();
            if f.extension().is_some_and(|e| e == "trace") {
                let text = std::fs::read_to_string(&f).unwrap();
                out.push((f, text));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn traces_round_trip_as_text() {
    let all = traces();
    assert!(
        all.len() >= 20,
        "expected the committed traces, found {}",
        all.len()
    );
    for (path, text) in &all {
        let t = Trace::parse(text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            &t.write(),
            text,
            "{} does not write back identically",
            path.display()
        );
        let folder = path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(t.server, folder, "{}", path.display());
    }
}

/// Typed views of a message's parameters; returns how many decoded.
fn typed_params(m: &Message) -> usize {
    let mut n = 0;
    match &m.body {
        Body::Signon(_) | Body::Common(_) => {
            if matches!(m.header.req_rep_id, 0x7003 | 0xF003) {
                signon::Attributes::from_params(&m.params).unwrap();
                n += 1;
            }
            if m.header.req_rep_id == 0xF004 {
                signon::SignonInfo::from_params(&m.params).unwrap();
                n += 1;
            }
            n += signon::messages(&m.params).unwrap().len();
            if let Some(p) = m.param(so_cp::JOB_NAME) {
                assert!(p.data.len() > 4);
                n += 1;
            }
        }
        Body::RemoteCommand(_) => {
            n += rmtcmd::messages(&m.params).unwrap().len();
            for q in m.params.iter().filter(|q| q.cp == rc_cp::PARAMETER) {
                let p = rmtcmd::ProgramParameter::decode(&q.data).unwrap();
                assert_eq!(&p.to_param(), q);
                p.value().unwrap();
                n += 1;
            }
            if rmtcmd::command(&m.params).is_some() {
                n += 1;
            }
        }
        Body::Database(_) => {
            let params = dbp::expanded(&m.params).unwrap();
            for p in params.iter().filter(|p| !p.data.is_empty()) {
                match p.cp {
                    db_reply::SQLCA if m.dir == Dir::Reply => {
                        let s = Sqlca::decode(&p.data).unwrap_or_else(|| {
                            panic!("SQLCA of {} bytes in {}", p.data.len(), m.name())
                        });
                        assert_eq!(s.encode(), p.data);
                        n += 1;
                    }
                    db_reply::SUPER_DATA_FORMAT | db_reply::SUPER_MARKER_FORMAT
                        if m.dir == Dir::Reply =>
                    {
                        let f = SuperFormat::decode(&p.data).unwrap();
                        assert_eq!(f.encode(), p.data);
                        for i in 0..f.fields.len() {
                            assert!(f.name(i).is_some(), "field {i} has a name");
                        }
                        n += 1;
                    }
                    db_reply::EXTENDED_RESULT_DATA if m.dir == Dir::Reply => {
                        let d = ResultData::decode(&p.data).unwrap();
                        assert_eq!(d.encode(), p.data);
                        n += 1;
                    }
                    db_reply::SERVER_ATTRIBUTES if m.dir == Dir::Reply => {
                        let fixed = dbp::ServerAttributes::SIZE;
                        let a = dbp::ServerAttributes::decode(&p.data[..fixed]).unwrap();
                        assert_eq!(p.data.len(), fixed + usize::from(a.schema_length));
                        n += 1;
                    }
                    _ => {}
                }
            }
            // In exchange attributes 0x3807 is the date format, not text.
            if m.dir == Dir::Request
                && m.header.req_rep_id != hostsvr::database::id::SET_ATTRIBUTES
                && let Some(s) = dbp::statement_text(&params)
            {
                assert!(s.unwrap().to_text().is_some());
                n += 1;
            }
        }
        _ => {}
    }
    n
}

#[test]
fn frames_decode_and_encode_byte_identically() {
    let (mut frames, mut typed, mut views, mut svrmaps) = (0, 0, 0, 0);
    for (path, text) in traces() {
        let t = Trace::parse(&text).unwrap();
        for c in &t.connections {
            let chunks: Vec<&Chunk> = c.chunks().collect();
            for ch in chunks {
                let at = format!("{} conn {} chunk {}", path.display(), c.id, ch.seq);
                if !ch.framed {
                    assert_eq!(
                        t.server, "svrmap",
                        "{at}: raw bytes outside the port mapper"
                    );
                    match ch.dir {
                        Dir::Request => assert_eq!(
                            svrmap::Request::decode(&ch.bytes).unwrap().encode(),
                            ch.bytes,
                            "{at}"
                        ),
                        Dir::Reply => assert_eq!(
                            svrmap::Reply::decode(&ch.bytes).unwrap().encode(),
                            ch.bytes,
                            "{at}"
                        ),
                    }
                    svrmaps += 1;
                    continue;
                }
                let m = Message::decode(ch.dir, &ch.bytes).unwrap_or_else(|e| panic!("{at}: {e}"));
                assert_eq!(
                    m.encode(),
                    ch.bytes,
                    "{at}: {} does not encode back",
                    m.name()
                );
                assert!(
                    m.tail.is_empty(),
                    "{at}: {} has bytes outside LL/CP",
                    m.name()
                );
                assert!(
                    !m.name().ends_with(&format!("{:04X}", m.header.req_rep_id)),
                    "{at}: unnamed ID {}",
                    m.name()
                );
                frames += 1;
                if m.body.is_typed() {
                    typed += 1;
                }
                views += typed_params(&m);
            }
        }
    }
    println!(
        "{frames} frames, {typed} typed templates, {views} typed parameter views, {svrmaps} port mapper chunks"
    );
    assert!(frames > 500, "{frames} frames");
    assert!(
        typed * 10 >= frames * 6,
        "only {typed} of {frames} templates typed"
    );
    assert!(views > 100, "{views} typed views");
}

#[test]
fn typed_templates_cover_the_typed_servers() {
    // Every frame of the servers this codec types fully has a typed template.
    for (path, text) in traces() {
        let t = Trace::parse(&text).unwrap();
        if !matches!(
            t.server.as_str(),
            "signon" | "rmtcmd" | "database" | "central"
        ) {
            continue;
        }
        for c in &t.connections {
            for ch in c.chunks() {
                let m = Message::decode(ch.dir, &ch.bytes).unwrap();
                assert!(
                    m.body.is_typed(),
                    "{} conn {} chunk {}: {} untyped",
                    path.display(),
                    c.id,
                    ch.seq,
                    m.name()
                );
            }
        }
    }
}

#[test]
fn committed_traces_are_masked() {
    let user = ccsid::to_ebcdic("MASKUSER  ").unwrap();
    for (path, text) in traces() {
        let t = Trace::parse(&text).unwrap();
        for c in &t.connections {
            for ch in c.chunks().filter(|c| c.framed) {
                let m = Message::decode(ch.dir, &ch.bytes).unwrap();
                if !matches!(m.header.req_rep_id >> 8, 0x70 | 0x71 | 0xF0 | 0xF1) {
                    continue;
                }
                for p in &m.params {
                    if hostsvr::trace::mask::SECRET_CPS.contains(&p.cp) {
                        assert!(
                            p.data.iter().all(|&b| b == 0xFF),
                            "{}: CP {:04X} not masked",
                            path.display(),
                            p.cp
                        );
                    }
                    if p.cp == so_cp::USER_ID && ch.dir == Dir::Request {
                        assert_eq!(p.data, user, "{}: user not masked", path.display());
                    }
                }
            }
        }
    }
}
