// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Telnet tickets, kept by the storage service beside the session
//! tokens: 256 random bits for one profile, good for one 5250
//! automatic sign-on within their time. A ticket is used up by its
//! first use, accepted or not.

use std::collections::HashMap;

use qap_proto::Token;

/// How long a ticket lives.
pub const TICKET_SECS: u32 = 60;

/// Why a ticket was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Never issued, or used already.
    Unknown,
    /// Past its time.
    Expired,
    /// Issued to another profile.
    WrongUser,
    /// Offered over plain Telnet.
    NotTls,
    /// `QRMTSIGN` allows no automatic sign-on.
    NotAllowed,
    /// The profile cannot sign on now: disabled, `*NONE` password, or
    /// its password expired.
    Profile,
}

#[derive(Debug, Clone)]
struct Ticket {
    user: String,
    until: u64,
}

/// The tickets issued and not used.
#[derive(Debug, Default)]
pub struct Tickets {
    map: HashMap<[u8; 32], Ticket>,
}

impl Tickets {
    /// None.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep `token` for `user` until `now` plus [`TICKET_SECS`]; the
    /// expired ones go.
    pub fn issue(&mut self, token: &Token, user: &str, now: u64) {
        self.map.retain(|_, t| now < t.until);
        self.map.insert(
            token.0,
            Ticket {
                user: user.to_string(),
                until: now + u64::from(TICKET_SECS),
            },
        );
    }

    /// Use `token` for `user`: gone whatever the answer.
    pub fn redeem(
        &mut self,
        token: &Token,
        user: &str,
        now: u64,
        tls: bool,
    ) -> Result<(), Refused> {
        let t = self.map.remove(&token.0).ok_or(Refused::Unknown)?;
        if !tls {
            Err(Refused::NotTls)
        } else if now >= t.until {
            Err(Refused::Expired)
        } else if !t.user.eq_ignore_ascii_case(user.trim()) {
            Err(Refused::WrongUser)
        } else {
            Ok(())
        }
    }

    /// Tickets waiting.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether none wait.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

/// The Telnet server's whole check of `token` for `user`, as the
/// storage service makes it: the ticket used up whatever the answer;
/// then `QRMTSIGN` (`allowed`) and the profile (`profile_ok`: not
/// disabled, a password, not expired), so a ticket outliving its
/// profile's right to sign on signs nothing on.
///
/// # Errors
/// Why the ticket signs nothing on.
pub fn check(
    table: &mut Tickets,
    token: &Token,
    user: &str,
    now: u64,
    tls: bool,
    allowed: bool,
    profile_ok: impl FnOnce(&str) -> bool,
) -> Result<(), Refused> {
    table.redeem(token, user, now, tls)?;
    if !allowed {
        return Err(Refused::NotAllowed);
    }
    if !profile_ok(&user.trim().to_ascii_uppercase()) {
        return Err(Refused::Profile);
    }
    Ok(())
}

/// A ticket from the password variable: 64 hexadecimal characters, else
/// `None` (a password, which automatic sign-on never accepts in plain
/// text).
#[must_use]
pub fn parse_hex(b: &[u8]) -> Option<Token> {
    if b.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, pair) in b.chunks(2).enumerate() {
        let s = std::str::from_utf8(pair).ok()?;
        out[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(Token(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_use_expiry_user_and_tls() {
        let mut table = Tickets::new();
        let tok = |n: u8| Token([n; 32]);
        for n in 1..=4 {
            table.issue(&tok(n), "BOB", 100);
        }
        assert_eq!(table.redeem(&tok(1), "bob", 159, true), Ok(()));
        assert_eq!(
            table.redeem(&tok(1), "BOB", 159, true),
            Err(Refused::Unknown)
        );
        assert_eq!(
            table.redeem(&tok(2), "BOB", 160, true),
            Err(Refused::Expired)
        );
        assert_eq!(
            table.redeem(&tok(3), "ANN", 110, true),
            Err(Refused::WrongUser)
        );
        // Refused for plain Telnet, and used up all the same.
        assert_eq!(
            table.redeem(&tok(4), "BOB", 110, false),
            Err(Refused::NotTls)
        );
        assert_eq!(
            table.redeem(&tok(4), "BOB", 110, true),
            Err(Refused::Unknown)
        );
        assert!(table.is_empty());
    }

    /// The service's path: an expired ticket, and a good ticket for a
    /// profile whose password expired, sign nothing on.
    #[test]
    fn the_whole_check() {
        let mut table = Tickets::new();
        let tok = |n: u8| Token([n; 32]);
        for n in 1..=4 {
            table.issue(&tok(n), "BOB", 100);
        }
        let ok = |_: &str| true;
        assert_eq!(
            check(&mut table, &tok(1), "BOB", 110, true, true, ok),
            Ok(())
        );
        assert_eq!(
            check(&mut table, &tok(2), "BOB", 160, true, true, ok),
            Err(Refused::Expired)
        );
        let expired = |u: &str| u != "BOB";
        assert_eq!(
            check(&mut table, &tok(3), "bob", 110, true, true, expired),
            Err(Refused::Profile)
        );
        assert_eq!(
            check(&mut table, &tok(4), "BOB", 110, true, false, ok),
            Err(Refused::NotAllowed)
        );
        assert!(table.is_empty(), "used up whatever the answer");
    }

    #[test]
    fn hex_form() {
        let hex = "00ff".repeat(16);
        let tok = parse_hex(hex.as_bytes()).unwrap();
        assert_eq!(tok.0[1], 0xFF);
        assert!(parse_hex(b"SECRET1").is_none());
        assert!(parse_hex("zz".repeat(32).as_bytes()).is_none());
        assert!(parse_hex("ab".repeat(31).as_bytes()).is_none());
    }
}
