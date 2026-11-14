// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 186 to 194 on the main guest. Each sets up what it needs when
//! an earlier one did not (`--only` runs any of them alone): the
//! administrator's password, the instance, and forwards of 389 and 636.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ldap::msg::Scope;

use super::tool::{self, Outcome};
use super::{
    ADMIN_DN, ADMIN_PW, CMD, INSTANCE, SLOW, SUFFIX, add, admin, bind_rc, bound, clean, delete,
    dump, end, errors, exists, full_log, job_line, qsh, replace, root_dse, search, start,
    wait_ldap, want,
};
use crate::qemu::session::Session;
use crate::steps::Suite;
use crate::tcp::{GUEST, forward_port};

/// How long a run of `ldaptest` may take.
const RUN_LIMIT: Duration = Duration::from_mins(30);
/// Scenarios that fail against the server on main, each a finding in
/// `docs/DIRSRV.md` §16: reported, not failing the step.
const KNOWN: &[&str] = &[];

/// The profiles step 188 makes: name, password, extra `CRTUSRPRF` words.
const PROFILES: &[(&str, &str, &str)] = &[
    ("LDUSER", "Ldap2026u", ""),
    ("LDDIS", "Ldap2026d", " STATUS(*DISABLED)"),
    ("LDBAD", "Ldap2026b", ""),
    ("LDADM", "Ldap2026a", ""),
];

/// A profile's password from [`PROFILES`].
fn pw_of(name: &str) -> &'static str {
    PROFILES
        .iter()
        .find(|(n, _, _)| *n == name)
        .map_or("", |(_, p, _)| p)
}

/// The step 189 certificates' labels and the client's subject.
const SERVER_LABEL: &str = "DIRSRV189";
const CLIENT_LABEL: &str = "LDAPCLI189";
/// The suffix step 189 adds for the client certificate's entry.
const EXT_SUFFIX: &str = "o=qs400test";
const EXT_DN: &str = "cn=ldext,o=qs400test";

/// The profile step 191 publishes.
const PUB_USER: &str = "PUBUSER";
/// Where the users agent publishes.
const PUB_PARENT: &str = "ou=Users,cn=localhost";
/// The parent's ACL: anyone adds, deletes and writes below it.
const PUB_ACL: &str = "group:CN=ANYBODY:object:ad:normal:rwsc:sensitive:rwsc:critical:rwsc:system:rsc:restricted:rsc";
/// The publishing configuration.
const PUB_CONF: &str = "/QIBM/UserData/OS400/DirSrv/publish.conf";

/// Failures not in [`KNOWN`].
fn new_failures(o: &Outcome) -> Vec<&str> {
    o.failed
        .iter()
        .filter(|l| !KNOWN.contains(&Outcome::name(l)))
        .map(String::as_str)
        .collect()
}

/// `os400-profile=NAME,cn=accounts,` and the projected root.
fn profile_dn(root: &str, name: &str) -> String {
    format!("os400-profile={name},cn=accounts,{root}")
}

/// The PEM blocks of `text` with the label `kind` (`CERTIFICATE`,
/// `PRIVATE KEY` ...), each with its armour.
fn pem_blocks(text: &str, kind: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with("-----BEGIN ") && l.contains(kind) {
            cur = Some(format!("{l}\n"));
        } else if let Some(c) = cur.as_mut() {
            c.push_str(l);
            c.push('\n');
            if l.starts_with("-----END ") {
                out.extend(cur.take());
            }
        }
    }
    out
}

#[allow(
    clippy::unused_self,
    clippy::many_single_char_names,
    clippy::too_many_lines
)] // the steps share one signature; one check after another
impl Suite {
    /// The administrator's password, the instance and the forwards,
    /// unless an earlier step made them: the host ports of 389 and 636.
    fn dir_ensure(&self, s: &mut Session) -> Result<(u16, u16), String> {
        {
            let st = self.dir.borrow();
            if st.started
                && let Some(p) = st.ports
            {
                return Ok(p);
            }
        }
        full_log(s)?;
        if !self.dir.borrow().admin_set {
            want(
                s,
                &format!("CHGDIRSVRA DN(*ADMINDN '{ADMIN_PW}')"),
                &["GLD0123"],
                CMD,
            )?;
            self.dir.borrow_mut().admin_set = true;
        }
        start(s)?;
        let ports = self.dir_ports()?;
        wait_ldap(ports.0, Duration::from_secs(60))?;
        self.dir.borrow_mut().started = true;
        Ok(ports)
    }

    /// The forwards of 389 and 636, made once.
    fn dir_ports(&self) -> Result<(u16, u16), String> {
        if let Some(p) = self.dir.borrow().ports {
            return Ok(p);
        }
        let p = (
            forward_port(self, GUEST, 389)?,
            forward_port(self, GUEST, 636)?,
        );
        self.dir.borrow_mut().ports = Some(p);
        Ok(p)
    }

    /// The instance ended and started again (a configuration change).
    fn dir_restart(&self, s: &mut Session) -> Result<u16, String> {
        end(s)?;
        start(s)?;
        let p = self.dir_ports()?;
        wait_ldap(p.0, Duration::from_secs(60))?;
        Ok(p.0)
    }

    /// A step's result, with the guest's state written out when it failed.
    fn dir_checked(
        &self,
        s: &mut Session,
        step: &str,
        r: Result<String, String>,
    ) -> Result<String, String> {
        r.map_err(|e| {
            let path = dump(&self.out_dir, step, s, "");
            format!("{e}\n(guest state: {path})")
        })
    }

