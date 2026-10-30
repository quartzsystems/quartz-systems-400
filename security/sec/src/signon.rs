// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signing on at a display: the profile and password checked, the
//! sign-on limits applied, the attempt counted and audited. A user that
//! does not exist and a wrong password read the same (`CPF1107`), so
//! user names cannot be found out from the sign-on display.

use ipc::{wire_enum, wire_struct};
use store::obj::ObjectId;
use store::txn::Txn;

use crate::audit::{Event, Kind};
use crate::auth::bits::Auth;
use crate::password::policy::Policy;
use crate::password::{form_for, verify};
use crate::profile::limits;
use crate::profile::objects;
use crate::profile::payload::UsrPrf;
use crate::profile::spcaut::SpecialAuth;
use crate::{Result, SecError};

/// The profile that may always sign on at the console, even disabled.
pub const SECOFR: &str = "QSECOFR";

/// What a sign-on needs to know besides the store.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Attempt {
    /// User typed.
    pub user: String,
    /// Password typed.
    pub password: String,
    /// The display device.
    pub device: String,
    /// The device is the system console.
    pub console: bool,
    /// Interactive jobs the user has active now.
    pub sessions: u32,
    /// Unix seconds.
    pub now: u64,
    /// An RFC 4777 substitute in place of the password (auto-signon).
    pub proof: Option<Substitute>,
}

/// What a TN5250E client sent to prove its password: the seeds both
/// sides used and the substitute it computed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Substitute {
    /// The server's seed.
    pub server_seed: [u8; 8],
    /// The client's seed.
    pub client_seed: [u8; 8],
    /// The substitute: eight bytes (DES) or twenty (SHA-1).
    pub substitute: Vec<u8>,
}

wire_struct!(Substitute {
    server_seed,
    client_seed,
    substitute
});

/// What the display does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Next {
    /// Start the job.
    #[default]
    Start,
    /// The password expired: Change Password first.
    ChangePassword,
}

wire_enum!(Next {
    0 => Start,
    1 => ChangePassword,
});

/// A sign-on that succeeded.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SignedOn {
    /// The profile name.
    pub user: String,
    /// What to do next.
    pub next: Next,
    /// Show Sign-on Information (`DSPSGNINF`, `QDSPSGNINF`).
    pub show_info: bool,
    /// The sign-on before this one, Unix seconds, 0 for none.
    pub previous: u64,
    /// Attempts not valid since then.
    pub invalid: u32,
    /// Days until the password expires, when it warns.
    pub expires_in: Option<i64>,
    /// `INLPGM`, `lib/name` or `*NONE`.
    pub inlpgm: String,
    /// `INLMNU`, `lib/name` or `*SIGNOFF`.
    pub inlmnu: String,
    /// `CURLIB`.
    pub curlib: String,
    /// `JOBD`, `lib/name`.
    pub jobd: String,
    /// `LMTCPB`: `*NO`, `*PARTIAL`, `*YES`.
    pub lmtcpb: String,
    /// `ATNPGM`, `lib/name` or a special value.
    pub atnpgm: String,
    /// `MSGQ`, `lib/name`.
    pub msgq: String,
    /// `DLVRY`.
    pub dlvry: String,
}

wire_struct!(SignedOn {
    user,
    next,
    show_info,
    previous,
    invalid,
    expires_in,
    inlpgm,
    inlmnu,
    curlib,
    jobd,
    lmtcpb,
    atnpgm,
    msgq,
    dlvry
});

/// What the sign-on values say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Values {
    /// `QMAXSIGN`, `None` for `*NOMAX`.
    pub maxsign: Option<u32>,
    /// `QMAXSGNACN` 1 device, 2 profile, 3 both.
    pub maxsgnacn: u8,
    /// `QLMTSECOFR`.
    pub lmtsecofr: bool,
    /// `QLMTDEVSSN`.
    pub lmtdevssn: bool,
    /// `QDSPSGNINF`.
    pub dspsgninf: bool,
    /// The password values.
    pub policy: Policy,
}

