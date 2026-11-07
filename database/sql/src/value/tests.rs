// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use dds::{DataType, FieldDef};

use super::*;

fn field(t: DataType, len: u32) -> FieldDef {
    let mut f = FieldDef::new(qsname::Name::parse("F").unwrap(), t, len, 0);
    f.byte_len = f.data_len();
    f
}

fn round_trip(f: &FieldDef, v: &Value) -> Value {
    let mut rec = vec![0u8; usize::try_from(f.byte_len).unwrap()];
    db::record::write_field(f, &mut rec, &v.to_field(f).unwrap()).unwrap();
    Value::from_field(f, &db::record::read_field(f, &rec, &[], None).unwrap()).unwrap()
}

#[test]
fn keys_and_compares() {
    let a = Value::Num(Decimal::parse("1.50").unwrap());
    let b = Value::Num(Decimal::parse("1.5").unwrap());
    assert_eq!(a.key(), b.key());
    assert_eq!(Value::Num(Decimal::parse("0.00").unwrap()).key(), "0");
    assert_eq!(
        Value::Str("ab  ".into()).key(),
        Value::Str("ab".into()).key()
    );
    assert_eq!(a.compare(&b).unwrap(), Some(Ordering::Equal));
    assert_eq!(Value::Null.compare(&a).unwrap(), None);
    assert!(Value::Str("x".into()).compare(&a).is_err());
    let d = Value::Date(Date::new(2026, 1, 2).unwrap());
    assert_eq!(
        d.compare(&Value::Str("2026-01-02".into())).unwrap(),
        Some(Ordering::Equal)
    );
    assert_eq!(
        d.compare(&Value::Str("01/03/2026".into())).unwrap(),
        Some(Ordering::Less)
    );
    assert_eq!(Timestamp::parse("2026-01-02 10:20:30").unwrap().micros(), 0);
}

#[test]
fn floats_keep_their_fraction() {
    for (prec, len) in [
        (dds::FloatPrecision::Single, 9),
        (dds::FloatPrecision::Double, 17),
    ] {
        let mut f = field(DataType::Float, len);
        f.precision = prec;
        f.byte_len = f.data_len();
        for text in ["4.5", "-0.1", "1234.0625", "0.000123"] {
            let v = Value::Num(Decimal::parse(text).unwrap());
            assert_eq!(round_trip(&f, &v).to_string(), text);
        }
        // Past a decimal's 63 digits.
        for text in ["1E30", "-2.5E-30", "3.4028235E38"] {
            let v = Value::Double(text.parse().unwrap());
            assert_eq!(round_trip(&f, &v).to_string(), text);
        }
    }
    let mut d = field(DataType::Float, 17);
    d.precision = dds::FloatPrecision::Double;
    d.byte_len = d.data_len();
    for text in ["1E300", "-2.5E-308", "0.1"] {
        let v = Value::Double(text.parse().unwrap());
        assert_eq!(round_trip(&d, &v).to_string(), text);
    }
    // A single is a double beside a decimal, as IBM promotes it.
    let r = Value::Real(0.1);
    assert_eq!(
        r.compare(&Value::Num(Decimal::parse("0.1").unwrap()))
            .unwrap(),
        Some(Ordering::Greater)
    );
    assert_eq!(r.compare(&Value::Real(0.1)).unwrap(), Some(Ordering::Equal));
    assert_eq!(
        Value::Double(1e300).compare(&Value::int(1)).unwrap(),
        Some(Ordering::Greater)
    );
    assert_eq!(
        Value::Double(-0.0).compare(&Value::Double(0.0)).unwrap(),
        Some(Ordering::Equal)
    );
    assert_eq!(Value::Double(2.0).key(), Value::int(2).key());
    assert!(Value::Double(1e300).as_decimal().is_err());
}

