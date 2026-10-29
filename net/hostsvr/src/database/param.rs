// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database parameter shapes: strings with a CCSID and a length, the
//! RLE container (`0x3832`) a compressed reply or request travels in,
//! and the server attributes record of the exchange attributes reply.

use crate::cp::{db_reply, db_request};
use crate::frame::{Param, param};
use crate::template::template;
use crate::{Error, rle};

/// A string parameter: CCSID (u16), length (u16, or u32 in the extended
/// statement text), bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbString {
    /// The text's CCSID.
    pub ccsid: u16,
    /// Whether the length is four bytes (`0x3831`).
    pub long: bool,
    /// The text's bytes.
    pub text: Vec<u8>,
}

impl DbString {
    /// Decode a string parameter's data.
    pub fn decode(b: &[u8], long: bool) -> Result<Self, Error> {
        let head = if long { 6 } else { 4 };
        if b.len() < head {
            return Err(Error::Short {
                what: "database string",
                need: head,
                have: b.len(),
            });
        }
        let ccsid = u16::from_be_bytes([b[0], b[1]]);
        let n = if long {
            u32::from_be_bytes([b[2], b[3], b[4], b[5]]) as usize
        } else {
            usize::from(u16::from_be_bytes([b[2], b[3]]))
        };
        if b.len() != head + n {
            return Err(Error::Invalid(format!(
                "database string says {n} bytes, has {}",
                b.len() - head
            )));
        }
        Ok(Self {
            ccsid,
            long,
            text: b[head..].to_vec(),
        })
    }

    /// The data.
    pub fn encode(&self) -> Vec<u8> {
        let mut v = self.ccsid.to_be_bytes().to_vec();
        if self.long {
            v.extend_from_slice(
                &u32::try_from(self.text.len())
                    .unwrap_or(u32::MAX)
                    .to_be_bytes(),
            );
        } else {
            v.extend_from_slice(
                &u16::try_from(self.text.len())
                    .unwrap_or(u16::MAX)
                    .to_be_bytes(),
            );
        }
        v.extend_from_slice(&self.text);
        v
    }

    /// The text, when it is UTF-16 (CCSID 13488 or 1200) or EBCDIC 37.
    pub fn to_text(&self) -> Option<String> {
        match self.ccsid {
            13488 | 1200 => {
                let units: Vec<u16> = self
                    .text
                    .chunks(2)
                    .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
                    .collect();
                String::from_utf16(&units).ok()
            }
            37 => Some(ccsid::from_ebcdic(&self.text)),
            _ => None,
        }
    }
}

/// The statement text of an SQL request: `0x3807` or `0x3831` (not
/// in exchange attributes, where `0x3807` is the date format option).
pub fn statement_text(params: &[Param]) -> Option<Result<DbString, Error>> {
    params.iter().find_map(|p| match p.cp {
        db_request::STATEMENT_TEXT => Some(DbString::decode(&p.data, false)),
        db_request::EXTENDED_STATEMENT_TEXT => Some(DbString::decode(&p.data, true)),
        _ => None,
    })
}

/// The parameters inside an RLE container (`0x3832`): its data is the
/// decompressed length (u32) and the RLE bytes, which expand to LL/CP
/// parameters.
pub fn decompress(p: &Param) -> Result<Vec<Param>, Error> {
    if p.cp != db_reply::COMPRESSED || p.data.len() < 4 {
        return Err(Error::Invalid("not an RLE container".into()));
    }
    let n = u32::from_be_bytes([p.data[0], p.data[1], p.data[2], p.data[3]]) as usize;
    let raw = rle::decompress(&p.data[4..])?;
    if raw.len() != n {
        return Err(Error::Invalid(format!(
            "RLE container expands to {} bytes, says {n}",
            raw.len()
        )));
    }
    let (params, tail) = param::split(&raw);
    if !tail.is_empty() {
        return Err(Error::Invalid(
            "RLE container holds a partial parameter".into(),
        ));
    }
    Ok(params)
}

/// A frame's parameters with every RLE container expanded in place.
pub fn expanded(params: &[Param]) -> Result<Vec<Param>, Error> {
    let mut out = Vec::new();
    for p in params {
        if p.cp == db_reply::COMPRESSED {
            out.extend(decompress(p)?);
        } else {
            out.push(p.clone());
        }
    }
    Ok(out)
}

template! {
    base 0;
    /// The fixed part of the server attributes (`0x3804`); the default
    /// schema name (its length is the last field) follows.
    pub struct ServerAttributes {
        /// Reserved (zero in the traces).
        reserved: u16,
        /// Date format parser option.
        date_format: u16,
        /// Date separator.
        date_separator: u16,
        /// Time format.
        time_format: u16,
        /// Time separator.
        time_separator: u16,
        /// Decimal separator.
        decimal_separator: u16,
        /// Naming convention (0 SQL, 1 system).
        naming: u16,
        /// Ignore decimal data errors.
        ignore_decimal_errors: u16,
        /// Commitment control level.
        commit_level: u16,
        /// DRDA package size.
        package_size: u16,
        /// Translation indicator (EBCDIC `0`/`1`).
        translation: u8,
        /// Server job CCSID.
        server_ccsid: u16,
        /// NLSS sort sequence value.
        nlss: u16,
        /// Language ID (3 EBCDIC).
        language_id: [u8; 3],
        /// Language table (10 EBCDIC).
        language_table: [u8; 10],
        /// Language table library.
        language_table_library: [u8; 10],
        /// Language feature code (4 EBCDIC digits).
        language_feature: [u8; 4],
        /// Server functional level (`V7R1M01400`, EBCDIC).
        functional_level: [u8; 10],
        /// Relational database name.
        rdb_name: [u8; 18],
        /// Default SQL library.
        default_library: [u8; 10],
        /// Server job: name, user, number (26 EBCDIC).
        job: [u8; 26],
        /// Default schema name length.
        schema_length: u16,
    }
}