impl Values {
    /// From the system values.
    pub fn from_sysvals(get: &dyn Fn(&str) -> String) -> Self {
        let on = |n: &str| get(n).trim() == "1";
        Self {
            maxsign: get("QMAXSIGN").trim().parse().ok(),
            maxsgnacn: get("QMAXSGNACN").trim().parse().unwrap_or(3),
            lmtsecofr: on("QLMTSECOFR"),
            lmtdevssn: on("QLMTDEVSSN"),
            dspsgninf: on("QDSPSGNINF"),
            policy: Policy::from_sysvals(get),
        }
    }
}

/// A sign-on that failed: the message, and whether the device is to be
/// varied off (`QMAXSGNACN` 1 or 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    /// Message identifier.
    pub msgid: String,
    /// Quartz's text.
    pub text: String,
    /// Vary the device off.
    pub vary_off: bool,
}

wire_struct!(Refused {
    msgid,
    text,
    vary_off
});

fn refused(msgid: &str, text: impl Into<String>) -> Refused {
    Refused {
        msgid: msgid.into(),
        text: text.into(),
        vary_off: false,
    }
}

fn pw(etyp: &str, user: &str, device: &str) {
    crate::audit::record(
        Event::new(Kind::Pw)
            .set("ETYP", etyp)
            .set("USRP", user)
            .set("DEVN", device)
            .by(user),
    );
}

