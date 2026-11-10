// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NetServer` sign-on in the storage service (`docs/NETSERVER.md`
//! section 5): a `QZLSFILE` job's `NTLMv2` response checked against the
//! sealed NT hash (`sec::netsvr`), which never leaves the service; the
//! users disabled for `NetServer`, `CPIB682` to `QSYSOPR` when one is;
//! the sealer over the LIC stash key that every password set uses.

use std::sync::Arc;

use certs::stash::{KeySealer, Sealer};
use sec::netsvr::{LoggedOn, Logon, Refusal};
use sec::password::nthash::NtSealer;
use sec::signon::Values;

use super::security::{require, sysval_text};
use super::served::JobCtx;
use crate::ipl::system::{System, now_unix};

/// The connection jobs' name.
const FILE_JOB: &str = "QZLSFILE";
/// The server job's name.
const SERVER_JOB: &str = "QZLSSERVER";

/// The LIC stash key as the NT hash sealer.
struct LicSealer(KeySealer);

impl NtSealer for LicSealer {
    fn generation(&self) -> u32 {
        self.0.generation()
    }

    fn seal(&self, aad: &[u8], plain: &[u8]) -> Option<Vec<u8>> {
        self.0.seal(aad, plain).ok()
    }

    fn unseal(&self, aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
        self.0.unseal(aad, sealed).ok()
    }
}

/// Install the sealer from the LIC partition's stash key slot (made at
/// IPL); with none, passwords set get no NT hash.
pub(crate) fn install_sealer(system: &mut System) {
    let s = super::certs::sealer(system).map(|k| Arc::new(LicSealer(k)) as Arc<dyn NtSealer>);
    sec::password::nthash::install(s);
}

/// `netsvr_signon`: only a `QZLSFILE` job asks. With `swap` the job then
/// runs as the user (a guest too).
pub fn signon(
    system: &mut System,
    ctx: &mut JobCtx,
    logon: &Logon,
    swap: bool,
) -> Result<LoggedOn, Refusal> {
    if ctx.job.name != FILE_JOB {
        return Err(Refusal::NoSuchUser);
    }
    let values = Values::from_sysvals(&|n: &str| sysval_text(system, n));
    let now = now_unix();
    let sealer = sec::password::nthash::sealer();
    let r = sec::guard::as_system(|| {
        let s = system.storage.as_mut()?;
        let qsys = s.qsys?;
        let mut txn = s.store.begin().ok()?;
        sec::netsvr::sign_on(
            &mut txn,
            qsys,
            logon,
            &values,
            sec::netsvr::users(),
            sealer.as_deref(),
            now,
        )
        .ok()
    });
    // A store that cannot be read signs nobody on.
    let r = r.unwrap_or(Err(Refusal::NoSuchUser));
    match &r {
        Ok(ok) if swap => super::hostsvr::become_user(system, ctx, &ok.user),
        Err(Refusal::WrongPassword { disabled_now: true }) => {
            disabled_message(system, ctx, &logon.user, &logon.client);
        }
        _ => {}
    }
    r
}

/// `CPIB682` to `QSYSOPR`: `user` disabled for `NetServer`. Its data is
/// the profile name (`CHAR(10)`) and the client (`CHAR(46)`).
fn disabled_message(system: &mut System, ctx: &JobCtx, user: &str, client: &str) {
    let name = user.trim().to_ascii_uppercase();
    let pad = |s: &str, n: usize| {
        let mut b: Vec<u8> = ccsid::to_ebcdic(s).unwrap_or_default();
        b.resize(n, 0x40);
        b
    };
    let mut data = pad(&name, 10);
    data.extend(pad(client, 46));
    let m = wm::obj::msgq::QMsg {
        msgid: sec::netsvr::DISABLED_MSGID.into(),
        msgf: ("QCPFMSG".into(), "QSYS".into()),
        severity: 0,
        data,
        text: super::msgs::text(system, sec::netsvr::DISABLED_MSGID, &[&name, client]),
        job: (ctx.job.name.clone(), ctx.job.user.clone(), ctx.job.number),
        from_pgm: "QZLSFILE".into(),
        ..wm::obj::msgq::QMsg::default()
    };
    let _ = super::msgs::send(system, "QSYS", "QSYSOPR", m);
}

/// `netsvr_started`: the server job says `NetServer` started; every
/// user is enabled and `QMAXSIGN` is taken now.
pub fn started(system: &System, ctx: &JobCtx) {
    if ctx.job.name != SERVER_JOB {
        return;
    }
    let max = sysval_text(system, "QMAXSIGN").trim().parse().ok();
    sec::netsvr::users().started(max);
}

/// `netsvr_enable_user` (`QZLSCHSI` reset of a disabled user):
/// `*IOSYSCFG`. Whether the user was disabled.
pub fn enable_user(system: &mut System, user: &str) -> Result<bool, String> {
    let what = super::msgs::text(system, "TXT0E2C", &[]);
    require(sec::profile::SpecialAuth::IOSYSCFG, &what)?;
    Ok(sec::netsvr::enable_netserver_user(user))
}

/// `netsvr_disabled_users`: the users disabled for `NetServer` and since
/// when (Unix seconds).
pub fn disabled_users() -> Vec<(String, u64)> {
    sec::netsvr::users().disabled()
}
