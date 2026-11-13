// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use super::*;

#[test]
fn base_types() {
    let mut e = Encoder::new();
    e.put_u32(1);
    e.put_i32(-2);
    e.put_u64(3);
    e.put_i64(-4);
    e.put_bool(true);
    let b = e.into_bytes();
    assert_eq!(b.len(), 28);
    assert_eq!(&b[4..8], &[0xff, 0xff, 0xff, 0xfe]);
    let mut d = Decoder::new(&b);
    assert_eq!(d.get_u32(), Ok(1));
    assert_eq!(d.get_i32(), Ok(-2));
    assert_eq!(d.get_u64(), Ok(3));
    assert_eq!(d.get_i64(), Ok(-4));
    assert_eq!(d.get_bool(), Ok(true));
    assert!(d.finish().is_ok());
}

#[test]
fn opaque_padding() {
    let mut e = Encoder::new();
    e.put_opaque(b"abcde", 8).unwrap();
    assert_eq!(e.as_bytes(), b"\0\0\0\x05abcde\0\0\0");
    assert_eq!(Decoder::new(e.as_bytes()).get_opaque(8).unwrap(), b"abcde");
    // Over the bound either way.
    assert!(matches!(Encoder::new().put_opaque(b"abcde", 4), Err(Error::TooLong { len: 5, max: 4 })));
    assert!(matches!(Decoder::new(e.as_bytes()).get_opaque(4), Err(Error::TooLong { .. })));
    // Non-zero padding.
    assert_eq!(Decoder::new(b"\0\0\0\x01a\0\x01\0").get_opaque(8), Err(Error::Padding));
    // Missing padding.
    assert!(matches!(Decoder::new(b"\0\0\0\x01a").get_opaque(8), Err(Error::Short { .. })));
}

#[test]
fn lengths_are_not_trusted() {
    // A length of 4 GiB in a 4-byte message is refused before any allocation.
    let b = [0xff, 0xff, 0xff, 0xff];
    assert!(matches!(Decoder::new(&b).get_opaque(u32::MAX), Err(Error::Short { .. })));
    assert!(matches!(Decoder::new(&b).get_array::<u64>(u32::MAX), Err(Error::Short { .. })));
    // Two million u32s announced with eight bytes behind.
    let b = [0, 0x20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert!(matches!(Decoder::new(&b).get_array::<u32>(u32::MAX), Err(Error::Short { .. })));
}

#[test]
fn arrays_and_lists() {
    let mut e = Encoder::new();
    e.put_array(&[1u32, 2], 2).unwrap();
    e.put_fixed_array(&[7i32], 1).unwrap();
    e.put_list(&[5u32, 6]).unwrap();
    let b = e.into_bytes();
    assert_eq!(b.len(), 12 + 4 + 20);
    let mut d = Decoder::new(&b);
    assert_eq!(d.get_array::<u32>(2).unwrap(), vec![1, 2]);
    assert_eq!(d.get_fixed_array::<i32>(1).unwrap(), vec![7]);
    assert_eq!(d.get_list::<u32>().unwrap(), vec![5, 6]);
    assert!(matches!(Encoder::new().put_array(&[1u32, 2, 3], 2), Err(Error::TooLong { .. })));
    assert!(matches!(Encoder::new().put_fixed_array(&[1u32], 2), Err(Error::FixedLength { .. })));
}

#[test]
fn a_long_list_is_not_recursion() {
    let mut e = Encoder::new();
    let v: Vec<u32> = (0..100_000).collect();
    e.put_list(&v).unwrap();
    assert_eq!(Decoder::new(e.as_bytes()).get_list::<u32>().unwrap().len(), 100_000);
}

#[test]
fn options_boxes_and_fixed() {
    let v: Option<Box<[u8; 3]>> = Some(Box::new(*b"xyz"));
    let b = to_bytes(&v).unwrap();
    assert_eq!(b, b"\0\0\0\x01xyz\0");
    assert_eq!(from_bytes::<Option<Box<[u8; 3]>>>(&b), Ok(v));
    assert_eq!(from_bytes::<Option<u32>>(&[0, 0, 0, 0]), Ok(None));
    assert_eq!(from_bytes::<Option<u32>>(&[0, 0, 0, 2]), Err(Error::Bool(2)));
    assert_eq!(from_bytes::<u32>(&[0, 0, 0, 0, 1]), Err(Error::Trailing(1)));
    assert_eq!(<[u8; 5]>::MIN_SIZE, 8);
}

#[test]
fn depth_is_limited() {
    // Ten nested options decode at depth 10, not at depth 5.
    type O = Option<Box<Option<Box<Option<Box<Option<Box<Option<Box<Option<Box<u32>>>>>>>>>>>>;
    let mut b = Vec::new();
    for _ in 0..6 {
        b.extend_from_slice(&[0, 0, 0, 1]);
    }
    b.extend_from_slice(&[0, 0, 0, 9]);
    assert!(O::decode(&mut Decoder::new(&b)).is_ok());
    assert_eq!(O::decode(&mut Decoder::with_depth(&b, 5)), Err(Error::TooDeep(5)));
}
