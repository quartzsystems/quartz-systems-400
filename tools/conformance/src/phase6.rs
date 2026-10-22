// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 6 steps of the disk suite: security. Profiles and authorities
//! from the service console (which runs as `QSYS`), then the same
//! objects as a plain user signed on at the console, adopted authority,
//! domain and state, auditing and its end action, deleting a profile,
//! the security levels and the numbers.

use std::time::{Duration, Instant};

use crate::phase5::{console_sign_on_as, screen_any, screen_wait};
use crate::qemu::session::Session;
use crate::steps::Suite;

const CMD: Duration = Duration::from_secs(10);
/// The password the users of these steps get.
const PASSWORD: &str = "LETMEIN1";

/// A program that reads and writes `QGPL/SECRET`; built once adopting
/// its owner and once not.
fn secret_writer(name: &str) -> String {
    format!(
        "; writes QGPL/SECRET: with its owner's authority when it adopts
module {name}
static $v: char(10)
proc {name}() entry {{
L0:
  monitor M, CPF2189 -> REFUSED
  dtaara.read \"SECRET\", \"QGPL\", $v
  mov $v, \"adopted\"
  dtaara.write \"SECRET\", \"QGPL\", $v
  dsply \"secret written\"
  endmon M
  ret
REFUSED:
  dsply \"secret refused\"
  ret
}}
"
    )
}

/// A user-state program importing a privileged procedure.
const RAW: &str = "; imports a privileged procedure: refused at load in user state
module RAW
extern proc _QSYSRAW($a: i32)
static $a: i32 inz(1)
proc RAW() entry {
L0:
  callb _QSYSRAW($a)
  ret
}
";

/// A program meant to be bound in system state.
const SYSPGM: &str = "; bound STATE(*SYSTEM) by the harness
module SYSPGM
proc SYSPGM() entry {
L0:
  dsply \"system state\"
  ret
}
";

/// A user-state program calling the system-state one.
const USRCALL: &str = "; a user-state call into system state
module USRCALL
proc USRCALL() entry {
L0:
  call SYSPGM()
  dsply \"not reached\"
  ret
}
";

fn want(out: &str, needle: &str, what: &str) -> Result<(), String> {
    if out.contains(needle) {
        Ok(())
    } else {
        Err(format!("{what}: no {needle:?} in:\n{out}"))
    }
}

fn refuse(out: &str, needle: &str, what: &str) -> Result<(), String> {
    if out.contains(needle) {
        Err(format!("{what}: {needle:?} is still in:\n{out}"))
    } else {
        Ok(())
    }
}

/// Type a line at Command Entry and press Enter.
fn entry(s: &mut Session, line: &str) -> Result<(), String> {
    s.command(&format!("type {line}"), CMD)?;
    s.command("key enter", CMD)?;
    Ok(())
}

/// Type a line at Command Entry; the screen once one of its lines
/// satisfies `done`.
fn entry_wait(s: &mut Session, line: &str, done: impl Fn(&str) -> bool) -> Result<String, String> {
    entry(s, line)?;
    let mut last = String::new();
    for _ in 0..80 {
        last = s.command("screen", CMD)?;
        if last.lines().any(&done) {
            return Ok(last);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "after {line:?} the console never showed what was expected:\n{last}"
    ))
}

/// A line holding every word.
fn has(words: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |l| words.iter().all(|w| l.contains(w))
}

/// Sign on at the console as `user` and reach Command Entry.
fn sign_on(s: &mut Session, user: &str) -> Result<(), String> {
    screen_wait(s, "Sign On")?;
    let screen = console_sign_on_as(s, user, PASSWORD, PASSWORD)?;
    if screen.contains("Main Menu") {
        entry(s, "call QCMD")?;
    }
    screen_wait(s, "Command Entry").map(|_| ())
}

fn sign_off(s: &mut Session) -> Result<(), String> {
    entry(s, "signoff")?;
    screen_wait(s, "Sign On").map(|_| ())
}

