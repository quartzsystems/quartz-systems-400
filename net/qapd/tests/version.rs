// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Version negotiation (docs/QAP.md, "Compatibility of new values"):
//! the server speaks 1 and 2 and picks the highest both sides do; a
//! version-1 client sees exactly the version-1 protocol.

mod common;

use qap_proto::ops::command::CommandText;
use qap_proto::ops::connection::{Hello, HelloReply};
use qap_proto::{Envelope, Op, Service, Status};

use common::{cfg, start};

#[test]
fn the_highest_common_version() {
    for (min, max, want) in [(1, 1, 1), (1, 2, 2), (2, 2, 2), (1, 3, 3), (3, 3, 3), (1, 9, 3), (2, 9, 3)] {
        let mut r = start(cfg());
        let h: HelloReply = r.client.hello_range(min, max).body_as().unwrap();
        assert_eq!(h.version, want, "{min}..{max}");
        assert_eq!(h.services, Service::of_version(want));
    }
    let mut r = start(cfg());
    let refused = r.client.call(
        Op::Hello,
        &Hello {
            min_version: 4,
            max_version: 9,
            ..Hello::default()
        },
    );
    assert_eq!(refused.status, Status::VersionUnsupported);
    assert!(r.client.closed());
}

#[test]
fn version_1_sees_only_version_1() {
    let mut r = start(cfg());
    let h: HelloReply = r.client.hello().body_as().unwrap();
    assert_eq!(h.version, 1);
    assert!(!h.services.contains(&Service::Command));
    // Version 2's operations do not exist on this connection, signed on
    // or not.
    for op in [Op::RunCommand, Op::CheckCommand, Op::DescribeCommand, Op::TelnetTicket, Op::PasswordStatus] {
        let id = r.client.request(op, &CommandText::default());
        let rep = r.client.reply(id);
        assert_eq!(rep.status, Status::InvalidRequest, "{op:?}");
        assert!(rep.body.is_empty());
    }
    // Every frame it gets decodes with version 1's values.
    let e = r.client.call(Op::Ping, &());
    assert_eq!(e.status, Status::Ok);
    let unknown = Envelope {
        op: 9,
        ..Envelope::request(50, Op::SignOff, &())
    };
    r.client.send(&unknown);
    assert_eq!(r.client.reply(50).status, Status::InvalidRequest);
}
