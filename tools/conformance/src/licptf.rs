// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QEMU step 164, a LIC PTF end to end, in a guest of its own with a
//! writable OVMF variable store (`BootNext` must survive the reset):
//! the PTF is built on the host as `build ptf` builds one (this build's
//! slot UKIs, the build's key), published by the update service's
//! reference server on host loopback, ordered by the guest with
//! `SNDPTFORD` from 10.0.2.2, loaded, applied (its image into slot B),
//! made effective by an IPL from slot B (`C6004503`), then removed by an
//! IPL back on slot A (`docs/PTF.md` section 5).

use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, SRC_IPL_COMPLETE, StepReport, Suite};

/// The step's name.
const NAME: &str = "164 LIC PTF";
/// The PTF.
const PTF: &str = "QS16401";
/// The PTF's command lines take the storage service a while: the
/// order moves the whole stream, the apply writes an image to the ESP.
const LONG: Duration = Duration::from_secs(300);
/// The reference server's port (the step is a plain `fn`).
static PORT: AtomicU16 = AtomicU16::new(0);

/// The build's file a pointer names.
fn pointed(name: &str) -> Result<PathBuf, String> {
    let p = crate::target_dir().join("images").join(name);
    let t = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(PathBuf::from(t.trim()))
}

/// The PTF, built as `build ptf` builds `lic = "*BUILD"` (without the
/// Secure Boot signature: the guest's firmware does not check one).
fn build_ptf() -> Result<Vec<u8>, String> {
    let key_text = fs::read(crate::target_dir().join("keys/dev.ed25519"))
        .map_err(|e| format!("the build's dev key: {e}"))?;
    let key = ptf::manifest::signing_key(&key_text).map_err(|e| e.to_string())?;
    let a = fs::read(pointed("uki-a.path")?).map_err(|e| format!("slot A UKI: {e}"))?;
    let b = fs::read(pointed("uki-b.path")?).map_err(|e| format!("slot B UKI: {e}"))?;
    let m = ptf::manifest::Manifest::parse(&format!(
        "ptf = \"{PTF}\"\nproduct = \"QS00SS1\"\nkind = \"delayed\"\nlic = \"*BUILD\"\n\
         cover = \"{PTF} replaces the Licensed Internal Code for the conformance suite.\"\n"
    ))
    .map_err(|e| e.to_string())?;
    let built = ptfbuild::build(&ptfbuild::Input {
        manifest: &m,
        cover: m.cover.clone(),
        lic: Some(&a),
        lic_b: Some(&b),
        key: &key,
        now: 1_794_000_000,
    })
    .map_err(|e| e.to_string())?;
    Ok(built.stream)
}

