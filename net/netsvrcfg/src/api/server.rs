// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server APIs: `QZLSCHSN` (name, domain, text), `QZLSCHSG`
//! (guest), `QZLSCHSI` (`ZLSS0100` attributes, `ZLSS0200` users to
//! enable), `QZLSSTRS`, `QZLSENDS` and `QZLSENSS` (end sessions).
//! Attribute changes are pending until a start with reset or the next
//! IPL.

use super::error::{ApiError, NOT_SUPPORTED_RC, authority, configure, end_session, msg, start};
use super::host::{Enabled, GuestCheck, Host};
use super::parms::Parms;
use crate::fmt::qsl;
use crate::fmt::zlss::{self, S0100_MIN, S0100_SIZE};
use crate::model::attrs::{Dialect, Settings};
use crate::model::valid;

fn need_iosyscfg(api: &str, h: &mut dyn Host) -> Result<(), ApiError> {
    if h.caller().iosyscfg {
        Ok(())
    } else {
        Err(ApiError::of(msg::AUTHORITY, api).bin4(authority::IOSYSCFG))
    }
}

fn bad_char(api: &str, parm: i32, value: &str) -> ApiError {
    ApiError::of(msg::CHARACTER, api).bin4(parm).text(value, 15)
}

/// `QZLSCHSN(server name 15, domain 15, text 50 [, allow system name 1])`.
pub fn change_name(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSCHSN";
    need_iosyscfg(API, h)?;
    let server = valid::netbios_name(&p.text(0)).map_err(|_| bad_char(API, 1, &p.text(0)))?;
    let domain = valid::netbios_name(&p.text(1)).map_err(|_| bad_char(API, 2, &p.text(1)))?;
    let text = p.text(2);
    let allow = match p.ch(3) {
        None => None,
        Some('0') => Some(false),
        Some('1') => Some(true),
        Some(c) => return Err(bad_char(API, 4, &c.to_string())),
    };
    let mut c = h.load()?;
    let s = &mut c.attrs.pending;
    s.server_name = server;
    s.domain = domain;
    s.text = text;
    if let Some(a) = allow {
        s.system_name_access = a;
    }
    h.save(&c)?;
    Ok(Vec::new())
}

/// `QZLSCHSG(guest profile 10)`: blanks or `*NONE` remove guest support.
pub fn change_guest(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSCHSG";
    let who = h.caller();
    if !(who.iosyscfg && who.secadm) {
        return Err(ApiError::of(msg::AUTHORITY, API).bin4(authority::IOSYSCFG_SECADM));
    }
    let raw = p.text(0);
    let guest = if raw.is_empty() || raw == "*NONE" {
        String::new()
    } else {
        let n = valid::object_name(&raw).map_err(|_| bad_char(API, 1, &raw))?;
        match h.guest_check(&n) {
            GuestCheck::Usable => n,
            GuestCheck::NotFound => {
                return Err(ApiError::of(msg::GUEST_NOT_FOUND, API)
                    .text("*USE", 10)
                    .text(&n, 10));
            }
            GuestCheck::Special(sp) => {
                return Err(ApiError::of(msg::GUEST_SPECIAL, API)
                    .text(&n, 10)
                    .text(&sp, 10));
            }
        }
    };
    let mut c = h.load()?;
    c.attrs.pending.guest = guest;
    h.save(&c)?;
    Ok(Vec::new())
}

fn cfg_err(r: i32, rc: i32) -> ApiError {
    ApiError::of(msg::CONFIGURE, "QZLSCHSI").bin4(r).bin4(rc)
}

