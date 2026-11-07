// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 105: what Phase 12 closed of the earlier phases' leftovers,
//! end to end: the FTP client's own display, `PGMINFO(*PCML)` into
//! `INFOSTMF`, the host table to and from stream files, a system dump
//! copied to a save file, and an image catalog imported from another's
//! directory.

use tn5250_driver::script::Context;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase5::SECOFR_PASSWORD;
use crate::phase8::{connect, run, secofr};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// An RPG program with entry parameters, for its PCML.
const PCMLPGM: &str = "**FREE
dcl-pi *n;
  who char(10);
  amt packed(9:2);
end-pi;
*inlr = *on;
";

fn want(out: &str, what: &str, words: &[&str]) -> Result<(), String> {
    if words.iter().all(|w| out.contains(w)) {
        Ok(())
    } else {
        Err(format!("{what}:\n{out}"))
    }
}

impl Suite {
    /// Step 105: the leftovers' commands over telnet and the service
    /// console.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn leftovers(&self, s: &mut Session) -> Result<String, String> {
        // PCML into INFOSTMF.
        let _ = s.command("CRTLIB LIB(LEFTOVER)", CMD);
        let _ = s.command("CRTSRCPF FILE(LEFTOVER/QRPGLESRC) RCDLEN(112)", CMD);
        crate::qir::qemu::push(s, "PCMLSRC", PCMLPGM)?;
        let put = s.command(
            "svc.srcput LEFTOVER/QRPGLESRC PCMLPGM SRCTYPE(RPGLE) FROMSTMF(PCMLSRC)",
            CMD,
        )?;
        want(&put, "srcput", &["written"])?;
        let made = s.command(
            "CRTBNDRPG PGM(LEFTOVER/PCMLPGM) SRCFILE(LEFTOVER/QRPGLESRC) PGMINFO(*PCML) INFOSTMF('/tmp/pcmlpgm.pcml')",
            std::time::Duration::from_secs(60),
        )?;
        want(
            &made,
            "CRTBNDRPG PGMINFO(*PCML)",
            &["PCML for PCMLPGM written"],
        )?;
        let pcml = s.command("svc.ifs cat /tmp/pcmlpgm.pcml", CMD)?;
        want(
            &pcml,
            "the PCML",
            &[
                "<pcml version=\"7.0\">",
                "path=\"/QSYS.LIB/LEFTOVER.LIB/PCMLPGM.PGM\"",
                "type=\"packed\" length=\"9\" precision=\"2\"",
            ],
        )?;
        // The host table out to a stream file and a new entry back in.
        let out = s.command("CPYTCPHT TOOBJ('/tmp/hosts') REPLACE(*YES)", CMD)?;
        want(&out, "CPYTCPHT TOOBJ", &["copied to /tmp/hosts"])?;
        let hosts = s.command("svc.ifs cat /tmp/hosts", CMD)?;
        want(&hosts, "the host file", &["127.0.0.1", "LOOPBACK"])?;
        s.command(
            "svc.ifs put /tmp/newhosts 10.9.9.9 QS12HOST  # from a stream file",
            CMD,
        )?;
        let mrg = s.command("MRGTCPHT FROMOBJ('/tmp/newhosts')", CMD)?;
        want(&mrg, "MRGTCPHT FROMOBJ", &["1 entries added"])?;
        s.command("CPYTCPHT TOOBJ('/tmp/hosts') REPLACE(*YES)", CMD)?;
        let again = s.command("svc.ifs cat /tmp/hosts", CMD)?;
        want(&again, "the merged host table", &["10.9.9.9 QS12HOST"])?;
        let _ = s.command("RMVTCPHTE INTNETADR('10.9.9.9')", CMD);
        // A system dump to a save file.
        let taken = s.command("svc.sysdump take", CMD)?;
        let dump = taken
            .split_whitespace()
            .find(|w| w.starts_with("QSD"))
            .ok_or_else(|| format!("no dump taken:\n{taken}"))?
            .trim_end_matches('.')
            .to_string();
        s.command("CRTSAVF FILE(LEFTOVER/DUMPSAVF)", CMD)?;
        let copied = s.command(&format!("svc.sysdump copy {dump} LEFTOVER/DUMPSAVF"), CMD)?;
        let listed = s.command("DSPSAVF FILE(LEFTOVER/DUMPSAVF)", CMD)?;
        if !listed.contains(&dump) {
            return Err(format!(
                "the dump is not in the save file:\n{copied}\n{listed}"
            ));
        }
        let _ = s.command(&format!("svc.sysdump delete {dump}"), CMD);
        // IMPORT(*YES) over a directory: another catalog's is refused by
        // name, an empty one has nothing to take over (the import itself
        // is imgclg's catalog test: the IFS cannot copy the image files
        // the catalogs write straight to the xfs).
        s.command("CRTIMGCLG IMGCLG(LEFTA) DIR('/leftover/a') TYPE(*OPT) CRTDIR(*YES) ADDVRTVOL(1) IMGSIZ(48)", CMD)?;
        let busy = s.command(
            "CRTIMGCLG IMGCLG(LEFTB) DIR('/leftover/a') TYPE(*OPT) IMPORT(*YES)",
            CMD,
        )?;
        let empty = s.command(
            "CRTIMGCLG IMGCLG(LEFTB) DIR('/leftover/e') TYPE(*OPT) CRTDIR(*YES) IMPORT(*YES)",
            CMD,
        )?;
        let _ = s.command("DLTIMGCLG IMGCLG(LEFTA) KEEP(*NO)", CMD);
        if !busy.contains("LEFTA") || empty.contains("created") {
            return Err(format!("IMPORT(*YES) was not checked:\n{busy}\n{empty}"));
        }
        // The FTP client's display, signed on to the loopback server.
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        let mut d = connect(port, "QSECOFR", SECOFR_PASSWORD)?;
        let c = Connect::qs400(port, "QSECOFR", SECOFR_PASSWORD);
        let options = ScriptOptions::default();
        let mut ctx = Context::new(&c, &options);
        secofr(&mut d, &mut ctx)?;
        run(
            &mut d,
            &mut ctx,
            "wait \"Main Menu\"\ntype call qcmd\nkey enter\nwait \"Command Entry\"\n",
        )?;
        run(
            &mut d,
            &mut ctx,
            "type FTP RMTSYS(*INTNETADR) INTNETADR('127.0.0.1')\nkey enter\nwait \"File Transfer Protocol\"\n",
        )?;
        run(&mut d, &mut ctx, "wait \"Enter login ID\"\n")?;
        run(
            &mut d,
            &mut ctx,
            "type QSECOFR\nkey enter\nwait \"Enter password\"\n",
        )?;
        run(
            &mut d,
            &mut ctx,
            &format!("type {SECOFR_PASSWORD}\nkey enter\nwait \"230\"\n"),
        )?;
        run(&mut d, &mut ctx, "type pwd\nkey enter\nwait \"257\"\n")?;
        let shown = d.term.text();
        if !shown.contains(">    pwd")
            || !shown.contains("Previous FTP subcommands and messages:")
            || shown.contains(SECOFR_PASSWORD)
        {
            return Err(format!("the FTP display is not as the primary's:\n{shown}"));
        }
        run(
            &mut d,
            &mut ctx,
            "type quit\nkey enter\nwait \"Command Entry\"\n",
        )?;
        run(
            &mut d,
            &mut ctx,
            "type signoff\nkey enter\nwait \"Sign On\"\n",
        )?;
        d.disconnect();
        let _ = s.command("DLTLIB LIB(LEFTOVER)", CMD);
        Ok(format!(
            "PCML written; host table to /tmp/hosts and QS12HOST merged back; {dump} in a save file; IMPORT(*YES) checked; FTP client display signed on, PWD, QUIT"
        ))
    }
}
