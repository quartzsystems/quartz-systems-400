// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `NetServer` sign-on: an `NTLMv2` response checked against the
//! profile's NT hash, in the storage service. The hash is unsealed only
//! here; what goes back is the profile and the session base key.

use ipc::{wire_enum, wire_struct};
use store::obj::ObjectId;
use store::txn::Txn;

use crate::Result;
use crate::audit::{Event, Kind};
use crate::netsvr::users::NetUsers;
use crate::password::nthash::{self, NtSealer};
use crate::profile::{limits, objects};
use crate::signon::Values;

/// What the client sent in its `AUTHENTICATE` message.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Logon {
    /// The user name as sent.
    pub user: String,
    /// The domain as sent.
    pub domain: String,
    /// The server's challenge from the `CHALLENGE` message.
    pub server_challenge: [u8; 8],
    /// The `NTLMv2` response (`NtChallengeResponse`).
    pub nt_response: Vec<u8>,
    /// The guest profile, when one is configured (`QZLSCHSG`).
    pub guest: Option<String>,
    /// The client's name or address, for the audit entry.
    pub client: String,
}

wire_struct!(Logon {
    user,
    domain,
    server_challenge,
    nt_response,
    guest,
    client
});

/// A sign-on that succeeded.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoggedOn {
    /// The profile the session runs as.
    pub user: String,
    /// It is the guest profile (an unknown user).
    pub guest: bool,
    /// The `NTLMv2` session base key; `None` for a guest, which proves
    /// nothing and so has no key to sign with.
    pub session_base_key: Option<[u8; 16]>,
}

wire_struct!(LoggedOn {
    user,
    guest,
    session_base_key
});

/// Why a sign-on failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// No profile by the name, and no guest profile to use.
    NoSuchUser,
    /// The profile is `*DISABLED` (or past `USREXPDATE`).
    Disabled,
    /// The password has expired.
    Expired,
    /// `PASSWORD(*NONE)`.
    NoPassword,
    /// A password but no NT hash: set before V1R2M0, restored from
    /// another system or through `QSYSUPWD`. A password change makes one.
    NoNtHash,
    /// Disabled for `NetServer` (`QMAXSIGN` passwords not valid).
    NetServerDisabled,
    /// The response does not prove the password; `disabled_now` when
    /// this one reached `QMAXSIGN`.
    WrongPassword {
        /// This attempt disabled the user for `NetServer`.
        disabled_now: bool,
    },
    /// Not an `NTLMv2` response (`NTLMv1`, LM): refused, not counted.
    NotNtlmV2,
}

wire_enum!(Refusal {
    1 => NoSuchUser,
    2 => Disabled,
    3 => Expired,
    4 => NoPassword,
    5 => NoNtHash,
    6 => NetServerDisabled,
    7 => WrongPassword { disabled_now },
    8 => NotNtlmV2,
});

impl Refusal {
    /// The `QCPFMSG` message `NetServer` logs for it, if any.
    #[must_use]
    pub fn msgid(self) -> Option<&'static str> {
        Some(match self {
            Self::NoSuchUser => "CPIB68A",
            Self::Disabled => "CPIB68B",
            Self::Expired => "CPIB68C",
            Self::NoPassword | Self::NoNtHash => "CPIB68D",
            Self::NetServerDisabled => "CPIB68E",
            Self::WrongPassword { .. } | Self::NotNtlmV2 => return None,
        })
    }
}

/// The message to `QSYSOPR` when a user is disabled for `NetServer`.
pub const DISABLED_MSGID: &str = "CPIB682";

/// `MMDDYY` and `HHMMSS` of Unix seconds (UTC).
fn date_time(now: u64) -> (String, String) {
    let days = i64::try_from(now / 86_400).unwrap_or(0);
    let secs = now % 86_400;
    // Civil date from days since 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        format!("{m:02}{d:02}{:02}", y.rem_euclid(100)),
        format!("{:02}{:02}{:02}", secs / 3600, secs / 60 % 60, secs % 60),
    )
}

/// The `VP` entry: `P` a password not valid, `D` the user disabled.
fn vp(kind: &str, user: &str, now: u64) {
    let (date, time) = date_time(now);
    crate::audit::record(
        Event::new(Kind::Vp)
            .set("TYPE", kind)
            .set("NWSD", "*NETSERVER")
            .set("SDAT", date)
            .set("STIM", time)
            .set("PROF", user),
    );
}

/// The guest profile, when it can be used: it exists, is enabled and
/// has no special authority.
fn guest(txn: &mut Txn<'_>, qsys: ObjectId, name: &str, now: u64) -> Option<String> {
    let name = name.trim().to_ascii_uppercase();
    let id = objects::find(txn, qsys, &name).ok().flatten()?;
    let p = objects::read_id(txn, id).ok()?;
    let usable = !p.disabled && !limits::user_expired(&p, now) && p.spcaut.0 == 0;
    usable.then_some(name)
}

/// Sign `l` on at `now`. `users` keeps the `NetServer` counts, `sealer`
/// opens the NT hashes. Nothing in the profile changes.
pub fn sign_on(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    l: &Logon,
    v: &Values,
    users: &NetUsers,
    sealer: Option<&dyn NtSealer>,
    now: u64,
) -> Result<std::result::Result<LoggedOn, Refusal>> {
    let name = l.user.trim().to_ascii_uppercase();
    let found = if name.is_empty() || name.chars().count() > 10 {
        None
    } else {
        objects::find(txn, qsys, &name).ok().flatten()
    };
    let Some(id) = found else {
        // An unknown user is the guest when there is one; no entry is
        // written (IBM writes none for a user that does not exist).
        return Ok(match l.guest.as_deref().and_then(|g| guest(txn, qsys, g, now)) {
            Some(g) => Ok(LoggedOn {
                user: g,
                guest: true,
                session_base_key: None,
            }),
            None => Err(Refusal::NoSuchUser),
        });
    };
    let p = objects::read_id(txn, id)?;
    if p.disabled || limits::user_expired(&p, now) {
        return Ok(Err(Refusal::Disabled));
    }
    if users.is_disabled(&name) {
        return Ok(Err(Refusal::NetServerDisabled));
    }
    if p.pwd_none {
        return Ok(Err(Refusal::NoPassword));
    }
    if !nthash::has(&p) {
        return Ok(Err(Refusal::NoNtHash));
    }
    if l.nt_response.len() < ntlm::MIN_NTLMV2_RESPONSE {
        return Ok(Err(Refusal::NotNtlmV2));
    }
    let key = nthash::open_with(sealer, &p).iter().find_map(|h| {
        ntlm::verify_ntlmv2(h, &l.user, &l.domain, &l.server_challenge, &l.nt_response)
    });
    let Some(key) = key else {
        vp("P", &name, now);
        let disabled_now = users.failed(&name, v.maxsign, now);
        if disabled_now {
            vp("D", &name, now);
        }
        return Ok(Err(Refusal::WrongPassword { disabled_now }));
    };
    if v.policy.expired(&p, now) {
        return Ok(Err(Refusal::Expired));
    }
    users.succeeded(&name);
    Ok(Ok(LoggedOn {
        user: name,
        guest: false,
        session_base_key: Some(key),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date_time(0), ("010170".into(), "000000".into()));
        // 2026-11-09 13:45:07 UTC.
        assert_eq!(
            date_time(1_794_231_907),
            ("110926".into(), "134507".into())
        );
    }
}