/// Sign on. `device_authority` answers what the user has to the device
/// description (for `QLMTSECOFR`): its private or owner authority only.
pub fn sign_on(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    a: &Attempt,
    v: &Values,
    device_authority: &dyn Fn(&mut Txn<'_>, &str) -> Option<Auth>,
) -> Result<std::result::Result<SignedOn, Refused>> {
    let user = a.user.trim().to_ascii_uppercase();
    let not_correct = || refused("CPF1107", "Password or user profile is not correct.");
    let Ok(Some(id)) = objects::find(txn, qsys, &user) else {
        pw("U", &user, &a.device);
        return Ok(Err(not_correct()));
    };
    let mut p = objects::read_id(txn, id)?;
    let exempt = a.console && user == SECOFR;
    if limits::user_expired(&p, a.now) && !p.disabled && !exempt {
        p.disabled = true;
        objects::write(txn, id, &p)?;
    }
    if p.disabled && !exempt {
        return Ok(Err(refused(
            "CPF1394",
            format!("User profile {user} is disabled and cannot sign on."),
        )));
    }
    if p.pwd_none {
        return Ok(Err(refused(
            "CPF1118",
            format!("User profile {user} has no password and cannot sign on."),
        )));
    }
    let form = form_for(v.policy.pwdlvl);
    let proven = match &a.proof {
        Some(s) => crate::password::verify_substitute(&p, &user, s, v.policy.pwdlvl),
        None => p.hash(form).is_some_and(|h| verify(h, &a.password)),
    };
    if !proven {
        pw("P", &user, &a.device);
        p.signon_invalid += 1;
        let r = match v.maxsign {
            Some(max) if p.signon_invalid >= max && !exempt => {
                let mut r = refused(
                    "CPF1393",
                    format!(
                        "Too many sign-on attempts were not valid; user profile {user} or the device is disabled."
                    ),
                );
                if matches!(v.maxsgnacn, 2 | 3) {
                    p.disabled = true;
                }
                r.vary_off = matches!(v.maxsgnacn, 1 | 3) && !a.console;
                r
            }
            Some(max) if p.signon_invalid + 1 == max && !exempt => refused(
                "CPF1392",
                "Password or user profile is not correct. The next attempt that is not valid disables the profile or the device.",
            ),
            _ => not_correct(),
        };
        objects::write(txn, id, &p)?;
        return Ok(Err(r));
    }
    // Security officers sign on only where they are authorized by name;
    // a sign-on without a device (a host server) has no device to limit.
    let strong = p.spcaut.has(SpecialAuth::ALLOBJ) || p.spcaut.has(SpecialAuth::SERVICE);
    let display = !a.device.is_empty();
    if v.lmtsecofr && strong && !exempt && display {
        let ok = device_authority(txn, &user).is_some_and(|b| b.has(Auth::CHANGE));
        if !ok {
            return Ok(Err(refused(
                "CPF1110",
                format!(
                    "User profile {user} is not authorized to sign on at device {}.",
                    a.device
                ),
            )));
        }
    }
    let limited = p.lmtdevssn.resolve(v.lmtdevssn);
    if limited && a.sessions > 0 && display {
        return Ok(Err(refused(
            "CPF1220",
            format!("User profile {user} is signed on at another device already."),
        )));
    }
    let invalid = p.signon_invalid;
    let previous = p.last_signon;
    p.signon_invalid = 0;
    p.prev_signon = p.last_signon;
    p.last_signon = a.now;
    objects::write(txn, id, &p)?;
    Ok(Ok(signed_on(&p, v, a.now, previous, invalid)))
}

fn qual((n, l): &(String, String)) -> String {
    if n.starts_with('*') || l.is_empty() || l == "*LIBL" {
        n.clone()
    } else {
        format!("{l}/{n}")
    }
}

fn signed_on(p: &UsrPrf, v: &Values, now: u64, previous: u64, invalid: u32) -> SignedOn {
    let expired = v.policy.expired(p, now);
    SignedOn {
        user: p.name.clone(),
        next: if expired {
            Next::ChangePassword
        } else {
            Next::Start
        },
        show_info: p.dspsgninf.resolve(v.dspsgninf),
        previous,
        invalid,
        expires_in: v.policy.warn(p, now),
        inlpgm: qual(&p.inlpgm),
        inlmnu: qual(&p.inlmnu),
        curlib: p.curlib.clone(),
        jobd: qual(&p.jobd),
        lmtcpb: p.lmtcpb.name().into(),
        atnpgm: qual(&p.atnpgm),
        msgq: if p.msgq.0 == "*USRPRF" {
            format!("QUSRSYS/{}", p.name)
        } else {
            qual(&p.msgq)
        },
        dlvry: p.dlvry.clone(),
    }
}

/// The three fields of the Change Password display.
#[derive(Debug, Clone, Copy)]
pub struct Typed<'a> {
    /// The current password.
    pub current: &'a str,
    /// The new one.
    pub new: &'a str,
    /// The new one again.
    pub confirm: &'a str,
}

/// The password changed on the Change Password display after an
/// expired sign-on; then the job starts.
pub fn change_expired(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    user: &str,
    typed: Typed<'_>,
    v: &Values,
    now: u64,
    validate: &mut crate::password::policy::Validator<'_>,
) -> Result<SignedOn> {
    let Typed {
        current,
        new,
        confirm,
    } = typed;
    if new != confirm {
        return Err(SecError::msg(
            "CPF2361",
            "The new password and its confirmation differ.",
        ));
    }
    let (id, mut p) = objects::read(txn, qsys, user)?;
    crate::password::policy::set(
        &mut p,
        new,
        crate::password::policy::By::User { current },
        &v.policy,
        now,
        validate,
    )?;
    p.changed = now;
    objects::write(txn, id, &p)?;
    crate::audit::record(
        Event::new(Kind::Cp)
            .set("CMDN", "CHGPWD")
            .set("USRP", user)
            .set("PWDC", "Y"),
    );
    Ok(signed_on(&p, v, now, p.prev_signon, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire() {
        let s = SignedOn {
            user: "BOB".into(),
            next: Next::ChangePassword,
            expires_in: Some(3),
            ..SignedOn::default()
        };
        let back: SignedOn = ipc::decode(&ipc::encode(&s)).unwrap();
        assert_eq!(back, s);
    }
}
