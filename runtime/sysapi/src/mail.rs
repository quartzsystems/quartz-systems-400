// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QtmmSendMail`, the Send MIME Mail API: a stream file holding a
//! whole MIME message goes to the recipients given as `ADDT0100`
//! structures. The call is OS linkage (every argument by address):
//! the file name and its length, the originator and its length, the
//! first recipient structure, the number of recipients, the error
//! code. Names are in the job's CCSID. The job process supplies the
//! [`Mailer`] that reads the file and hands it to the mail router; a
//! job without one fails the call.

use std::cell::RefCell;
use std::rc::Rc;

use mi::Job;

use crate::net::abi::{bound_c, fat};
use crate::param::{ErrCode, bin4_at};
use crate::provider::ApiFail;

/// The message for every failure of the call (`QTCPMSG`).
pub const NOT_SENT: &str = "TCP1A4C";

/// `ADDT0100`'s fixed part.
pub const ADDT0100_LEN: usize = 24;

/// What a call asks: the file, the originator, the recipients with
/// their distribution type (0 normal, 1 copy, 2 blind copy).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MailRequest {
    /// The stream file's path.
    pub file: String,
    /// The originator's address.
    pub originator: String,
    /// The recipients.
    pub recipients: Vec<(String, i32)>,
}

/// Sends a request; why not as the text of the failure.
pub type Mailer = Rc<dyn Fn(&MailRequest) -> Result<(), String>>;

thread_local! {
    static MAILER: RefCell<Option<Mailer>> = const { RefCell::new(None) };
}

/// Set (or clear) this thread's mailer.
pub fn set_mailer(m: Option<Mailer>) {
    MAILER.with(|c| *c.borrow_mut() = m);
}

/// The procedures of this module with their code addresses.
pub(crate) fn procedures() -> Vec<(&'static str, usize)> {
    vec![("QtmmSendMail", b_qtmm as *const () as usize)]
}

bound_c!(b_qtmm(file: u128, flen: u128, orig: u128, olen: u128, first: u128, total: u128, err: u128) -> i64 = qtmm);

/// The address a pointer argument names, 0 for null or dead.
fn addr(j: &Job, p: u128) -> u64 {
    if fat(p).is_null() {
        return 0;
    }
    j.spaces.extent(fat(p)).map_or(0, |(at, _)| at)
}

/// `n` bytes at `at`, or nothing when `at` is 0.
fn bytes(at: u64, n: usize) -> Vec<u8> {
    if at == 0 || n == 0 {
        return Vec::new();
    }
    mi::storage::view(at, n).to_vec()
}

fn int_at(at: u64) -> i32 {
    if at == 0 { 0 } else { bin4_at(mi::storage::view(at, 4), 0) }
}

/// The recipients from the first `ADDT0100` structure: each one's
/// address follows its fixed part; the next is `Next_Addr_Structure_Offset`
/// bytes on from the start of the current one.
#[must_use]
pub fn recipients(first: u64, total: usize) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    let mut at = first;
    for _ in 0..total.min(10_000) {
        if at == 0 {
            break;
        }
        let head = bytes(at, ADDT0100_LEN);
        let next = bin4_at(&head, 0);
        let len = usize::try_from(bin4_at(&head, 4)).unwrap_or(0).min(256);
        let dist = bin4_at(&head, 16);
        let address = ccsid::decode_trimmed(&bytes(at + ADDT0100_LEN as u64, len));
        out.push((address, dist));
        let step = u64::try_from(next).ok().filter(|n| *n > 0).unwrap_or((ADDT0100_LEN + len) as u64);
        at += step;
    }
    out
}

/// The request a call's arguments make.
#[must_use]
pub fn request(file: u64, flen: u64, orig: u64, olen: u64, first: u64, total: u64) -> MailRequest {
    let fl = usize::try_from(int_at(flen)).unwrap_or(0).min(255);
    let ol = usize::try_from(int_at(olen)).unwrap_or(0).min(256);
    let n = usize::try_from(int_at(total)).unwrap_or(0);
    MailRequest {
        file: ccsid::decode_trimmed(&bytes(file, fl)),
        originator: ccsid::decode_trimmed(&bytes(orig, ol)),
        recipients: recipients(first, n),
    }
}

#[allow(clippy::too_many_arguments)]
fn qtmm(j: &mut Job, file: u128, flen: u128, orig: u128, olen: u128, first: u128, total: u128, err: u128) -> i64 {
    let e = ErrCode::from_addr(addr(j, err));
    let req = request(addr(j, file), addr(j, flen), addr(j, orig), addr(j, olen), addr(j, first), addr(j, total));
    let r = if req.file.is_empty() {
        Err("no file name".to_string())
    } else if req.recipients.is_empty() {
        Err("no recipients".to_string())
    } else {
        match MAILER.with(|c| c.borrow().clone()) {
            Some(m) => m(&req),
            None => Err("this job cannot send mail".to_string()),
        }
    };
    match r {
        Ok(()) => e.ok(),
        Err(why) => e.fail(j, &ApiFail::id(NOT_SENT).text(&why, 128)),
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Storage at a stable address for the decoder.
    fn place(b: &[u8]) -> (Box<[u8]>, u64) {
        let keep: Box<[u8]> = b.into();
        let at = keep.as_ptr() as u64;
        (keep, at)
    }

    fn addt(addr: &str, dist: i32, next: i32) -> Vec<u8> {
        let a = ccsid::to_ebcdic(addr).unwrap();
        let mut v = Vec::new();
        v.extend(next.to_be_bytes());
        v.extend(i32::try_from(a.len()).unwrap().to_be_bytes());
        v.extend(ccsid::to_ebcdic("ADDR0100").unwrap());
        v.extend(dist.to_be_bytes());
        v.extend(0i32.to_be_bytes());
        v.extend(a);
        v
    }

    #[test]
    fn recipient_structures() {
        let one = addt("anne@example.com", 0, 0);
        let mut both = addt("anne@example.com", 0, 24 + 16 + 4);
        both.extend([0x40; 4]);
        both.extend(addt("bob@example.com", 2, 0));
        let (_k, at) = place(&both);
        assert_eq!(
            recipients(at, 2),
            [("anne@example.com".to_string(), 0), ("bob@example.com".to_string(), 2)]
        );
        // A zero offset: the next follows the address.
        let mut seq = one.clone();
        seq.extend(addt("c@x.org", 1, 0));
        let (_k2, at) = place(&seq);
        assert_eq!(recipients(at, 2)[1], ("c@x.org".to_string(), 1));
    }

    #[test]
    fn a_request_from_its_arguments() {
        let (_f, file) = place(&ccsid::to_ebcdic("/tmp/mail.eml").unwrap());
        let (_fl, flen) = place(&13i32.to_be_bytes());
        let (_o, orig) = place(&ccsid::to_ebcdic("qsecofr@qs400.example.com").unwrap());
        let (_ol, olen) = place(&25i32.to_be_bytes());
        let (_r, first) = place(&addt("anne@example.com", 0, 0));
        let (_t, total) = place(&1i32.to_be_bytes());
        let r = request(file, flen, orig, olen, first, total);
        assert_eq!(r.file, "/tmp/mail.eml");
        assert_eq!(r.originator, "qsecofr@qs400.example.com");
        assert_eq!(r.recipients, [("anne@example.com".to_string(), 0)]);
    }
}
