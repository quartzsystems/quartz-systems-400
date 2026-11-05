// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! BSON, the binary form `JSON_TO_BSON` makes and `BSON_TO_JSON` and
//! `FORMAT BSON` read: little-endian documents of typed elements. A
//! whole number is an int32 (`0x10`) when it fits, else an int64
//! (`0x12`); any other number a double (`0x01`), as the primary writes
//! them (`tools/conformance/sql/cases/types/JSON1.sql`).

use super::value::Json;

/// BSON that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadBson;

/// `doc` as BSON; a value that is not an object or array is wrapped as
/// the member of a document under the empty name.
#[must_use]
pub fn encode(doc: &Json) -> Vec<u8> {
    let mut out = Vec::new();
    match doc {
        Json::Object(m) => document(m.iter().map(|(k, v)| (k.as_str(), v)), &mut out),
        Json::Array(items) => {
            let keys: Vec<String> = (0..items.len()).map(|i| i.to_string()).collect();
            document(keys.iter().map(String::as_str).zip(items.iter()), &mut out);
        }
        v => document(std::iter::once(("", v)), &mut out),
    }
    out
}

fn document<'a>(members: impl Iterator<Item = (&'a str, &'a Json)>, out: &mut Vec<u8>) {
    let start = out.len();
    out.extend_from_slice(&[0; 4]);
    for (k, v) in members {
        element(k, v, out);
    }
    out.push(0);
    let len = u32::try_from(out.len() - start).unwrap_or(u32::MAX);
    out[start..start + 4].copy_from_slice(&len.to_le_bytes());
}

fn cstring(s: &str, out: &mut Vec<u8>) {
    out.extend(s.bytes().filter(|b| *b != 0));
    out.push(0);
}

fn element(k: &str, v: &Json, out: &mut Vec<u8>) {
    if let Json::Raw(t) = v {
        let parsed = super::value::parse(t).unwrap_or_else(|_| Json::Str(t.clone()));
        return element(k, &parsed, out);
    }
    let kind_at = out.len();
    out.push(0);
    cstring(k, out);
    let kind = match v {
        Json::Null => 0x0A,
        Json::Bool(b) => {
            out.push(u8::from(*b));
            0x08
        }
        Json::Num(n) => {
            let whole = !n.contains(['.', 'e', 'E']);
            match n.parse::<i64>() {
                Ok(i) if whole && i32::try_from(i).is_ok() => {
                    out.extend_from_slice(&i32::try_from(i).expect("fits").to_le_bytes());
                    0x10
                }
                Ok(i) if whole => {
                    out.extend_from_slice(&i.to_le_bytes());
                    0x12
                }
                _ => {
                    out.extend_from_slice(&n.parse::<f64>().unwrap_or(0.0).to_le_bytes());
                    0x01
                }
            }
        }
        Json::Str(s) => {
            let b = s.as_bytes();
            out.extend_from_slice(&u32::try_from(b.len() + 1).unwrap_or(u32::MAX).to_le_bytes());
            out.extend_from_slice(b);
            out.push(0);
            0x02
        }
        Json::Object(m) => {
            document(m.iter().map(|(k, v)| (k.as_str(), v)), out);
            0x03
        }
        Json::Array(items) => {
            let keys: Vec<String> = (0..items.len()).map(|i| i.to_string()).collect();
            document(keys.iter().map(String::as_str).zip(items.iter()), out);
            0x04
        }
        Json::Raw(_) => unreachable!("parsed above"),
    };
    out[kind_at] = kind;
}

struct R<'a> {
    b: &'a [u8],
    at: usize,
}

impl R<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], BadBson> {
        let end = self.at.checked_add(n).ok_or(BadBson)?;
        let s = self.b.get(self.at..end).ok_or(BadBson)?;
        self.at = end;
        Ok(s)
    }

    fn u32(&mut self) -> Result<u32, BadBson> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four")))
    }

    fn cstring(&mut self) -> Result<String, BadBson> {
        let rest = self.b.get(self.at..).ok_or(BadBson)?;
        let n = rest.iter().position(|b| *b == 0).ok_or(BadBson)?;
        let s = String::from_utf8(rest[..n].to_vec()).map_err(|_| BadBson)?;
        self.at += n + 1;
        Ok(s)
    }

    fn document(&mut self) -> Result<Vec<(String, Json)>, BadBson> {
        let start = self.at;
        let len = usize::try_from(self.u32()?).map_err(|_| BadBson)?;
        let end = start.checked_add(len).filter(|e| *e <= self.b.len() && len >= 5).ok_or(BadBson)?;
        let mut members = Vec::new();
        loop {
            let kind = *self.take(1)?.first().expect("one");
            if kind == 0 {
                break;
            }
            let k = self.cstring()?;
            let v = match kind {
                0x01 => {
                    let x = f64::from_le_bytes(self.take(8)?.try_into().expect("eight"));
                    Json::Num(if x.is_finite() { format!("{x}") } else { return Err(BadBson) })
                }
                0x02 => {
                    let n = usize::try_from(self.u32()?).map_err(|_| BadBson)?;
                    let s = self.take(n)?;
                    let s = s.strip_suffix(&[0]).ok_or(BadBson)?;
                    Json::Str(String::from_utf8(s.to_vec()).map_err(|_| BadBson)?)
                }
                0x03 => Json::Object(self.document()?),
                0x04 => Json::Array(self.document()?.into_iter().map(|(_, v)| v).collect()),
                0x08 => Json::Bool(*self.take(1)?.first().expect("one") != 0),
                0x0A => Json::Null,
                0x10 => Json::Num(i32::from_le_bytes(self.take(4)?.try_into().expect("four")).to_string()),
                0x12 => Json::Num(i64::from_le_bytes(self.take(8)?.try_into().expect("eight")).to_string()),
                _ => return Err(BadBson),
            };
            members.push((k, v));
        }
        if self.at != end {
            return Err(BadBson);
        }
        Ok(members)
    }
}

/// BSON as a JSON object.
pub fn decode(b: &[u8]) -> Result<Json, BadBson> {
    let mut r = R { b, at: 0 };
    let doc = r.document()?;
    if r.at != b.len() {
        return Err(BadBson);
    }
    Ok(Json::Object(doc))
}

#[cfg(test)]
mod tests {
    use super::super::value::parse;
    use super::*;

    #[test]
    fn as_the_primary_writes_it() {
        let j = parse(r#"{"a":1,"b":"x","c":[true,null],"d":1.5,"e":12345678901}"#).unwrap();
        let hex = crate::value::hex(&encode(&j));
        assert_eq!(
            hex,
            "3A000000106100010000000262000200000078000463000C000000083000010A310000016400000000000000F83F126500351CDCDF0200000000"
        );
        let mut s = String::new();
        decode(&encode(&j)).unwrap().write(&mut s, true);
        assert_eq!(s, r#"{"a":1,"b":"x","c":[true, null ],"d":1.5,"e":12345678901}"#);
        assert!(decode(&[5, 0, 0]).is_err());
    }
}
