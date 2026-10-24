// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use super::*;

#[test]
fn builds_header_attrs_and_padding() {
    let mut b = Builder::new(18, NLM_F_REQUEST | NLM_F_DUMP);
    b.bytes(&[0u8; 16]).str(3, "eth0").u8(16, 6);
    let m = b.finish(7);
    assert_eq!(m.len(), 16 + 16 + 12 + 8);
    assert_eq!(ne_u32(&m, 0), Some(u32::try_from(m.len()).unwrap()));
    assert_eq!(ne_u16(&m, 4), Some(18));
    assert_eq!(ne_u16(&m, 6), Some(0x301));
    assert_eq!(ne_u32(&m, 8), Some(7));
    // "eth0\0" is a 9-byte attribute padded to 12.
    assert_eq!(&m[32..44], &[9, 0, 3, 0, b'e', b't', b'h', b'0', 0, 0, 0, 0]);
    assert_eq!(&m[44..52], &[5, 0, 16, 0, 6, 0, 0, 0]);
}

#[test]
fn nests_set_the_flag_and_length() {
    let mut b = Builder::new(16, NLM_F_REQUEST);
    b.nest(18, |b| {
        b.str(1, "vlan");
        b.nest(2, |b| {
            b.u16(1, 100);
        });
    });
    let m = b.finish(1);
    let attrs: Vec<Attr<'_>> = Attrs::new(&m[16..]).collect();
    assert_eq!(attrs.len(), 1);
    assert!(attrs[0].nested);
    assert_eq!(attrs[0].kind, 18);
    let inner = attrs[0].attrs();
    assert_eq!(inner.get(1).unwrap().str(), "vlan");
    assert_eq!(inner.get(2).unwrap().attrs().get(1).unwrap().u16(), Some(100));
}

#[test]
fn parses_errors_acks_and_done() {
    // An ack (error 0) and an EEXIST (-17) for sequence 5, then DONE.
    let mut buf = Vec::new();
    for code in [0i32, -17] {
        let mut b = Builder::new(NLMSG_ERROR, 0);
        let mut p = code.to_ne_bytes().to_vec();
        p.extend_from_slice(&[0u8; 16]);
        b.bytes(&p);
        buf.extend_from_slice(&b.finish(5));
    }
    let mut d = Builder::new(NLMSG_DONE, NLM_F_MULTI);
    d.bytes(&0i32.to_ne_bytes());
    buf.extend_from_slice(&d.finish(5));
    let ms: Vec<Msg<'_>> = messages(&buf).map(Result::unwrap).collect();
    assert_eq!(ms.len(), 3);
    assert_eq!(ms[0].error(), Some(0));
    assert_eq!(ms[1].error(), Some(17));
    assert_eq!(ms[2].done_error(), Some(0));
    assert_eq!(ms[2].error(), None);
}

#[test]
fn rejects_a_truncated_message() {
    let mut m = Builder::new(16, 0).finish(1);
    m[0] = 200;
    assert!(messages(&m).next().unwrap().is_err());
}

#[test]
fn stops_at_a_truncated_attribute() {
    let buf = [8u8, 0, 1, 0, 1, 2, 3, 4, 40, 0, 2, 0];
    let a: Vec<Attr<'_>> = Attrs::new(&buf).collect();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].u32(), Some(0x0403_0201));
}