/// The update service's directory with the PTF, as `build ptf-catalog`
/// writes it.
fn publish(dir: &std::path::Path, stream: Vec<u8>) -> Result<(), String> {
    let key_text = fs::read(crate::target_dir().join("keys/dev.ed25519"))
        .map_err(|e| format!("the build's dev key: {e}"))?;
    let key = ptf::manifest::signing_key(&key_text).map_err(|e| e.to_string())?;
    let groups = [ptf_update::publish::GroupDef {
        name: "C26312110".into(),
        kind: ptf_update::catalog::GroupKind::Cumulative,
        level: 1,
        release: release::RELEASE.to_string(),
        text: format!("Cumulative PTF package of {}: {PTF}", release::RELEASE),
        ptfs: vec![PTF.into()],
    }];
    ptf_update::publish::publish(
        dir,
        &ptf_update::publish::Publication {
            ptfs: &[stream],
            hiper: &[],
            groups: &groups,
            sequence: 2,
            now: 1_794_000_000,
        },
        &key,
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn want(out: &str, what: &str, needles: &[&str]) -> Result<(), String> {
    if needles.iter().all(|n| out.contains(n)) {
        Ok(())
    } else {
        Err(format!("{what}: expected {needles:?}:\n{out}"))
    }
}

impl Suite {
    /// Step 164 in its guest; with `--only` when named.
    pub(crate) fn lic_ptf_steps(&self) -> Result<Vec<StepReport>, crate::HarnessError> {
        if let Some(only) = &self.only
            && !only.iter().any(|p| NAME.starts_with(p.as_str()))
        {
            return Ok(Vec::new());
        }
        let fail = |reason: String| crate::HarnessError::Failed {
            step: NAME,
            reason,
            log: String::new(),
        };
        let template = crate::firmware::locate_vars(&self.launch.firmware).ok_or_else(|| {
            fail(format!(
                "no OVMF variable store beside {} (set {})",
                self.launch.firmware.display(),
                crate::firmware::VARS_ENV
            ))
        })?;
        let vars = self.out_dir.join("lic-ptf-vars.fd");
        fs::copy(&template, &vars)?;
        // A copy of the disk: the step moves its slots.
        let disk = self.out_dir.join("lic-ptf.img");
        if let Some(img) = &self.launch.image {
            fs::copy(img, &disk)?;
        }
        let dir = self.out_dir.join("update-service");
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        publish(&dir, build_ptf().map_err(fail)?).map_err(fail)?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let server = ptf_update::server::Server::new(&dir).spawn(listener, "/qs400")?;
        PORT.store(server.addr.port(), Ordering::SeqCst);
        let launch = Launch {
            vars: Some(vars),
            image: Some(disk),
            ..self.launch.clone()
        };
        let mut s = Session::spawn(&launch)?;
        let r = self.step(&mut s, NAME, Self::lic_ptf)?;
        s.command("pwrdwn", CMD).ok();
        s.wait_exit(Duration::from_secs(20));
        server.stop();
        Ok(vec![r])
    }

    /// Wait for an IPL to finish at the service console.
    fn ipl_done(&self, s: &mut Session) -> Result<(), String> {
        s.expect(SRC_IPL_COMPLETE, 4 * self.boot_timeout)?;
        s.expect("Service console ready", 4 * self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        Ok(())
    }

    /// `pwrdwn restart`, then the IPL; `src` must show during it.
    fn ipl_with(&self, s: &mut Session, src: Option<&str>) -> Result<(), String> {
        s.send("pwrdwn restart")?;
        s.expect("SRC D1001000", CMD)?;
        if let Some(src) = src {
            s.expect(&format!("SRC {src}"), 4 * self.boot_timeout)?;
        }
        self.ipl_done(s)
    }

    fn lic_ptf(&self, s: &mut Session) -> Result<String, String> {
        let port = PORT.load(Ordering::SeqCst);
        self.ipl_done(s)?;
        let ptf =
            |s: &mut Session| s.command(&format!("dspptf LICPGM(QS00SS1) SELECT({PTF})"), CMD);
        want(&ptf(s)?, "DSPPTF before", &["IPL source ##MACH#A"])?;

        // The order from the update service on the host.
        let url = format!("http://10.0.2.2:{port}/qs400");
        let set = s.command(&format!("QSCHGUPDA URL('{url}')"), CMD)?;
        if set.contains("CPD") || set.contains("CPF") {
            return Err(format!("QSCHGUPDA: {set}"));
        }
        let got = s.command(&format!("SNDPTFORD PTFID(({PTF}))"), LONG)?;
        want(
            &got,
            "SNDPTFORD",
            &[&format!("PTF {PTF} received as save file QGPL/{PTF}")],
        )?;

        let loaded = s.command(
            &format!("LODPTF LICPGM(QS00SS1) DEV(*SAVF) SELECT({PTF}) SAVF(QGPL/{PTF})"),
            LONG,
        )?;
        if loaded.contains("CPF") || loaded.contains("CPD") {
            return Err(format!("LODPTF: {loaded}"));
        }
        let applied = s.command(&format!("APYPTF LICPGM(QS00SS1) SELECT({PTF})"), LONG)?;
        want(
            &applied,
            "APYPTF",
            &[&format!("PTF {PTF}: its LIC image is in slot B")],
        )?;
        want(
            &ptf(s)?,
            "DSPPTF after APYPTF",
            &["Apply at next IPL", "##MACH#A"],
        )?;

        // The IPL boots slot B once; coming up there applies it.
        self.ipl_with(s, Some("C6004503"))?;
        let on_b = ptf(s)?;
        want(
            &on_b,
            "DSPPTF on slot B",
            &["IPL source ##MACH#B", PTF, "Temporarily applied"],
        )?;

        // Removed: the next IPL goes back to slot A, once and for good.
        let removed = s.command(&format!("RMVPTF LICPGM(QS00SS1) SELECT({PTF})"), LONG)?;
        if removed.contains("CPF") || removed.contains("CPD") {
            return Err(format!("RMVPTF: {removed}"));
        }
        self.ipl_with(s, None)?;
        let on_a = ptf(s)?;
        want(
            &on_a,
            "DSPPTF back on slot A",
            &["IPL source ##MACH#A", "Temporarily removed"],
        )?;
        // And it stays there.
        self.ipl_with(s, None)?;
        want(
            &ptf(s)?,
            "DSPPTF after another IPL",
            &["IPL source ##MACH#A"],
        )?;
        Ok(format!(
            "{PTF} ordered from the update service, loaded, applied: IPL on slot B (C6004503), removed: IPL on slot A, twice"
        ))
    }
}
