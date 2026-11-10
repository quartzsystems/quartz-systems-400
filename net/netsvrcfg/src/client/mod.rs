// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NetServer administration as an API caller does it: every read is a
//! `QZLSOLST` list and every change a `QZLS*` call, so a page or a
//! protocol operation gets exactly the authority checks, validation
//! and messages a program calling the APIs gets. [`Api`] carries one
//! call (to the storage service in a job, to a [`MemHost`] in tests).

mod decode;
mod request;

pub use decode::{attributes, shares};

use crate::api::{self, MemHost, ServerControl};
use crate::fmt::{list, qsl, text};
use crate::model::attrs::{Attributes, Settings};
use crate::model::share::Share;

/// A failed call: the message identifier and its replacement data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// `CPFB68A`, ...
    pub id: String,
    /// The data, EBCDIC and big-endian, the API's name first.
    pub data: Vec<u8>,
}

impl Failure {
    /// Text field `n` bytes at `off` of the data.
    #[must_use]
    pub fn text_at(&self, off: usize, n: usize) -> String {
        text::text_at(&self.data, off, n)
    }

    /// The 4-byte integer at `off`.
    #[must_use]
    pub fn bin4(&self, off: usize) -> i32 {
        text::bin4(&self.data, off)
    }

    /// The API refused for a missing special authority or ownership
    /// (`CPFB684`, `CPDB680`).
    #[must_use]
    pub fn not_authorized(&self) -> bool {
        matches!(self.id.as_str(), "CPFB684" | "CPDB680")
    }
}

/// One API call: the API's name and its parameters' bytes in order (the
/// error code left out); the answer's bytes.
pub trait Api {
    /// Call `api`.
    ///
    /// # Errors
    /// The API's error message and data.
    fn call(&mut self, api: &str, parms: Vec<Vec<u8>>) -> Result<Vec<u8>, Failure>;
}

impl<F: FnMut(&str, Vec<Vec<u8>>) -> Result<Vec<u8>, Failure>> Api for F {
    fn call(&mut self, api: &str, parms: Vec<Vec<u8>>) -> Result<Vec<u8>, Failure> {
        self(api, parms)
    }
}

impl<S: ServerControl> Api for MemHost<S> {
    fn call(&mut self, name: &str, parms: Vec<Vec<u8>>) -> Result<Vec<u8>, Failure> {
        api::call(self, name, &parms).map_err(|e| Failure {
            id: e.id.to_string(),
            data: e.data,
        })
    }
}

/// The receiver length lists ask for.
const RECEIVER: i32 = 512 * 1024;

/// `QZLSOLST` of `format` for `qualifier`: the records' bytes (after
/// the list information) and the record count.
fn olst(a: &mut dyn Api, format: &str, qualifier: &str) -> Result<(Vec<u8>, usize), Failure> {
    let parms = vec![
        RECEIVER.to_be_bytes().to_vec(),
        text::fixed(format, 8),
        text::fixed(qualifier, 15),
    ];
    let out = a.call("QZLSOLST", parms)?;
    let n = usize::try_from(text::bin4(&out, 4)).unwrap_or(0);
    let data = out.get(list::LIST_INFO_SIZE..).unwrap_or(&[]).to_vec();
    Ok((data, n))
}

/// Every share, by name, with paths in CCSID `ccsid` (the caller's
/// job's).
///
/// # Errors
/// The API's message.
pub fn list_shares(a: &mut dyn Api, ccsid: u16) -> Result<Vec<Share>, Failure> {
    let (data, n) = olst(a, "ZLSL0101", "*ALL")?;
    Ok(shares(&data, n, ccsid))
}

/// The share named `name`, when there is one.
///
/// # Errors
/// The API's message.
pub fn share(a: &mut dyn Api, name: &str, ccsid: u16) -> Result<Option<Share>, Failure> {
    let (data, n) = olst(a, "ZLSL0101", &name.to_ascii_uppercase())?;
    Ok(shares(&data, n, ccsid).into_iter().next())
}