    /// Step 193: `QSSECCHK` on the shipped instance: anonymous binds, no
    /// TLS and no administrator password are the three `DIRSRV` warnings.
    pub(crate) fn dir_secchk(&self, s: &mut Session) -> Result<String, String> {
        if self.dir.borrow().admin_set {
            return Err("the administrator's password is set already: run 193 first".into());
        }
        let out = s.command("QSSECCHK OUTPUT(*)", Duration::from_secs(60))?;
        let lines: Vec<&str> = crate::secacg::status_lines(&out, "WARN")
            .into_iter()
            .filter(|l| l.trim_start().starts_with("DIRSRV"))
            .collect();
        if lines.len() != 3 {
            return Err(format!(
                "QSSECCHK: want 3 DIRSRV warnings for QUSRDIR, got {}:\n{out}",
                lines.len()
            ));
        }
        let fails: Vec<&str> = crate::secacg::status_lines(&out, "FAIL")
            .into_iter()
            .filter(|l| l.trim_start().starts_with("DIRSRV"))
            .collect();
        if !fails.is_empty() {
            return Err(format!("QSSECCHK: DIRSRV failures:\n{out}"));
        }
        Ok(format!(
            "3 DIRSRV warnings: {}",
            lines
                .iter()
                .map(|l| l.split_whitespace().skip(3).collect::<Vec<_>>().join(" "))
                .collect::<Vec<_>>()
                .join(" | ")
        ))
    }

    /// Step 186: the instance's job starts, listens and ends.
    pub(crate) fn dir_start(&self, s: &mut Session) -> Result<String, String> {
        let r = self.dir_start_body(s);
        self.dir_checked(s, "186", r)
    }

    fn dir_start_body(&self, s: &mut Session) -> Result<String, String> {
        full_log(s)?;
        let none = s.command("CHGDIRSVRA DN(*ADMINDN ' ')", CMD)?;
        if !none.contains("GLD0202") {
            return Err(format!("CHGDIRSVRA without a password: no GLD0202:\n{none}"));
        }
        // Started before the password is set: the administrator cannot
        // bind; CHGDIRSVRA's password takes effect at once.
        start(s)?;
        let (p, _) = self.dir_ports()?;
        wait_ldap(p, Duration::from_secs(60))?;
        let before = if self.dir.borrow().admin_set {
            None
        } else {
            Some(bind_rc(p, ADMIN_DN, ADMIN_PW)?)
        };
        if before.is_some_and(|rc| rc != 49) {
            return Err(format!(
                "the administrator bound with no password set: {before:?}"
            ));
        }
        want(
            s,
            &format!("CHGDIRSVRA DN(*ADMINDN '{ADMIN_PW}')"),
            &["GLD0123"],
            CMD,
        )?;
        self.dir.borrow_mut().admin_set = true;
        let after = bind_rc(p, ADMIN_DN, ADMIN_PW)?;
        if after != 0 {
            return Err(format!(
                "the administrator's bind after CHGDIRSVRA: {after} (it takes effect at once)"
            ));
        }
        let list = s.command("wrkactjob", CMD)?;
        let sbs = crate::nfsstep::subsystem_of(&list, INSTANCE);
        let row = job_line(s, INSTANCE)?;
        if sbs.as_deref() != Some("QSYSWRK") || !row.contains("QDIRSRV") {
            return Err(format!(
                "{INSTANCE} is not QDIRSRV's in QSYSWRK ({sbs:?}): {row}\n{list}"
            ));
        }
        let net = s.command(
            "svc.sql SELECT LOCAL_PORT, TCP_STATE FROM QSYS2/NETSTAT_INFO WHERE LOCAL_PORT = 389;",
            Duration::from_secs(30),
        )?;
        if !net.contains("389") || !net.to_ascii_uppercase().contains("LISTEN") {
            return Err(format!("389 is not listening in NETSTAT_INFO:\n{net}"));
        }
        let name = root_dse(p, "ibm-slapdServerInstanceName")?;
        if !name.eq_ignore_ascii_case(INSTANCE) {
            return Err(format!("the root DSE names instance {name}"));
        }
        let log = s.command(&format!("svc.dspjoblog JOB({INSTANCE})"), CMD)?;
        if !log.contains("GLD0269") {
            return Err(format!("no GLD0269 in {INSTANCE}'s job log:\n{log}"));
        }
        let mark = s.log.len();
        end(s)?;
        self.dir.borrow_mut().started = false;
        // The ended job's log, else its line on the serial console.
        let ended = s
            .command(&format!("svc.dspjoblog JOB({INSTANCE})"), CMD)
            .unwrap_or_default();
        let gld0265 = if ended.contains("GLD0265") {
            "in the ended job's log"
        } else if s.log.get(mark..).is_some_and(|l| l.contains("GLD0265")) {
            "on the console (the ended job's log is not kept)"
        } else {
            return Err(format!(
                "no GLD0265 after ENDTCPSVR:\n{ended}\n(serial since:)\n{}",
                s.log.get(mark..).unwrap_or("")
            ));
        };
        if bind_rc(p, "", "").is_ok_and(|rc| rc == 0) {
            return Err("389 still answers after ENDTCPSVR".into());
        }
        start(s)?;
        wait_ldap(p, Duration::from_secs(60))?;
        self.dir.borrow_mut().started = true;
        Ok(format!(
            "GLD0202 without a password; administrator bind {before:?} before CHGDIRSVRA, 0 after; QUSRDIR as QDIRSRV in QSYSWRK listening on 389 (host {p}); GLD0269; ENDTCPSVR ended it, GLD0265 {gld0265}; started again"
        ))
    }

