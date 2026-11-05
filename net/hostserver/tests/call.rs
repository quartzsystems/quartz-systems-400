// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `CALL` through the database server over a stand-in engine that
//! plans and makes the call: the markers' modes in the format, the
//! `OUT` values as the execute's result data, the result sets opened
//! one after another on the statement's block.

mod common;

use common::TraceAuth;
use hostserver::database::types::ColType;
use hostserver::database::{Database, DbConfig, data};
use hostserver::{Handler, NoExits};
use hostsvr::cp::{db_reply, db_request};
use hostsvr::database::param::DbString;
use hostsvr::frame::{Frame, Param};
use sql::embed::{Embedded, Op, Reply, Request};
use sql::sqlca::Sqlca;
use sql::value::Value;

/// What the engine and the job together answer.
#[derive(Default)]
struct Engine {
    outs: Vec<(usize, Value)>,
    sets: Vec<String>,
}

fn describe_row(t: i64, len: i64, name: &str, mode: i64) -> Vec<Value> {
    let mut r = vec![Value::int(t), Value::int(len), Value::Str(name.into())];
    r.extend([Value::int(37), Value::Str(String::new()), Value::int(0), Value::int(0), Value::Str(name.into()), Value::Str(name.into())]);
    r.push(Value::int(mode));
    r
}

impl Embedded for Engine {
    fn run(&mut self, req: &Request) -> Reply {
        assert!(req.desc.calls || matches!(req.desc.op, Op::Describe | Op::Fetch | Op::Close | Op::Prepare | Op::Execute));
        let d = &req.desc;
        let mut r = Reply::default();
        match (d.op, d.orient.as_str()) {
            (Op::Describe, "INPUT") => {
                r.rows = vec![describe_row(497, 4, "A", 1), describe_row(453, 5, "B", 2)];
            }
            (Op::Describe, "CURSOR") => {
                r.rows = vec![describe_row(497, 4, "ID", 0)];
            }
            (Op::Execute, _) => {
                assert_eq!(req.inputs.first(), Some(&Value::int(7)));
                self.outs = vec![(1, Value::Str("HELLO".into()))];
                self.sets = vec!["RS#1".into(), "RS#2".into()];
                r.sqlca = Sqlca { sqlcode: 466, sqlstate: "0100C".into(), sqlerrd: [0, 2, 0, 0, 0, 0], ..Sqlca::default() };
                r.rows = self.sets.iter().map(|n| vec![Value::Str(n.clone()), Value::Str("C1".into()), Value::int(1)]).collect();
            }
            (Op::Fetch, _) => {
                let n = if d.name == "RS#1" { 3 } else { 1 };
                r.rows = (1..=n).map(|i| vec![Value::int(i)]).collect();
            }
            _ => {}
        }
        r
    }

    fn take_outs(&mut self) -> Vec<(usize, Value)> {
        std::mem::take(&mut self.outs)
    }
}

const ORS: u32 = 0x8000_0000 | 0x0800_0000 | 0x0400_0000 | 0x0200_0000 | 0x0080_0000;

fn request(function: u16, params: Vec<Param>) -> Vec<u8> {
    let mut t = ORS.to_be_bytes().to_vec();
    t.extend([0u8; 10]);
    t.extend(1u16.to_be_bytes());
    t.extend(0u16.to_be_bytes());
    t.extend([0u8; 2]);
    Frame::new(0xE004, function, t, params).encode()
}

fn reply(db: &mut Database<TraceAuth, Engine, NoExits>, function: u16, params: Vec<Param>) -> Frame {
    let out = db.handle(&request(function, params));
    Frame::decode(out.replies.last().expect("a reply")).expect("a frame")
}

fn sqlcode(f: &Frame) -> i32 {
    i32::from_be_bytes(f.template[16..20].try_into().unwrap())
}

#[test]
fn a_call_gives_back_its_outputs_and_its_result_sets() {
    let mut db = Database::new(TraceAuth::default(), Engine::default(), NoExits, DbConfig::default());
    db.session.user = Some("QSECOFR".into());
    let text = DbString { ccsid: 37, long: false, text: hostserver::text::ebcdic("CALL QSTEST.P(?, ?)") }.encode();
    let f = reply(&mut db, 0x1803, vec![Param::new(db_request::STATEMENT_TEXT, text)]);
    // The marker format: B is an output.
    let fmt = &f.param(db_reply::SUPER_MARKER_FORMAT).expect("marker format").data;
    assert_eq!((fmt[16 + 14], fmt[16 + 48 + 14]), (0xF0, 0xF1));
    let mut a = ColType::integer("", true);
    a.mode = 1;
    let mut b = ColType::from_describe(453, 5, "");
    b.mode = 2;
    let markers = vec![a, b.clone()];
    let values = data::encode_rows(&markers, &[vec![Value::int(7), Value::Null]], 1);
    let f = reply(&mut db, 0x1805, vec![Param::new(db_request::EXTENDED_MARKER_DATA, values)]);
    assert_eq!(sqlcode(&f), 466);
    let out = &f.param(db_reply::EXTENDED_RESULT_DATA).expect("output values").data;
    let row = data::decode_rows(out, &markers);
    assert_eq!(row[0][1].as_text().trim(), "HELLO");
    // The first result set, then the second.
    for want in [3, 1] {
        let f = reply(&mut db, 0x180E, Vec::new());
        let rows = data::decode_rows(&f.param(db_reply::EXTENDED_RESULT_DATA).expect("rows").data, &[ColType::integer("ID", true)]);
        assert_eq!(rows.len(), want);
        reply(&mut db, 0x180A, Vec::new());
    }
}