/// The attributes, current and pending.
///
/// # Errors
/// The API's message.
pub fn read_attributes(a: &mut dyn Api) -> Result<Attributes, Failure> {
    let (data, _) = olst(a, "ZLSL0201", "*ALL")?;
    let mut at = attributes(&data);
    let (own, _) = olst(a, "QSLL0100", "*ALL")?;
    let (c, p) = qsl::read_l0100(&own);
    c.apply(&mut at.current);
    p.apply(&mut at.pending);
    Ok(at)
}

/// The users disabled for NetServer (`ZLSL0900`), by name.
///
/// # Errors
/// The API's message.
pub fn disabled_users(a: &mut dyn Api) -> Result<Vec<String>, Failure> {
    let (data, n) = olst(a, "ZLSL0900", "")?;
    Ok((0..n)
        .map(|i| text::text_at(&data, i * 10, 10))
        .filter(|u| !u.is_empty())
        .collect())
}

/// Add `s` (`QZLSADFS` or `QZLSADPS`).
///
/// # Errors
/// The API's message.
pub fn add_share(a: &mut dyn Api, s: &Share) -> Result<(), Failure> {
    let (api, parms) = request::share(s, true);
    a.call(api, parms).map(drop)
}

/// Change share `s` to what it holds (`QZLSCHFS` or `QZLSCHPS`).
///
/// # Errors
/// The API's message.
pub fn change_share(a: &mut dyn Api, s: &Share) -> Result<(), Failure> {
    let (api, parms) = request::share(s, false);
    a.call(api, parms).map(drop)
}

/// Remove share `name` (`QZLSRMS`).
///
/// # Errors
/// The API's message.
pub fn remove_share(a: &mut dyn Api, name: &str) -> Result<(), Failure> {
    a.call("QZLSRMS", vec![text::fixed(name, 12)]).map(drop)
}

/// Make `to` the pending attributes, from `from` (the pending ones
/// now): `QZLSCHSN` when the name, domain, text or system name access
/// changed, `QZLSCHSG` for the guest, `QZLSCHSI` `ZLSS0100` for the
/// others IBM's format has and `QSLS0100` for QS/400's own. Stops at
/// the first refusal; what was called before it stays.
///
/// # Errors
/// The API's message.
pub fn change_attributes(a: &mut dyn Api, from: &Settings, to: &Settings) -> Result<(), Failure> {
    for (api, parms) in request::attributes(from, to) {
        a.call(api, parms)?;
    }
    Ok(())
}

/// Start the server (`QZLSSTRS`); `reset` makes the pending attributes
/// current.
///
/// # Errors
/// The API's message.
pub fn start(a: &mut dyn Api, reset: bool) -> Result<(), Failure> {
    let r = text::fixed(if reset { "1" } else { "0" }, 1);
    a.call("QZLSSTRS", vec![r]).map(drop)
}

/// End the server (`QZLSENDS`).
///
/// # Errors
/// The API's message.
pub fn end(a: &mut dyn Api) -> Result<(), Failure> {
    a.call("QZLSENDS", Vec::new()).map(drop)
}

/// End session `id` (`QZLSENSS *SESSID`).
///
/// # Errors
/// The API's message.
pub fn end_session(a: &mut dyn Api, id: u64) -> Result<(), Failure> {
    let id = i64::try_from(id).unwrap_or(i64::MAX);
    a.call(
        "QZLSENSS",
        vec![text::fixed("*SESSID", 15), id.to_be_bytes().to_vec()],
    )
    .map(drop)
}

/// Enable `users` for NetServer again (`QZLSCHSI` `ZLSS0200`).
///
/// # Errors
/// The API's message.
pub fn enable_users(a: &mut dyn Api, users: &[String]) -> Result<(), Failure> {
    let b = request::s0200(users);
    let len = i32::try_from(b.len()).unwrap_or(i32::MAX);
    a.call(
        "QZLSCHSI",
        vec![b, len.to_be_bytes().to_vec(), text::fixed("ZLSS0200", 8)],
    )
    .map(drop)
}

#[cfg(test)]
mod tests;