    /// Step 187: `ldaptest` from the host, and the read-only answers
    /// against the primary's record.
    pub(crate) fn dir_ldaptest(&self, s: &mut Session) -> Result<String, String> {
        let r = self.dir_ldaptest_body(s);
        self.dir_checked(s, "187", r)
    }

    fn dir_ldaptest_body(&self, s: &mut Session) -> Result<String, String> {
        let (p, sp) = self.dir_ensure(s)?;
        let env = [("LDAPTEST_ADMIN_PW", ADMIN_PW)];
        let args: Vec<String> = [
            "127.0.0.1",
            "--port",
            &p.to_string(),
            "--ldaps-port",
            &sp.to_string(),
            "--timeout",
            "60",
            "--suite",
            "read,controls,ext,write,acl,ldif",
            "--suffix",
            SUFFIX,
        ]
        .iter()
        .map(|a| (*a).to_string())
        .collect();
        let r = tool::run(&args, &env, RUN_LIMIT)?;
        let _ = std::fs::write(self.out_dir.join("dirsrv-187-ldaptest.txt"), r.text());
        let o = Outcome::of(&r.out);
        if o.ok.is_empty() {
            return Err(format!("ldaptest ran nothing:\n{}", r.text()));
        }
        let bad = new_failures(&o);
        // The read-only scenarios as the primary's record has them.
        let ours = self.out_dir.join("dirsrv-ldaptest-qs400.txt");
        let oracle: Vec<String> = [
            "oracle",
            "--port",
            &p.to_string(),
            "--ldaps-port",
            &sp.to_string(),
            "--out",
            &ours.display().to_string(),
        ]
        .iter()
        .map(|a| (*a).to_string())
        .collect();
        let rec = tool::run(&oracle, &[("QS_ORACLE_HOST", "127.0.0.1")], RUN_LIMIT)?;
        let theirs = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("oracle/dirsrv/ldaptest-primary.txt"),
        )
        .map_err(|e| format!("the primary's record: {e}"))?;
        let ours_text = std::fs::read_to_string(&ours)
            .map_err(|e| format!("{}: {e}\n{}", ours.display(), rec.text()))?;
        let d = tool::diff(&theirs, &ours_text);
        let dpath = self.out_dir.join("dirsrv-187-oracle.diff");
        let _ = std::fs::write(&dpath, d.join("\n") + "\n");
        let scen = d.iter().filter(|l| l.starts_with("@ ")).count();
        if !bad.is_empty() {
            return Err(format!(
                "{} scenarios failed:\n{}\n(the whole run: {})",
                bad.len(),
                bad.join("\n"),
                self.out_dir.join("dirsrv-187-ldaptest.txt").display()
            ));
        }
        Ok(format!(
            "{} scenarios passed, {} skipped ({}); the read-only record differs from the primary's in {scen} scenarios ({} lines, {})",
            o.ok.len(),
            o.skipped.len(),
            o.skipped
                .iter()
                .map(|l| Outcome::name(l))
                .collect::<Vec<_>>()
                .join(" "),
            d.len(),
            dpath.display()
        ))
    }

    /// Step 188: the projected backend and profile binds.
    pub(crate) fn dir_projected(&self, s: &mut Session) -> Result<String, String> {
        let r = self.dir_projected_body(s);
        self.dir_checked(s, "188", r)
    }

    fn dir_projected_body(&self, s: &mut Session) -> Result<String, String> {
        let (p, _) = self.dir_ensure(s)?;
        clean(
            s,
            "CHGSECAUD QAUDCTL(*AUDLVL) QAUDLVL(*AUTFAIL *SECURITY)",
            SLOW,
        )?;
        clean(s, "CHGSYSVAL SYSVAL(QMAXSIGN) VALUE(3)", CMD)?;
        clean(s, "CHGSYSVAL SYSVAL(QMAXSGNACN) VALUE('2')", CMD)?;
        for (name, pw, extra) in PROFILES {
            let _ = s.command(&format!("DLTUSRPRF USRPRF({name}) OWNOBJOPT(*DLT)"), CMD);
            clean(
                s,
                &format!(
                    "CRTUSRPRF USRPRF({name}) PASSWORD('{pw}') PWDEXP(*NO) TEXT('Step 188 {name}'){extra}"
                ),
                CMD,
            )?;
        }
        clean(
            s,
            "CHGFCNUSG FCNID(QIBM_DIRSRV_ADMIN) USER(LDADM) USAGE(*ALLOWED)",
            CMD,
        )?;
        let root = root_dse(p, "ibm-osregistrycontext")?;
        // ldaptest's projected suite.
        let env = [
            ("LDAPTEST_ADMIN_PW", ADMIN_PW),
            ("LDAPTEST_PROFILE_PW", pw_of("LDUSER")),
            ("LDAPTEST_DISABLED_PW", pw_of("LDDIS")),
        ];
        let args: Vec<String> = [
            "127.0.0.1",
            "--port",
            &p.to_string(),
            "--suite",
            "projected",
            "--profile",
            "LDUSER",
            "--disabled-profile",
            "LDDIS",
        ]
        .iter()
        .map(|a| (*a).to_string())
        .collect();
        let r = tool::run(&args, &env, RUN_LIMIT)?;
        let _ = std::fs::write(self.out_dir.join("dirsrv-188-ldaptest.txt"), r.text());
        let o = Outcome::of(&r.out);
        let bad = new_failures(&o);
        if !bad.is_empty() || !o.skipped.is_empty() || o.ok.is_empty() {
            return Err(format!("ldaptest projected:\n{}", r.text()));
        }
        // QMAXSIGN: three wrong passwords disable the profile (QMAXSGNACN 2).
        let bad_dn = profile_dn(&root, "LDBAD");
        for i in 1..=3 {
            let rc = bind_rc(p, &bad_dn, "Wrong2026x")?;
            if rc != 49 {
                return Err(format!("wrong password {i}: {rc} rather than 49"));
            }
        }
        let right = bind_rc(p, &bad_dn, pw_of("LDBAD"))?;
        if right != 49 {
            return Err(format!(
                "LDBAD's right password after QMAXSIGN wrong ones: {right} rather than 49"
            ));
        }
        let prf = s.command("DSPUSRPRF USRPRF(LDBAD)", CMD)?;
        if !prf.contains("*DISABLED") {
            return Err(format!("LDBAD is not *DISABLED after 3 wrong binds:\n{prf}"));
        }
        let pw = s.command("DSPAUDJRNE ENTTYP(PW)", CMD)?;
        if !pw.contains("LDBAD") {
            return Err(format!("no PW entry for LDBAD:\n{pw}"));
        }
        // QIBM_DIRSRV_ADMIN: an administrator; a plain profile is not.
        let mut a = bound(p, &profile_dn(&root, "LDADM"), pw_of("LDADM"))?;
        let all = search(&mut a, &root, Scope::Sub, "(objectClass=os400-usrprf)", &["1.1"])?;
        if all.entries.len() < 5 {
            return Err(format!(
                "LDADM (QIBM_DIRSRV_ADMIN) sees {} profiles",
                all.entries.len()
            ));
        }
        let ou = format!("ou=ldadm188,{SUFFIX}");
        let added = add(
            &mut a,
            &ou,
            &[("objectClass", &["organizationalUnit"]), ("ou", &["ldadm188"])],
        )?;
        let deleted = delete(&mut a, &ou)?;
        if added != 0 || deleted != 0 {
            return Err(format!("LDADM's add {added}, delete {deleted}"));
        }
        let mut u = bound(p, &profile_dn(&root, "LDUSER"), pw_of("LDUSER"))?;
        let refused = add(
            &mut u,
            &ou,
            &[("objectClass", &["organizationalUnit"]), ("ou", &["ldadm188"])],
        )?;
        if refused != 50 {
            return Err(format!("LDUSER's add: {refused} rather than 50"));
        }
        let seen = search(&mut u, &root, Scope::Sub, "(objectClass=os400-usrprf)", &["1.1"])?;
        Ok(format!(
            "ldaptest projected: {} ok ({}); LDBAD disabled after 3 wrong binds, its right password then 49, a PW entry; LDADM (QIBM_DIRSRV_ADMIN) sees {} profiles and adds under {SUFFIX}; LDUSER sees {} and its add is 50",
            o.ok.len(),
            o.ok.join(" "),
            all.entries.len(),
            seen.entries.len()
        ))
    }

    /// Step 189: LDAPS, `StartTLS` and SASL `EXTERNAL`.
    pub(crate) fn dir_tls(&self, s: &mut Session) -> Result<String, String> {
        let r = self.dir_tls_body(s);
        self.dir_checked(s, "189", r)
    }

    fn dir_tls_body(&self, s: &mut Session) -> Result<String, String> {
        let (p, sp) = self.dir_ensure(s)?;
        let slow = Duration::from_secs(180);
        // The store may be there already (step 70).
        let _ = s.command("QSCRTCRTS KEYSTORE(*SYSTEM) PASSWORD('step189pw')", slow)?;
        for l in [SERVER_LABEL, CLIENT_LABEL] {
            let _ = s.command(&format!("QSDLTCRT LABEL({l})"), slow);
        }
        want(
            s,
            &format!("QSCRTCRT LABEL({SERVER_LABEL}) CN('qs400.dirsrv.test') TEXT('Step 189')"),
            &["TCP1C02", "TCP1C14"],
            slow,
        )?;
        want(
            s,
            &format!("QSASNCRT APPID(QS400_DIRECTORY_SERVER) LABEL({SERVER_LABEL})"),
            &["TCP1C08"],
            slow,
        )?;
        // The client's certificate, its subject an entry's DN.
        want(
            s,
            &format!(
                "QSCRTCRT LABEL({CLIENT_LABEL}) CN('ldext') ORG('qs400test') USAGE(*CLIENT) TEXT('Step 189 client')"
            ),
            &["TCP1C02", "TCP1C14"],
            slow,
        )?;
        let pem_path = "/tmp/ldcli189.pem";
        clean(
            s,
            &format!(
                "QSEXPCRT LABEL({CLIENT_LABEL}) TOSTMF('{pem_path}') INCKEY(*YES) REPLACE(*YES)"
            ),
            slow,
        )?;
        let pem = s.command(&format!("svc.ifs cat {pem_path}"), CMD)?;
        let certs = pem_blocks(&pem, "CERTIFICATE");
        let keys = pem_blocks(&pem, "PRIVATE KEY");
        let client = if let (Some(c), Some(k)) = (certs.first(), keys.first()) {
            let cp = self.out_dir.join("dirsrv-189-client.crt");
            let kp = self.out_dir.join("dirsrv-189-client.key");
            std::fs::write(&cp, c).map_err(|e| e.to_string())?;
            std::fs::write(&kp, k).map_err(|e| e.to_string())?;
            Some((cp, kp))
        } else {
            return Err(format!("QSEXPCRT's file holds no certificate and key:\n{pem}"));
        };
        clean(
            s,
            "QSCHGAPPID APPID(QS400_DIRECTORY_SERVER) CLTAUTH(*OPTIONAL) CATRUST(*ALL)",
            slow,
        )?;
        // SSLTLS and a suffix for the client's entry, through cn=configuration.
        let mut a = admin(p)?;
        let rc = replace(&mut a, "cn=SSL, cn=Configuration", "ibm-slapdSecurity", &["SSLTLS"])?;
        if rc != 0 {
            return Err(format!("ibm-slapdSecurity: SSLTLS: {rc}"));
        }
        let dir_dn = "cn=Directory, cn=RDBM Backends, cn=IBM Directory, cn=Schemas, cn=Configuration";
        let sfx = search(&mut a, dir_dn, Scope::Base, "(objectClass=*)", &["ibm-slapdSuffix"])?;
        let mut suffixes: Vec<String> = sfx
            .entries
            .first()
            .and_then(|e| e.values("ibm-slapdSuffix"))
            .unwrap_or_default()
            .iter()
            .map(|v| String::from_utf8_lossy(v).into_owned())
            .collect();
        if !suffixes.iter().any(|x| x.eq_ignore_ascii_case(EXT_SUFFIX)) {
            suffixes.push(EXT_SUFFIX.into());
            let v: Vec<&str> = suffixes.iter().map(String::as_str).collect();
            let rc = replace(&mut a, dir_dn, "ibm-slapdSuffix", &v)?;
            if rc != 0 {
                return Err(format!("ibm-slapdSuffix {EXT_SUFFIX}: {rc}"));
            }
        }
        let _ = a.unbind();
        let p = self.dir_restart(s)?;
        let sec = root_dse(p, "security").unwrap_or_default();
        let mut a = admin(p)?;
        if !exists(&mut a, EXT_SUFFIX)? {
            let rc = add(
                &mut a,
                EXT_SUFFIX,
                &[("objectClass", &["organization"]), ("o", &["qs400test"])],
            )?;
            if rc != 0 {
                return Err(format!("add {EXT_SUFFIX}: {rc}"));
            }
        }
        if !exists(&mut a, EXT_DN)? {
            let rc = add(
                &mut a,
                EXT_DN,
                &[
                    ("objectClass", &["person"]),
                    ("cn", &["ldext"]),
                    ("sn", &["External"]),
                ],
            )?;
            if rc != 0 {
                return Err(format!("add {EXT_DN}: {rc}"));
            }
        }
        let _ = a.unbind();
        let mut args: Vec<String> = [
            "127.0.0.1",
            "--port",
            &p.to_string(),
            "--ldaps-port",
            &sp.to_string(),
            "--suite",
            "ext",
        ]
        .iter()
        .map(|a| (*a).to_string())
        .collect();
        if let Some((c, k)) = &client {
            args.extend([
                "--client-cert".into(),
                c.display().to_string(),
                "--client-key".into(),
                k.display().to_string(),
            ]);
        }
        let r = tool::run(&args, &[("LDAPTEST_ADMIN_PW", ADMIN_PW)], RUN_LIMIT)?;
        let _ = std::fs::write(self.out_dir.join("dirsrv-189-ldaptest.txt"), r.text());
        let o = Outcome::of(&r.out);
        let bad = new_failures(&o);
        let need = ["starttls", "ldaps", "sasl-external", "sasl-external-no-cert"];
        let missing: Vec<&str> = need.iter().copied().filter(|n| !o.passed(n)).collect();
        if !bad.is_empty() || !missing.is_empty() {
            return Err(format!(
                "ldaptest ext: {missing:?} did not pass:\n{}",
                r.text()
            ));
        }
        Ok(format!(
            "{SERVER_LABEL} assigned to QS400_DIRECTORY_SERVER, ibm-slapdSecurity SSLTLS (root DSE security {sec:?}); ldaptest ext: {} ok ({}), {} skipped; EXTERNAL as {EXT_DN} with {CLIENT_LABEL}",
            o.ok.len(),
            o.ok.join(" "),
            o.skipped.len()
        ))
    }

    /// Step 190: the QSH tools and an ILE program over `QGLDCLNT`, in
    /// the guest against its own instance.
    pub(crate) fn dir_guest_tools(&self, s: &mut Session) -> Result<String, String> {
        let r = self.dir_guest_tools_body(s);
        self.dir_checked(s, "190", r)
    }

    fn dir_guest_tools_body(&self, s: &mut Session) -> Result<String, String> {
        let (p, _) = self.dir_ensure(s)?;
        let auth = format!("-h localhost -D {ADMIN_DN} -w {ADMIN_PW}");
        let base = format!("ou=t190,{SUFFIX}");
        let alice = format!("cn=alice,{base}");
        let ok = |s: &mut Session, what: &str, script: &str| -> Result<String, String> {
            let (fine, out) = qsh(s, script, Duration::from_secs(120))?;
            if fine {
                Ok(out)
            } else {
                Err(format!("{what}: {script}\n{out}"))
            }
        };
        let out = ok(
            s,
            "ldapsearch",
            &format!("ldapsearch -h localhost -b {SUFFIX} -s base objectclass=*"),
        )?;
        if !out.to_ascii_lowercase().contains("cn=localhost") {
            return Err(format!("ldapsearch of {SUFFIX}:\n{out}"));
        }
        // The records, written a line at a time.
        let mut add = String::new();
        for l in [
            format!("dn: {base}"),
            "objectClass: organizationalUnit".into(),
            "ou: t190".into(),
            String::new(),
            format!("dn: {alice}"),
            "objectClass: person".into(),
            "cn: alice".into(),
            "sn: Smith".into(),
        ] {
            let _ = write!(add, "echo \"{l}\" >> /tmp/a190.ldif; ");
        }
        ok(s, "write", &format!("rm -f /tmp/a190.ldif; {add}"))?;
        ok(s, "ldapadd", &format!("ldapadd {auth} -f /tmp/a190.ldif"))?;
        let mut m = String::new();
        for l in [
            format!("dn: {alice}"),
            "changetype: modify".into(),
            "replace: description".into(),
            "description: changed190".into(),
        ] {
            let _ = write!(m, "echo \"{l}\" >> /tmp/m190.ldif; ");
        }
        ok(s, "write", &format!("rm -f /tmp/m190.ldif; {m}"))?;
        ok(s, "ldapmodify", &format!("ldapmodify {auth} -f /tmp/m190.ldif"))?;
        ok(
            s,
            "ldapcompare (true)",
            &format!("ldapcompare {auth} \"{alice}\" description=changed190"),
        )?;
        let (f, no) = qsh(
            s,
            &format!("ldapcompare {auth} \"{alice}\" description=other"),
            CMD,
        )?;
        if f || !no.contains("exit status 5.") {
            return Err(format!("ldapcompare (false) did not end with status 5:\n{no}"));
        }
        ok(
            s,
            "ldapmodrdn",
            &format!("ldapmodrdn {auth} -r \"{alice}\" cn=alicia"),
        )?;
        let alicia = format!("cn=alicia,{base}");
        let found = ok(
            s,
            "ldapsearch",
            &format!("ldapsearch {auth} -b {base} \"(cn=alicia)\" description"),
        )?;
        if !found.contains("changed190") {
            return Err(format!("the renamed entry's description:\n{found}"));
        }
        let who = ok(s, "ldapexop", &format!("ldapexop {auth} -op whoami"))?;
        if !who.to_ascii_lowercase().contains("cn=administrator") {
            return Err(format!("ldapexop whoami:\n{who}"));
        }
        // The ILE program.
        let name = "LDAPC";
        let _ = s.command(&format!("DLTPGM PGM(QGPL/{name})"), CMD);
        let _ = s.command(&format!("DLTMOD MODULE(QGPL/{name})"), CMD);
        let text = super::prog::client(
            name,
            "localhost:389",
            ADMIN_DN,
            ADMIN_PW,
            &base,
            (&alicia, "sn", "Smith"),
        );
        crate::phase5::build(s, name, &text)?;
        let called = s.command(&format!("CALL PGM(QGPL/{name})"), Duration::from_secs(60))?;
        let said = |t: &str| super::prog::said(&called, t);
        let got = (
            said("LDAPC-BIND"),
            said("LDAPC-SRCH"),
            said("LDAPC-COUNT"),
            said("LDAPC-CMP"),
        );
        if got != (Some(0), Some(0), Some(2), Some(6)) {
            return Err(format!(
                "{name}: bind, search, count, compare {got:?} rather than 0, 0, 2, 6:\n{called}"
            ));
        }
        ok(
            s,
            "ldapdelete",
            &format!("ldapdelete {auth} \"{alicia}\" \"{base}\""),
        )?;
        let mut a = admin(p)?;
        if exists(&mut a, &base)? {
            return Err(format!("{base} is still there after ldapdelete"));
        }
        Ok("ldapsearch, ldapadd, ldapmodify, ldapcompare (0 and 5), ldapmodrdn, ldapexop whoami, ldapdelete in QSH; LDAPC over QGLDCLNT: bind 0, search 0, 2 entries, compare true".into())
    }

    /// Step 191: the users agent, configured in `publish.conf` to bind
    /// anonymously to a parent whose ACL lets anyone write below it,
    /// publishes a new profile and removes a deleted one. (Navigator's
    /// and QAP's way in, `dir_admin` 1568, is not served yet: §18.)
    pub(crate) fn dir_publish(&self, s: &mut Session) -> Result<String, String> {
        let r = self.dir_publish_body(s);
        self.dir_checked(s, "191", r)
    }

    fn dir_publish_body(&self, s: &mut Session) -> Result<String, String> {
        let (p, _) = self.dir_ensure(s)?;
        job_line(s, "QGLDPUBE")?;
        let mut a = admin(p)?;
        if !exists(&mut a, PUB_PARENT)? {
            let rc = add(
                &mut a,
                PUB_PARENT,
                &[
                    ("objectClass", &["organizationalUnit"]),
                    ("ou", &["Users"]),
                    ("aclEntry", &[PUB_ACL]),
                ],
            )?;
            if rc != 0 {
                return Err(format!("add {PUB_PARENT}: {rc}"));
            }
        }
        let _ = a.unbind();
        let conf = format!(
            "version: 1\n\ndn: cn=publishing\nqs400-pubFormat: 1\n\ndn: cn=users,cn=publishing\nqs400-pubEnabled: TRUE\nqs400-pubHost: localhost\nqs400-pubPort: 389\nqs400-pubParentDN: {PUB_PARENT}\nqs400-pubCreateParent: FALSE\nqs400-pubRetry: 10\nqs400-pubResync: 60"
        );
        let put = s.command(&format!("svc.ifs put {PUB_CONF} {conf}"), CMD)?;
        if !put.contains("bytes") {
            return Err(format!("{PUB_CONF}: {put}"));
        }
        let _ = s.command(&format!("DLTUSRPRF USRPRF({PUB_USER})"), CMD);
        clean(
            s,
            &format!("CRTUSRPRF USRPRF({PUB_USER}) PASSWORD(*NONE) TEXT('Published Person')"),
            CMD,
        )?;
        let filter = format!("(uid={PUB_USER})");
        let find = |want: bool| -> Result<(Duration, Option<String>), String> {
            let t0 = Instant::now();
            let end = t0 + Duration::from_secs(240);
            loop {
                let mut a = admin(p)?;
                let r = search(&mut a, PUB_PARENT, Scope::Sub, &filter, &["cn", "publisherName"])?;
                let _ = a.unbind();
                let e = r.entries.first().map(|e| {
                    format!(
                        "{} cn={:?} publisherName={:?}",
                        e.dn,
                        e.first("cn"),
                        e.first("publisherName")
                    )
                });
                if e.is_some() == want {
                    return Ok((t0.elapsed(), e));
                }
                if Instant::now() > end {
                    return Err(format!(
                        "{PUB_USER} {} under {PUB_PARENT} within 240 s ({:?})",
                        if want { "not published" } else { "still published" },
                        r.result
                    ));
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        };
        let (t_add, entry) = find(true)?;
        let entry = entry.unwrap_or_default();
        if !entry.contains("Published Person") {
            return Err(format!("the published entry: {entry}"));
        }
        clean(s, &format!("DLTUSRPRF USRPRF({PUB_USER})"), CMD)?;
        let (t_del, _) = find(false)?;
        Ok(format!(
            "users agent enabled in publish.conf (anonymous, under an ACL that lets anyone write); {PUB_USER} published in {:.0} s as {entry}; gone {:.0} s after DLTUSRPRF",
            t_add.as_secs_f64(),
            t_del.as_secs_f64()
        ))
    }

    /// Step 192: the entries survive a restart, an IPL and a save and
    /// restore; `RCLSTG` finds nothing wrong.
    pub(crate) fn dir_persist(&self, s: &mut Session) -> Result<String, String> {
        let r = self.dir_persist_body(s);
        self.dir_checked(s, "192", r)
    }

    fn dir_persist_body(&self, s: &mut Session) -> Result<String, String> {
        let (p, _) = self.dir_ensure(s)?;
        let ou = format!("ou=p192,{SUFFIX}");
        let keep = format!("cn=keep,{ou}");
        let mut a = admin(p)?;
        if !exists(&mut a, &ou)? {
            add(
                &mut a,
                &ou,
                &[("objectClass", &["organizationalUnit"]), ("ou", &["p192"])],
            )?;
        }
        let rc = add(
            &mut a,
            &keep,
            &[
                ("objectClass", &["person"]),
                ("cn", &["keep"]),
                ("sn", &["Kept"]),
                ("description", &["made by step 192"]),
            ],
        )?;
        if rc != 0 && rc != 68 {
            return Err(format!("add {keep}: {rc}"));
        }
        let _ = a.unbind();
        let there = |p: u16| -> Result<bool, String> {
            let mut a = admin(p)?;
            let r = exists(&mut a, &keep);
            let _ = a.unbind();
            r
        };
        let p = self.dir_restart(s)?;
        if !there(p)? {
            return Err(format!("{keep} lost across ENDTCPSVR and STRTCPSVR"));
        }
        // An IPL: the forwards stay (the same QEMU), the instance does not.
        s.send("PWRDWNSYS OPTION(*IMMED) RESTART(*YES)")?;
        s.expect("SRC D1001090", Duration::from_secs(180))?;
        s.expect_in_order(crate::steps::SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        crate::tcp::guest_address(s)?;
        self.dir.borrow_mut().started = false;
        full_log(s)?;
        start(s)?;
        wait_ldap(p, Duration::from_secs(90))?;
        self.dir.borrow_mut().started = true;
        if !there(p)? {
            return Err(format!("{keep} lost across the IPL"));
        }
        // The library saved, the entry deleted, the library restored.
        end(s)?;
        self.dir.borrow_mut().started = false;
        let _ = s.command("DLTF FILE(QGPL/DIRSAVF)", CMD);
        clean(s, "CRTSAVF FILE(QGPL/DIRSAVF)", CMD)?;
        let saved = want(
            s,
            "SAVLIB LIB(QUSRDIRDB) DEV(*SAVF) SAVF(QGPL/DIRSAVF)",
            &["CPC3722"],
            SLOW,
        )?;
        if !errors(&saved).is_empty() {
            return Err(format!("SAVLIB QUSRDIRDB:\n{saved}"));
        }
        let contents = s.command("DSPSAVF FILE(QGPL/DIRSAVF)", CMD)?;
        if !contents.contains("*DIRDB") {
            return Err(format!("the save file holds no *DIRDB:\n{contents}"));
        }
        start(s)?;
        wait_ldap(p, Duration::from_secs(60))?;
        let mut a = admin(p)?;
        let rc = delete(&mut a, &keep)?;
        let gone = !exists(&mut a, &keep)?;
        let _ = a.unbind();
        if rc != 0 || !gone {
            return Err(format!("delete {keep}: {rc}, gone {gone}"));
        }
        end(s)?;
        let restored = want(
            s,
            "RSTLIB SAVLIB(QUSRDIRDB) DEV(*SAVF) SAVF(QGPL/DIRSAVF)",
            &["CPC3703"],
            SLOW,
        )?;
        if !errors(&restored).is_empty() {
            return Err(format!("RSTLIB QUSRDIRDB:\n{restored}"));
        }
        start(s)?;
        wait_ldap(p, Duration::from_secs(60))?;
        self.dir.borrow_mut().started = true;
        if !there(p)? {
            return Err(format!("{keep} is not back after RSTLIB"));
        }
        let rcl = s.command("svc.rclstg *CHECK", Duration::from_secs(600))?;
        if !rcl.contains("0 problem(s)") {
            return Err(format!("svc.rclstg *CHECK:\n{rcl}"));
        }
        let objs = s.command("svc.obj ls QUSRDIRDB", CMD)?;
        if !objs.contains(INSTANCE) {
            return Err(format!("QUSRDIRDB holds no {INSTANCE}:\n{objs}"));
        }
        Ok(format!(
            "{keep} kept across ENDTCPSVR/STRTCPSVR and an IPL; SAVLIB QUSRDIRDB (a *DIRDB in the save file), deleted, RSTLIB brought it back; RCLSTG *CHECK 0 problems"
        ))
    }

    /// After the slip install from V1R1M0 (step 10): what V1R2M0 brought
    /// for the directory server is there, and step 186 passes on it.
    pub(crate) fn dir_after_upgrade(&self, s: &mut Session) -> Result<String, String> {
        let conf = "/QIBM/UserData/OS400/DirSrv/idsslapd-QUSRDIR/etc/ibmslapd.conf";
        let inst = "/QIBM/UserData/OS400/DirSrv/idsinstinfo/idsinstances.ldif";
        for path in [conf, inst] {
            if crate::nfsstep::stat(s, path)?.is_none() {
                return Err(format!("{path} is missing after the upgrade"));
            }
        }
        for jobd in ["QDIRSRV", "QGLDPUBE", "QGLDPUBA", "QDIRJOBD"] {
            let out = s.command(&format!("DSPJOBD JOBD(QSYS/{jobd})"), CMD)?;
            if out.contains("CPF") {
                return Err(format!("QSYS/{jobd}: {out}"));
            }
        }
        let prf = s.command("DSPUSRPRF USRPRF(QDIRSRV)", CMD)?;
        if prf.contains("CPF") {
            return Err(format!("QDIRSRV: {prf}"));
        }
        let lib = s.command("DSPLIB LIB(QUSRDIRDB)", CMD)?;
        if lib.contains("CPF") {
            return Err(format!("QUSRDIRDB: {lib}"));
        }
        let started = self.dir_start_body(s)?;
        let objs = s.command("svc.obj ls QUSRDIRDB", CMD)?;
        if !objs.contains(INSTANCE) || !objs.to_ascii_uppercase().contains("DIRDB") {
            return Err(format!("no *DIRDB {INSTANCE} in QUSRDIRDB:\n{objs}"));
        }
        end(s)?;
        self.dir.borrow_mut().started = false;
        Ok(format!(
            "directory server after the upgrade: the shipped files, QDIRSRV, its job descriptions, QUSRDIRDB; {started}; *DIRDB QUSRDIRDB/{INSTANCE} made"
        ))
    }

    /// Step 194: `ldaptest bench`: binds a minute and a paged search over
    /// `QS400_DIRSRV_BENCH_ENTRIES` entries (100 000 by default).
    pub(crate) fn dir_bench(&self, s: &mut Session) -> Result<String, String> {
        let r = self.dir_bench_body(s);
        self.dir_checked(s, "194", r)
    }

    fn dir_bench_body(&self, s: &mut Session) -> Result<String, String> {
        let (p, _) = self.dir_ensure(s)?;
        let entries = std::env::var("QS400_DIRSRV_BENCH_ENTRIES")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(100_000);
        let binds = std::env::var("QS400_DIRSRV_BENCH_BINDS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(10_000);
        let args: Vec<String> = [
            "bench",
            "127.0.0.1",
            "--port",
            &p.to_string(),
            "--timeout",
            "600",
            "--binds",
            &binds.to_string(),
            "--entries",
            &entries.to_string(),
            "--page",
            "1000",
            "--suffix",
            SUFFIX,
        ]
        .iter()
        .map(|a| (*a).to_string())
        .collect();
        let r = tool::run(
            &args,
            &[("LDAPTEST_ADMIN_PW", ADMIN_PW)],
            Duration::from_hours(4),
        )?;
        let path: PathBuf = self.out_dir.join("dirsrv-194-bench.txt");
        let _ = std::fs::write(&path, r.text());
        if r.code != Some(0) {
            return Err(format!("ldaptest bench:\n{}", r.text()));
        }
        let lines: Vec<&str> = r.out.lines().filter(|l| !l.trim().is_empty()).collect();
        Ok(lines.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_blocks_of_an_export() {
        let t = "junk\n-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n-----BEGIN PRIVATE KEY-----\nBBBB\n-----END PRIVATE KEY-----\n";
        assert_eq!(
            pem_blocks(t, "CERTIFICATE"),
            ["-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n"]
        );
        assert_eq!(pem_blocks(t, "PRIVATE KEY").len(), 1);
        assert_eq!(pw_of("LDDIS"), "Ldap2026d");
        assert_eq!(
            profile_dn("os400-sys=X", "A"),
            "os400-profile=A,cn=accounts,os400-sys=X"
        );
    }
}
