// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 85: display messages over TN5250. `GO USER` option 2 shows the
//! signed-on user's own queue, or `QSYSOPR` when the profile names it;
//! `DSPMSG QSYSOPR` and `DSPMSG MSGQ(*WRKUSR)` name their queues; an
//! inquiry another job sent is answered in its reply field.

use tn5250_driver::script::Context;
use tn5250_driver::session::Session as Display;
use tn5250_driver::{Connect, ScriptOptions};

use crate::phase8::{connect, run};
use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

/// The step's users: one with its own queue, one whose `MSGQ` is
/// `QSYSOPR`.
const OWN: &str = "MSGOWN85";
const OPR: &str = "MSGOPR85";
const PASSWORD: &str = "MSGS85PW";

/// The screen as text, one row a line.
fn screen(d: &Display) -> String {
    let mut out = String::new();
    for row in d.term.text_rows() {
        out.push('|');
        out.push_str(&row);
        out.push_str("|\n");
    }
    out
}

/// The queue and library in Display Messages' header.
fn header(d: &Display) -> Option<(String, String)> {
    let rows = d.term.text_rows();
    let value = |r: &str, label: &str| {
        r.split_once(label)
            .map(|(_, v)| v.split_whitespace().next().unwrap_or("").to_string())
    };
    let queue = rows.iter().find_map(|r| value(r, "Queue . . . . . :"))?;
    let lib = rows.iter().find_map(|r| value(r, "Library . . . :"))?;
    Some((queue, lib))
}

/// Sign on as `user` and reach the main menu.
fn sign_on(port: u16, user: &str) -> Result<(Display, Context), String> {
    let c = Connect::qs400(port, user, PASSWORD);
    let options = ScriptOptions {
        golden: None,
        update: false,
        release: String::new(),
        timeout_ms: 20000,
    };
    let mut ctx = Context::new(&c, &options);
    let mut d = connect(port, user, PASSWORD)?;
    run(&mut d, &mut ctx, "wait \"Sign On\"\nsignon\nsettle 1500\n")?;
    if d.term.contains("Sign-on Information") {
        run(&mut d, &mut ctx, "key enter\nsettle 1000\n")?;
    }
    run(&mut d, &mut ctx, "wait \"Main Menu\"\n")?;
    Ok((d, ctx))
}

/// `GO USER` through the main menu's option 1, then its option 2.
fn user_option_2(d: &mut Display, ctx: &mut Context) -> Result<(), String> {
    run(d, ctx, "type 1\nkey enter\nwait \"User Tasks\"\ntype 2\nkey enter\nsettle 2000\n")
}

/// Expect the messages of `queue` in `lib`: Display Messages' header,
/// or Work with Messages' (QSYSOPR's is named System Operator there).
fn expect_queue(d: &Display, what: &str, queue: &str, lib: &str) -> Result<(), String> {
    let basic = if queue == "QSYSOPR" { "System Operator" } else { queue };
    match header(d) {
        Some((q, l)) if q == queue && l == lib => Ok(()),
        None if d.term.contains("Work with Messages")
            && d.term.contains(&format!("Messages for:   {basic}")) =>
        {
            Ok(())
        }
        _ => Err(format!("{what}: the messages of {lib}/{queue} expected:\n{}", screen(d))),
    }
}

/// Leave with F3 until `text` shows.
fn back_to(d: &mut Display, ctx: &mut Context, text: &str) -> Result<(), String> {
    for _ in 0..4 {
        if d.term.contains(text) {
            return Ok(());
        }
        run(d, ctx, "key f3\nsettle 800\n")?;
    }
    Err(format!("F3 did not lead back to {text}:\n{}", screen(d)))
}