/// Assemble and bind a program into `QGPL`, with more `CRTPGM` keywords.
fn build(s: &mut Session, name: &str, text: &str, extra: &str) -> Result<(), String> {
    crate::qir::qemu::push(s, name, text)?;
    let m = s.command(&format!("qir asm {name} MODULE(QGPL/{name})"), CMD)?;
    let p = s.command(
        &format!("crtpgm QGPL/{name} MODULE(QGPL/{name}) {extra}"),
        CMD,
    )?;
    if !m.contains("created") || !p.contains("created") {
        return Err(format!("{name}:\n{m}\n{p}"));
    }
    Ok(())
}

impl Suite {
    /// `pwrdwn restart` from the service console; the time of the IPL.
    fn reipl(&self, s: &mut Session) -> Result<Duration, String> {
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001000", CMD)?;
        s.expect("SRC D1001090", Duration::from_secs(60))?;
        let t0 = s.elapsed();
        s.expect_in_order(crate::steps::SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        Ok(s.elapsed().saturating_sub(t0))
    }

    /// Step 37: the audit journal and auditing on; three profiles, one a
    /// group; a data area `*PUBLIC *EXCLUDE` with a private `*USE`; an
    /// authorization list securing another; the displays show it all and
    /// the profile changes are audited.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn user_profiles(&self, s: &mut Session) -> Result<String, String> {
        let me = s.command("whoami", CMD)?;
        want(&me, "QSYS", "whoami")?;
        want(&me, "*ALLOBJ", "whoami")?;
        for cmd in [
            "crtjrnrcv QSYS/AUDRCV0 THRESHOLD(64)",
            "crtjrn QSYS/QAUDJRN JRNRCV(QSYS/AUDRCV0) MNGRCV(*USER)",
            "chgsecaud QAUDCTL(*AUDLVL *OBJAUD) QAUDLVL(*DFTSET)",
        ] {
            let out = s.command(cmd, CMD)?;
            if out.contains("CPF") || out.contains("CPD") {
                return Err(format!("{cmd}:\n{out}"));
            }
        }
        let aud = s.command("dspsecaud", CMD)?;
        want(&aud, "*AUDLVL", "dspsecaud")?;
        for cmd in [
            "crtusrprf USRPRF(STAFF) PASSWORD(*NONE) TEXT('Staff group')",
            &format!("crtusrprf USRPRF(ANNE) PASSWORD({PASSWORD}) GRPPRF(STAFF) TEXT('Anne')"),
            &format!("crtusrprf USRPRF(BOB) PASSWORD({PASSWORD}) TEXT('Bob')"),
            "obj crt *DTAARA QGPL/SECRET payroll figures",
            "dtaara set QGPL/SECRET 42",
            "grtobjaut OBJ(QGPL/SECRET) OBJTYPE(*DTAARA) USER(*PUBLIC) AUT(*EXCLUDE)",
            "grtobjaut OBJ(QGPL/SECRET) OBJTYPE(*DTAARA) USER(ANNE) AUT(*USE)",
            "crtautl AUTL(PAYROLL) TEXT('Payroll')",
            "addautle AUTL(PAYROLL) USER(BOB) AUT(*CHANGE)",
            "obj crt *DTAARA QGPL/LEDGER ledger",
            "dtaara set QGPL/LEDGER ledger77",
            "grtobjaut OBJ(QGPL/LEDGER) OBJTYPE(*DTAARA) AUTL(PAYROLL)",
            "grtobjaut OBJ(QGPL/LEDGER) OBJTYPE(*DTAARA) USER(*PUBLIC) AUT(*AUTL)",
        ] {
            let out = s.command(cmd, CMD)?;
            if out.contains("CPF") || out.contains("CPD") {
                return Err(format!("{cmd}:\n{out}"));
            }
        }
        let anne = s.command("dspusrprf USRPRF(ANNE)", CMD)?;
        want(&anne, "STAFF", "dspusrprf ANNE")?;
        let users = s.command("dspautusr", CMD)?;
        want(&users, "ANNE", "dspautusr")?;
        want(&users, "BOB", "dspautusr")?;
        let aut = s.command("dspobjaut OBJ(QGPL/SECRET) OBJTYPE(*DTAARA)", CMD)?;
        let anne_row = aut
            .lines()
            .find(|l| l.trim_start().starts_with("ANNE"))
            .ok_or_else(|| format!("dspobjaut: no ANNE row:\n{aut}"))?;
        want(anne_row, "*USE", "dspobjaut ANNE")?;
        let public = aut
            .lines()
            .find(|l| l.contains("*PUBLIC"))
            .ok_or_else(|| format!("dspobjaut: no *PUBLIC row:\n{aut}"))?;
        want(public, "*EXCLUDE", "dspobjaut *PUBLIC")?;
        let list = s.command("dspautlobj AUTL(PAYROLL)", CMD)?;
        want(&list, "LEDGER", "dspautlobj")?;
        let cp = s.command("dspaudjrne ENTTYP(CP)", CMD)?;
        want(&cp, "ANNE", "dspaudjrne CP")?;
        Ok("QAUDJRN on; STAFF, ANNE (group STAFF), BOB; SECRET *PUBLIC *EXCLUDE with ANNE *USE; LEDGER secured by PAYROLL; CP entries audited".into())
    }