#[test]
fn decfloats() {
    let f = field(DataType::DecFloat, 34);
    for text in ["1.50", "-1E+400", "Infinity", "-NaN", "sNaN", "0E-10"] {
        let v = Value::DecFloat(DecFloat::parse(text, DfFormat::D34).unwrap());
        assert_eq!(round_trip(&f, &v).to_string(), v.to_string(), "{text}");
    }
    let one = Value::DecFloat(DecFloat::parse("1.0", DfFormat::D16).unwrap());
    assert_eq!(one.compare(&Value::int(1)).unwrap(), Some(Ordering::Equal));
    assert_eq!(one.key(), Value::int(1).key());
    let nan = Value::DecFloat(DecFloat::parse("NaN", DfFormat::D16).unwrap());
    assert_eq!(
        nan.compare(&Value::int(1)).unwrap(),
        Some(Ordering::Greater)
    );
    // DECFLOAT(16) rounds what it is given.
    let short = field(DataType::DecFloat, 16);
    let long = Value::Num(Decimal::parse("1.23456789012345678").unwrap());
    assert_eq!(round_trip(&short, &long).to_string(), "1.234567890123457");
}

#[test]
fn binary_and_boolean() {
    let mut vb = field(DataType::Bytes, 8);
    vb.varlen = Some(dds::VarLen { allocated: None });
    vb.byte_len = 10;
    let v = Value::Bin(vec![0, 0x40, 0xFF]);
    assert_eq!(round_trip(&vb, &v), v);
    let fixed = field(DataType::Bytes, 4);
    assert_eq!(round_trip(&fixed, &v), Value::Bin(vec![0, 0x40, 0xFF, 0]));
    assert!(Value::Bin(vec![0; 5]).to_field(&fixed).is_err());
    assert_eq!(v.to_string(), "0040FF");
    assert_eq!(
        Value::Bin(vec![1])
            .compare(&Value::Bin(vec![1, 0]))
            .unwrap(),
        Some(Ordering::Less)
    );
    // The primary: BX'C1' = 'A' is SQL0401.
    assert_eq!(
        Value::Bin(vec![0xC1])
            .compare(&Value::Str("A".into()))
            .unwrap_err()
            .sqlcode(),
        -401
    );
    let b = field(DataType::Boolean, 1);
    assert_eq!(round_trip(&b, &Value::Bool(true)), Value::Bool(true));
    assert_eq!(
        round_trip(&b, &Value::Str("false".into())),
        Value::Bool(false)
    );
    assert_eq!(
        Value::Bool(true).compare(&Value::int(1)).unwrap(),
        Some(Ordering::Equal)
    );
    assert!(Value::Str("maybe".into()).to_field(&b).is_err());
}

#[test]
fn timestamps_by_precision() {
    let t12 = field(DataType::Timestamp, DataType::timestamp_length(12));
    let t0 = field(DataType::Timestamp, DataType::timestamp_length(0));
    let v = Value::Timestamp(Timestamp::parse("2026-11-02 10:20:30.123456789012").unwrap());
    assert_eq!(
        round_trip(&t12, &v).to_string(),
        "2026-11-02 10:20:30.123456789012"
    );
    assert_eq!(round_trip(&t0, &v).to_string(), "2026-11-02 10:20:30");
    let old = field(DataType::Timestamp, 26);
    assert_eq!(
        round_trip(&old, &v).to_string(),
        "2026-11-02 10:20:30.123456"
    );
}

#[test]
fn lobs() {
    let c = field(DataType::Clob, 10);
    assert_eq!(
        lob_value(&c, &Value::Str("AB".into()).lob_bytes(&c).unwrap()),
        Value::Str("AB".into())
    );
    assert!(Value::Str("x".repeat(11)).lob_bytes(&c).is_err());
    let d = field(DataType::Dbclob, 3);
    let bytes = Value::Str("ä€x".into()).lob_bytes(&d).unwrap();
    assert_eq!(bytes.len(), 6);
    assert_eq!(lob_value(&d, &bytes), Value::Str("ä€x".into()));
    let b = field(DataType::Blob, 4);
    assert_eq!(
        lob_value(&b, &Value::Bin(vec![9, 8]).lob_bytes(&b).unwrap()),
        Value::Bin(vec![9, 8])
    );
}