/// Check a `ZLSS0100` request against what QS/400 has and put it in
/// the pending settings.
fn apply_0100(h: &mut dyn Host, s: &mut Settings, r: &zlss::S0100) -> Result<(), ApiError> {
    if r.code_page != 0
        && u16::try_from(r.code_page)
            .ok()
            .map(ccsid::conv::ccsid_of_codepage)
            .is_none_or(|c| c == 0 || !ccsid::conv::supported().contains(&c))
    {
        return Err(cfg_err(configure::CODE_PAGE, 0));
    }
    if !r.wins_primary.is_empty() || !r.scope.is_empty() {
        return Err(cfg_err(configure::WINS_PRIMARY, NOT_SUPPORTED_RC));
    }
    if !r.wins_secondary.is_empty() {
        return Err(cfg_err(configure::WINS_SECONDARY, NOT_SUPPORTED_RC));
    }
    if !(0..=720_000).contains(&r.browse_interval) {
        return Err(cfg_err(configure::BROWSE, 0));
    }
    if r.oplock_timeout != -1 && !(1..=600).contains(&r.oplock_timeout) {
        return Err(cfg_err(configure::OPLOCK, 0));
    }
    if r.idle_timeout != -1 && r.idle_timeout <= 0 {
        return Err(cfg_err(configure::IDLE, 0));
    }
    match r.wins_proxy {
        '0' => {}
        '1' => return Err(cfg_err(configure::WINS_PRIMARY, NOT_SUPPORTED_RC)),
        _ => return Err(cfg_err(configure::WINS_PROXY, 0)),
    }
    if r.domain_logon != 0 || r.server_role != 0 {
        return Err(cfg_err(configure::ROLE, 0));
    }
    match r.auth_method {
        '0' => {}
        '1' | '2' => return Err(cfg_err(configure::KERBEROS, 0)),
        _ => return Err(cfg_err(configure::AUTH_METHOD, 0)),
    }
    if !(0..=2).contains(&r.message_auth) {
        return Err(cfg_err(configure::MESSAGE_AUTH, 0));
    }
    if r.min_message_severity != -1 {
        return Err(cfg_err(configure::SEVERITY, 0));
    }
    if r.lanman_auth != 0 {
        return Err(cfg_err(configure::LANMAN, 0));
    }
    if let Some(e) = r.encryption
        && !(0..=1).contains(&e)
    {
        return Err(ApiError::of(msg::CHARACTER, "QZLSCHSI")
            .bin4(1)
            .text(&e.to_string(), 15));
    }
    let autl = match &r.server_autl {
        Some(a) if a.is_empty() || a == "*NONE" => Some(String::new()),
        Some(a) => {
            let n = valid::object_name(a).map_err(|_| bad_char("QZLSCHSI", 1, a))?;
            if !h.autl_exists(&n) {
                return Err(ApiError::new(msg::NO_AUTL).text(&n, 10));
            }
            Some(n)
        }
        None => None,
    };
    s.code_page = r.code_page;
    s.idle_timeout = r.idle_timeout;
    s.oplock_timeout = r.oplock_timeout;
    s.browse_interval = r.browse_interval;
    s.message_auth = r.message_auth;
    if let Some(e) = r.encryption {
        s.encryption = e;
    }
    if let Some(a) = autl {
        s.server_autl = a;
    }
    Ok(())
}