    /// Step 38: a wrong password is refused at the sign-on display; ANNE
    /// signs on, sees who she is, reads the data area she may, is refused
    /// what she may not (`CPF2189`, `CPF9802`, `CPF2222`), creates an
    /// object of her own; BOB reads through his list entry; the refusals
    /// are `AF` entries.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn user_sign_on(&self, s: &mut Session) -> Result<String, String> {
        screen_wait(s, "Sign On")?;
        s.command("type ANNE", CMD)?;
        s.command("key tab", CMD)?;
        s.command("type WRONGPW", CMD)?;
        s.command("key enter", CMD)?;
        screen_any(s, &["not correct"])?;
        sign_on(s, "ANNE")?;
        let me = entry_wait(s, "whoami", has(&["User", "ANNE"]))?;
        want(&me, "STAFF", "whoami as ANNE")?;
        entry_wait(s, "dtaara get QGPL/SECRET", |l| {
            l.contains("42") && !l.contains("dtaara")
        })?;
        entry_wait(s, "dtaara set QGPL/SECRET 43", has(&["CPF2189"]))?;
        entry_wait(s, "dtaara get QGPL/LEDGER", has(&["CPF2189", "LEDGER"]))?;
        entry_wait(
            s,
            "chkobj OBJ(QGPL/SECRET) OBJTYPE(*DTAARA) AUT(*CHANGE)",
            has(&["CPF9802"]),
        )?;
        entry_wait(s, "crtusrprf USRPRF(EVE)", has(&["CPF2222"]))?;
        entry_wait(s, "obj crt *DTAARA QGPL/ANNEDTA mine", |l| {
            l.contains("ANNEDTA") && !l.contains("obj crt")
        })?;
        sign_off(s)?;
        sign_on(s, "BOB")?;
        entry_wait(s, "dtaara get QGPL/LEDGER", has(&["ledger77"]))?;
        sign_off(s)?;
        let af = s.command("dspaudjrne ENTTYP(AF) USRPRF(ANNE)", CMD)?;
        want(&af, "SECRET", "dspaudjrne AF")?;
        want(&af, "LEDGER", "dspaudjrne AF")?;
        let owner = s.command("dspobjaut OBJ(QGPL/ANNEDTA) OBJTYPE(*DTAARA)", CMD)?;
        want(&owner, "ANNE", "ANNEDTA's owner")?;
        Ok("wrong password refused; ANNE read SECRET, was refused its update, LEDGER, CHKOBJ *CHANGE and CRTUSRPRF, created ANNEDTA; BOB read LEDGER through PAYROLL; AF entries for ANNE".into())
    }

    /// Step 39: a program owned by `QSYS` with `USRPRF(*OWNER)` writes
    /// the data area for ANNE; the same program with `USRPRF(*USER)` is
    /// refused.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn adopted_authority(&self, s: &mut Session) -> Result<String, String> {
        build(s, "ADOPT", &secret_writer("ADOPT"), "USRPRF(*OWNER)")?;
        build(s, "NOADOPT", &secret_writer("NOADOPT"), "USRPRF(*USER)")?;
        let shown = s.command("dsppgm QGPL/ADOPT", CMD)?;
        want(&shown, "*OWNER", "dsppgm ADOPT")?;
        sign_on(s, "ANNE")?;
        entry_wait(s, "call QGPL/NOADOPT", has(&["secret refused"]))?;
        entry_wait(s, "call QGPL/ADOPT", has(&["secret written"]))?;
        sign_off(s)?;
        let v = s.command("dtaara get QGPL/SECRET", CMD)?;
        want(&v, "adopted", "SECRET after ADOPT")?;
        Ok("NOADOPT refused for ANNE, ADOPT (USRPRF(*OWNER), owner QSYS) wrote SECRET".into())
    }

    /// Step 40: at `QSECURITY` 40 a user-state program importing a
    /// privileged procedure fails to load (`MCH6801`), and a user-state
    /// program calling a system-state one is refused the same way.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn domain_state(&self, s: &mut Session) -> Result<String, String> {
        build(s, "RAW", RAW, "")?;
        build(s, "SYSPGM", SYSPGM, "STATE(*SYSTEM)")?;
        build(s, "USRCALL", USRCALL, "")?;
        let raw = s.command("call QGPL/RAW", CMD)?;
        want(&raw, "MCH6801", "call RAW")?;
        let up = s.command("call QGPL/USRCALL", CMD)?;
        want(&up, "MCH6801", "call USRCALL")?;
        refuse(&up, "not reached", "call USRCALL")?;
        Ok(
            "RAW (imports _QSYSRAW) and USRCALL (calls a STATE(*SYSTEM) program) both MCH6801"
                .into(),
        )
    }

    /// Step 41: object auditing records a read (`ZR`); a create and a
    /// delete are `CO` and `DO`; `CPYAUDJRNE` writes an output file and
    /// `DSPAUDJRNE OUTPUT(*PRINT)` a spooled file.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn auditing(&self, s: &mut Session) -> Result<String, String> {
        for cmd in [
            "chgobjaud OBJ(QGPL/SECRET) OBJTYPE(*DTAARA) OBJAUD(*ALL)",
            "dtaara get QGPL/SECRET",
            "obj crt *DTAARA QGPL/TEMPA short lived",
            "obj dlt *DTAARA QGPL/TEMPA",
        ] {
            let out = s.command(cmd, CMD)?;
            if out.contains("CPF") || out.contains("CPD") {
                return Err(format!("{cmd}:\n{out}"));
            }
        }
        let zr = s.command("dspaudjrne ENTTYP(ZR)", CMD)?;
        want(&zr, "SECRET", "dspaudjrne ZR")?;
        let co = s.command("dspaudjrne ENTTYP(CO)", CMD)?;
        want(&co, "TEMPA", "dspaudjrne CO")?;
        let d = s.command("dspaudjrne ENTTYP(DO)", CMD)?;
        want(&d, "TEMPA", "dspaudjrne DO")?;
        let cpy = s.command("cpyaudjrne ENTTYP(AF) OUTFILE(QGPL/QAUDIT)", CMD)?;
        want(&cpy, "QAUDITAF", "cpyaudjrne")?;
        let pfm = s.command("dsppfm QGPL/QAUDITAF", CMD)?;
        want(&pfm, "ANNE", "dsppfm QAUDITAF")?;
        let printed = s.command("dspaudjrne ENTTYP(AF) OUTPUT(*PRINT)", CMD)?;
        want(&printed, "QPAUDENT", "dspaudjrne *PRINT")?;
        let spooled = s.command("wrksplf", CMD)?;
        want(&spooled, "QPAUDENT", "wrksplf")?;
        Ok("ZR for SECRET, CO and DO for TEMPA; QAUDITAF written; QPAUDENT spooled".into())
    }

    /// Step 42: a full receiver with `QAUDENDACN(*NOTIFY)` tells
    /// `QSYSOPR` (`CPI2283`); with `*PWRDWNSYS` the system powers down and
    /// the next IPL starts with `QAUDCTL(*NONE)` and `CPI2284` in `QHST`.
    pub(crate) fn audit_end_action(&self, s: &mut Session) -> Result<String, String> {
        for cmd in [
            "crtjrnrcv QSYS/AUDRCV1 THRESHOLD(1)",
            "chgjrn QSYS/QAUDJRN JRNRCV(QSYS/AUDRCV1)",
        ] {
            let out = s.command(cmd, CMD)?;
            if out.contains("CPF") || out.contains("CPD") {
                return Err(format!("{cmd}:\n{out}"));
            }
        }
        for i in 0..12 {
            s.command(&format!("obj crt *DTAARA QGPL/TEMPB{i} filler"), CMD)?;
            s.command(&format!("obj dlt *DTAARA QGPL/TEMPB{i}"), CMD)?;
        }
        let mut opr = String::new();
        for _ in 0..40 {
            opr = s.command("dspmsg QSYSOPR", CMD)?;
            if opr.contains("CPI2283") {
                break;
            }
            s.command("obj crt *DTAARA QGPL/TEMPC filler", CMD)?;
            s.command("obj dlt *DTAARA QGPL/TEMPC", CMD)?;
            std::thread::sleep(Duration::from_millis(250));
        }
        want(&opr, "CPI2283", "QSYSOPR after the receiver filled")?;
        let set = s.command("sysval QAUDENDACN *PWRDWNSYS", CMD)?;
        want(&set, "set to", "sysval QAUDENDACN")?;
        s.send("obj crt *DTAARA QGPL/TEMPD the last straw")?;
        s.expect("SRC D1001090", Duration::from_secs(60))?;
        if !s.wait_exit(Duration::from_secs(30)) {
            return Err("the system did not power off".into());
        }
        *s = Session::spawn(&self.launch).map_err(|e| e.to_string())?;
        let t0 = s.elapsed();
        s.expect_in_order(crate::steps::SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let boot = s.elapsed().saturating_sub(t0);
        let ctl = s.command("sysval QAUDCTL", CMD)?;
        want(&ctl, "*NONE", "QAUDCTL after the forced power down")?;
        let hist = s.command("dsplog MSGID(CPI2284)", CMD)?;
        want(&hist, "CPI2284", "QHST")?;
        // Auditing back on, with room.
        for cmd in [
            "crtjrnrcv QSYS/AUDRCV2 THRESHOLD(0)",
            "chgjrn QSYS/QAUDJRN JRNRCV(QSYS/AUDRCV2)",
            "chgsecaud QAUDCTL(*AUDLVL *OBJAUD)",
            "sysval QAUDENDACN *NOTIFY",
        ] {
            let out = s.command(cmd, CMD)?;
            if out.contains("CPF") || out.contains("CPD") {
                return Err(format!("{cmd}:\n{out}"));
            }
        }
        Ok(format!(
            "CPI2283 at QSYSOPR with *NOTIFY; *PWRDWNSYS powered the system down, IPL in {:.1}s with QAUDCTL *NONE and CPI2284 in QHST",
            boot.as_secs_f64()
        ))
    }

    /// Step 43: ANNE owns an object, so `DLTUSRPRF` stops unless told
    /// what to do with it; `*CHGOWN` moves it; her private authorities go
    /// with her; the group follows once it has no members; the indexes
    /// verify.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn delete_profile(&self, s: &mut Session) -> Result<String, String> {
        let stop = s.command("dltusrprf USRPRF(ANNE)", CMD)?;
        want(&stop, "CPF2204", "DLTUSRPRF of an owner")?;
        let gone = s.command("dltusrprf USRPRF(ANNE) OWNOBJOPT(*CHGOWN BOB)", CMD)?;
        want(&gone, "deleted", "DLTUSRPRF *CHGOWN")?;
        let users = s.command("dspautusr", CMD)?;
        refuse(&users, " ANNE ", "dspautusr")?;
        let owner = s.command("dspobjaut OBJ(QGPL/ANNEDTA) OBJTYPE(*DTAARA)", CMD)?;
        want(&owner, "BOB", "ANNEDTA's new owner")?;
        let secret = s.command("dspobjaut OBJ(QGPL/SECRET) OBJTYPE(*DTAARA)", CMD)?;
        refuse(&secret, "ANNE", "SECRET's authorities")?;
        let group = s.command("dltusrprf USRPRF(STAFF)", CMD)?;
        want(&group, "deleted", "DLTUSRPRF of the empty group")?;
        let check = s.command("rclstg *CHECK", CMD)?;
        want(&check, "profile indexes: no problems", "rclstg *CHECK")?;
        Ok("ANNE kept by CPF2204, deleted with *CHGOWN BOB; her private authority gone; STAFF deleted; indexes verify".into())
    }

    /// Step 44: `QSECURITY` 30: sign-on and adoption work, a privileged
    /// import loads and fails at the call (`MCH3401`); 50: the load is
    /// refused (`MCH6801`); then 40 for the rest.
    pub(crate) fn security_levels(&self, s: &mut Session) -> Result<String, String> {
        let set = s.command("sysval QSECURITY 30", CMD)?;
        want(&set, "set to 30", "sysval QSECURITY 30")?;
        let b30 = self.reipl(s)?;
        let me = s.command("whoami", CMD)?;
        want(&me, "30", "whoami at 30")?;
        sign_on(s, "BOB")?;
        entry_wait(s, "call QGPL/NOADOPT", has(&["secret refused"]))?;
        entry_wait(s, "call QGPL/ADOPT", has(&["secret written"]))?;
        sign_off(s)?;
        let raw = s.command("call QGPL/RAW", CMD)?;
        want(&raw, "MCH3401", "call RAW at 30")?;
        refuse(&raw, "MCH6801", "call RAW at 30")?;
        let set = s.command("sysval QSECURITY 50", CMD)?;
        want(&set, "set to 50", "sysval QSECURITY 50")?;
        let b50 = self.reipl(s)?;
        let raw = s.command("call QGPL/RAW", CMD)?;
        want(&raw, "MCH6801", "call RAW at 50")?;
        let set = s.command("sysval QSECURITY 40", CMD)?;
        want(&set, "set to 40", "sysval QSECURITY 40")?;
        Ok(format!(
            "30 (IPL {:.1}s): BOB signed on, NOADOPT refused, ADOPT wrote, RAW MCH3401; 50 (IPL {:.1}s): RAW MCH6801; back to 40",
            b30.as_secs_f64(),
            b50.as_secs_f64()
        ))
    }

    /// Step 45: the security numbers of `PHASE6.md`: sign-on from Enter
    /// to the first menu, and an authority check's console round trip
    /// cold and cached.
    #[allow(clippy::unused_self)] // every step has the same shape
    pub(crate) fn security_bench(&self, s: &mut Session) -> Result<String, String> {
        screen_wait(s, "Sign On")?;
        s.command("type BOB", CMD)?;
        s.command("key tab", CMD)?;
        s.command(&format!("type {PASSWORD}"), CMD)?;
        let t0 = Instant::now();
        s.command("key enter", CMD)?;
        let mut shown = false;
        for _ in 0..400 {
            let screen = s.command("screen", CMD)?;
            if screen.contains("Main Menu") || screen.contains("Command Entry") {
                shown = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let sign_on_time = t0.elapsed();
        if !shown {
            return Err("BOB never reached a menu".into());
        }
        let screen = s.command("screen", CMD)?;
        if screen.contains("Main Menu") {
            entry(s, "call QCMD")?;
        }
        screen_wait(s, "Command Entry")?;
        let check = "chkobj OBJ(QGPL/LEDGER) OBJTYPE(*DTAARA) AUT(*USE)";
        let t1 = Instant::now();
        entry_wait(s, check, has(&["LEDGER", "exists"]))?;
        let cold = t1.elapsed();
        entry(s, "wrkactjob")?;
        screen_wait(s, "QSYSARB")?;
        let t2 = Instant::now();
        entry_wait(s, check, has(&["LEDGER", "exists"]))?;
        let cached = t2.elapsed();
        sign_off(s)?;
        Ok(format!(
            "sign-on to the first menu {:.0} ms; CHKOBJ at Command Entry {:.0} ms cold, {:.0} ms cached (screen polling included)",
            sign_on_time.as_secs_f64() * 1000.0,
            cold.as_secs_f64() * 1000.0,
            cached.as_secs_f64() * 1000.0
        ))
    }
}