impl Suite {
    /// Step 85: DSPMSG's queues and an inquiry answered in its field.
    pub(crate) fn display_messages(&self, s: &mut Session) -> Result<String, String> {
        let port = self.telnet_port().ok_or("the launch has no telnet port")?;
        for (user, extra) in [(OWN, ""), (OPR, " MSGQ(QSYS/QSYSOPR)")] {
            let out = s.command(
                &format!("crtusrprf USRPRF({user}) PASSWORD({PASSWORD}) PWDEXP(*NO){extra}"),
                CMD,
            )?;
            if !out.contains("created") && !out.contains("CPF2214") {
                return Err(format!("crtusrprf {user}: {out}"));
            }
        }

        // The user with its own queue: GO USER option 2 shows it.
        let (mut d, mut ctx) = sign_on(port, OWN)?;
        let sent = s.command(
            &format!(
                "sndmsg MSG('Step 85 asks: go on?') TOMSGQ(QUSRSYS/{OWN}) MSGTYPE(*INQ) RPYMSGQ(QSYS/QSYSOPR)"
            ),
            CMD,
        )?;
        if !sent.contains("Message sent") {
            return Err(format!("sndmsg: {sent}"));
        }
        user_option_2(&mut d, &mut ctx)?;
        expect_queue(&d, "GO USER option 2", OWN, "QUSRSYS")?;
        if !d.term.contains("Step 85 asks: go on?") {
            return Err(format!("the inquiry is not on the display:\n{}", screen(&d)));
        }
        // QASTLVL ships *BASIC: the reply field is the intermediate
        // level's, which the job then keeps (*PRV).
        back_to(&mut d, &mut ctx, "User Tasks")?;
        run(&mut d, &mut ctx, "type dspmsg astlvl(*intermed)\nkey enter\nsettle 2000\n")?;
        expect_queue(&d, "DSPMSG ASTLVL(*INTERMED)", OWN, "QUSRSYS")?;
        // Its reply field is on the row that says so, at column 20.
        let row = d
            .term
            .text_rows()
            .iter()
            .position(|r| r.trim_end().ends_with("Reply . . ."))
            .ok_or_else(|| format!("no reply field:\n{}", screen(&d)))?;
        run(
            &mut d,
            &mut ctx,
            &format!("field {} 20 Y\nkey enter\nsettle 2000\n", row + 1),
        )?;
        let after = screen(&d);
        run(&mut d, &mut ctx, "wait \"User Tasks\"\ntype 2\nkey enter\nsettle 2000\n")?;
        expect_queue(&d, "GO USER option 2 again", OWN, "QUSRSYS")?;
        let answered = d
            .term
            .text_rows()
            .iter()
            .any(|r| r.contains("Reply . . :") && r.trim_end().ends_with(" Y"));
        if !answered {
            return Err(format!(
                "the inquiry does not show its reply:\n{after}---\n{}",
                screen(&d)
            ));
        }
        back_to(&mut d, &mut ctx, "User Tasks")?;
        run(&mut d, &mut ctx, "type dspmsg qsysopr\nkey enter\nsettle 2000\n")?;
        expect_queue(&d, "DSPMSG QSYSOPR", "QSYSOPR", "QSYS")?;
        back_to(&mut d, &mut ctx, "User Tasks")?;
        run(&mut d, &mut ctx, "type dspmsg msgq(*wrkusr)\nkey enter\nsettle 2000\n")?;
        expect_queue(&d, "DSPMSG MSGQ(*WRKUSR)", OWN, "QUSRSYS")?;
        back_to(&mut d, &mut ctx, "User Tasks")?;
        run(&mut d, &mut ctx, "type dspmsg astlvl(*basic)\nkey enter\nsettle 2000\n")?;
        if !d.term.contains("Work with Messages") || !d.term.contains(&format!("Messages for:   {OWN}")) {
            return Err(format!("DSPMSG ASTLVL(*BASIC):\n{}", screen(&d)));
        }
        back_to(&mut d, &mut ctx, "User Tasks")?;
        run(&mut d, &mut ctx, "type signoff\nkey enter\nwait \"Sign On\"\n")?;
        d.disconnect();

        // The user whose profile names QSYSOPR.
        let (mut d, mut ctx) = sign_on(port, OPR)?;
        user_option_2(&mut d, &mut ctx)?;
        expect_queue(&d, "GO USER option 2 (MSGQ QSYSOPR)", "QSYSOPR", "QSYS")?;
        back_to(&mut d, &mut ctx, "User Tasks")?;
        run(&mut d, &mut ctx, "type signoff\nkey enter\nwait \"Sign On\"\n")?;
        d.disconnect();
        Ok(format!(
            "{OWN}: GO USER 2 shows QUSRSYS/{OWN} (basic, QASTLVL), the inquiry answered Y in its field (intermediate), kept by *PRV; DSPMSG QSYSOPR, *WRKUSR, *BASIC; {OPR}: GO USER 2 shows QSYS/QSYSOPR"
        ))
    }
}