/// `QZLSCHSI(request, length, format)`.
pub fn change_info(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSCHSI";
    let format = p.text(2);
    let len = p.bin4(1);
    let n = usize::try_from(len).unwrap_or(0);
    let length_err = |min: usize, max: usize| {
        ApiError::of(msg::LENGTH, API)
            .bin4(2)
            .bin4(len)
            .bin4(i32::try_from(min).unwrap_or(0) - 1)
            .bin4(i32::try_from(max).unwrap_or(i32::MAX).saturating_add(1))
    };
    match format.as_str() {
        "ZLSS0100" => {
            need_iosyscfg(API, h)?;
            if !(S0100_MIN..=S0100_SIZE).contains(&n) {
                return Err(length_err(S0100_MIN, S0100_SIZE));
            }
            let b = p.raw(0, n);
            let r = zlss::s0100(&b);
            let mut c = h.load()?;
            let mut pending = c.attrs.pending.clone();
            apply_0100(h, &mut pending, &r)?;
            c.attrs.pending = pending;
            h.save(&c)?;
            Ok(Vec::new())
        }
        "ZLSS0200" => {
            let b = p.raw(0, n);
            let (count, names) = zlss::s0200(&b);
            let wanted = usize::try_from(count).unwrap_or(0);
            if count <= 0 || n < 4 + 10 * wanted {
                return Err(length_err(14, 0x7FFF_FFFE));
            }
            let mut failed = 0;
            for name in &names {
                match h.enable_user(name) {
                    Enabled::Done => {}
                    Enabled::NotAuthorized | Enabled::Failed(_) => failed += 1,
                }
            }
            if failed > 0 {
                return Err(ApiError::new(msg::NOT_ENABLED).bin4(failed));
            }
            Ok(Vec::new())
        }
        "QSLS0100" => {
            need_iosyscfg(API, h)?;
            if n != qsl::S0100_SIZE {
                return Err(length_err(qsl::S0100_SIZE, qsl::S0100_SIZE));
            }
            let (netbios, dialect, log) = qsl::read_s0100(&p.raw(0, n));
            let flag = |c: char| match c {
                '0' => Ok(false),
                '1' => Ok(true),
                c => Err(bad_char(API, 1, &c.to_string())),
            };
            let own = qsl::Own {
                allow_netbios: flag(netbios)?,
                min_dialect: Dialect::parse(&dialect).ok_or_else(|| bad_char(API, 1, &dialect))?,
                log_signon: flag(log)?,
            };
            let mut c = h.load()?;
            own.apply(&mut c.attrs.pending);
            h.save(&c)?;
            Ok(Vec::new())
        }
        _ => Err(ApiError::of(msg::CHANGE, API).bin4(1).text(&format, 8)),
    }
}

/// `QZLSSTRS(reset 1)`: `1` makes the pending attributes current.
pub fn start(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSSTRS";
    need_iosyscfg(API, h)?;
    let reset = match p.ch(0) {
        Some('0') => false,
        Some('1') => true,
        other => {
            return Err(ApiError::of(msg::START, API)
                .bin4(start::RESET_VALUE)
                .bin4(0)
                .text(&other.unwrap_or(' ').to_string(), 1));
        }
    };
    if h.server().active() {
        return Err(ApiError::of(msg::START, API)
            .bin4(start::ACTIVE)
            .bin4(0)
            .text(" ", 1));
    }
    h.server().start(reset)?;
    if reset {
        let mut c = h.load()?;
        if c.attrs.apply_pending() {
            h.save(&c)?;
        }
    }
    let now = h.now();
    h.sessions().started(now);
    Ok(Vec::new())
}

/// `QZLSENDS()`.
pub fn end(h: &mut dyn Host, _p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    need_iosyscfg("QZLSENDS", h)?;
    h.server().end()?;
    Ok(Vec::new())
}

/// `QZLSENSS(workstation 15 [, session identifier 8])`.
pub fn end_sessions(h: &mut dyn Host, p: &Parms<'_>) -> Result<Vec<u8>, ApiError> {
    const API: &str = "QZLSENSS";
    need_iosyscfg(API, h)?;
    let ws = p.text(0);
    let id = p.bin8(1);
    let err = |r: i32| {
        ApiError::of(msg::END_SESSION, API)
            .text(&ws, 15)
            .bin4(r)
            .bin8(id.unwrap_or(0))
    };
    if !h.server().active() {
        return Err(err(end_session::NOT_ACTIVE));
    }
    let ids: Vec<u64> = match (ws.as_str(), id) {
        ("*SESSID", Some(i)) => {
            let i = u64::try_from(i).unwrap_or(0);
            if h.sessions().get(i).is_none() {
                return Err(err(end_session::NO_SESSION));
            }
            vec![i]
        }
        ("*SESSID", None) => return Err(err(end_session::NO_SESSION)),
        ("*EXPANDED", _) => return Err(err(end_session::EXPANDED)),
        (_, Some(_)) => return Err(err(end_session::ID_WITHOUT_SESSID)),
        (w, None) => {
            let v = h.sessions().of_workstation(w);
            if v.is_empty() {
                return Err(err(end_session::NO_WORKSTATION));
            }
            v
        }
    };
    for i in ids {
        let job = h
            .sessions()
            .end(i)
            .map_err(|_| err(end_session::NO_SESSION))?;
        h.server().end_session(&job, i)?;
    }
    Ok(Vec::new())
}
