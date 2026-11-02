// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/jobscde`: the job schedule (`WRKJOBSCDE`): Add (`ADDJOBSCDE`),
//! Change (`CHGJOBSCDE`), Hold and Release (`HLDJOBSCDE`,
//! `RLSJOBSCDE`), Remove (`RMVJOBSCDE`, confirmed) and Submit now (the
//! entry's command through `SBMJOB`).

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::{Form, Response};
use qsweb::partials::{
    Button, Cell, Col, List, Prop, PropGroup, actions, buttons, form, input, input_cmd, list_table,
    props_table,
};

use super::{Confirm, confirm_page, confirmed, mono, need, page, pick, quote, run, strings, word};
use crate::env::work::ScheduleEntry;
use crate::env::{Need, Target};
use crate::routes::Rq;

/// Commands longer than this show cut at a word, with ` ...`.
const COMMAND_SHOWN: usize = 52;

/// Route `/jobscde/...`.
pub fn route(rq: &Rq<'_>, rest: &[&str]) -> Response {
    match rest {
        [] => list(rq),
        ["add"] => edit(rq, None),
        [name, n, op] => {
            let all = rq.env().work.schedule(rq.user());
            let Some(e) = all
                .into_iter()
                .find(|e| e.name == *name && e.entry.to_string() == *n)
            else {
                return rq.not_found(&format!("job schedule entry {name} number {n}"));
            };
            match *op {
                "change" => edit(rq, Some(&e)),
                "hold" | "release" | "submit" | "confirm" => act(rq, &e, op),
                _ => rq.not_found(&format!("job schedule entry page {op}")),
            }
        }
        _ => rq.not_found(&format!("jobscde/{}", rest.join("/"))),
    }
}

/// The command as the list shows it: cut at a blank within its first
/// `COMMAND_SHOWN` characters (not bytes: the text may not be ASCII).
fn short(cmd: &str) -> String {
    let Some((end, c)) = cmd.char_indices().nth(COMMAND_SHOWN) else {
        return cmd.to_string();
    };
    let cut = cmd[..end + c.len_utf8()].rfind(' ').unwrap_or(end);
    format!("{} ...", &cmd[..cut])
}

/// The schedule column: when the entry submits.
fn when(rq: &Rq<'_>, e: &ScheduleEntry) -> String {
    match e.frequency.as_str() {
        "*ONCE" => {
            let d = e.date.map(|d| rq.stamp(d).0).unwrap_or_default();
            format!("{d} {}", e.time)
        }
        "*MONTHLY" => match e.date {
            Some(d) => {
                let day = rq
                    .stamp(d)
                    .0
                    .split('/')
                    .nth(1)
                    .unwrap_or("1")
                    .trim_start_matches('0')
                    .to_string();
                format!("Day {day} {}", e.time)
            }
            None => format!("{} {}", e.days.join(" "), e.time),
        },
        _ => {
            if e.days.iter().any(|d| d == "*ALL") {
                format!("*ALL days {}", e.time)
            } else {
                format!("{} {}", e.days.join(" "), e.time)
            }
        }
    }
}

fn next(rq: &Rq<'_>, e: &ScheduleEntry) -> String {
    e.next
        .map(|t| {
            let (d, t) = rq.stamp(t);
            format!("{d} {}", t.get(..5).unwrap_or(&t))
        })
        .unwrap_or_default()
}

fn status(e: &ScheduleEntry) -> String {
    if e.held {
        word("HLD", Some(false))
    } else {
        esc("SCD")
    }
}

/// `/jobscde?sort=`; the buttons post here.
fn list(rq: &Rq<'_>) -> Response {
    if rq.post() {
        return rq.redirect(if rq.form().has("add") {
            "/jobscde/add"
        } else {
            "/jobscde"
        });
    }
    let mut all = rq.env().work.schedule(rq.user());
    match rq.query().str("sort") {
        "name" => all.sort_by(|a, b| a.name.cmp(&b.name)),
        "f" => all.sort_by(|a, b| a.frequency.cmp(&b.frequency)),
        "nx" => all.sort_by_key(|e| e.next.unwrap_or(u64::MAX)),
        "st" => all.sort_by_key(|e| e.held),
        _ => {}
    }
    let rows = all
        .iter()
        .map(|e| {
            let u = |t: &str| rq.ctx.url(&format!("/jobscde/{}/{}/{t}", e.name, e.entry));
            let mut acts = vec![(u("change"), "Change")];
            if e.held {
                acts.push((u("release"), "Release"));
            } else if e.frequency != "*ONCE" {
                acts.push((u("hold"), "Hold"));
            }
            acts.push((u("confirm"), "Remove"));
            if !e.held && e.frequency != "*ONCE" {
                acts.push((u("submit"), "Submit now"));
            }
            vec![
                Cell::act(actions(&acts)),
                Cell::mono(&e.name),
                Cell::mono(&short(&e.command)),
                Cell::mono(&e.frequency),
                Cell::text(&when(rq, e)),
                Cell::text(&next(rq, e)),
                Cell::mono(&e.jobq),
                Cell::mono(&e.user),
                Cell::mono_html(status(e)),
            ]
        })
        .collect();
    let list = List {
        cols: vec![
            Col::plain("Actions"),
            Col::sorted("Job name", "name"),
            Col::plain("Command"),
            Col::sorted("Frequency", "f"),
            Col::plain("Schedule"),
            Col::sorted("Next submit", "nx"),
            Col::plain("Job queue"),
            Col::plain("User"),
            Col::sorted("Status", "st"),
        ],
        rows,
        empty: "There are no job schedule entries.".into(),
        keep: Vec::new(),
    };
    let body = format!(
        "{}{}",
        list_table(&list),
        form(
            &rq.ctx,
            "",
            &buttons(&[
                Button::new("add", "Add..."),
                Button::new("refresh", "Refresh")
            ])
        )
    );
    page(
        rq,
        &["Scheduled jobs"],
        "Scheduled jobs",
        Some("Job schedule entries (WRKJOBSCDE)."),
        body,
        "jobscde",
        Some("/jobscde"),
    )
}

/// `ADDJOBSCDE` or `CHGJOBSCDE` from the form's fields.
fn command(e: Option<&ScheduleEntry>, f: &Form) -> String {
    let up = |k: &str| f.str(k).trim().to_ascii_uppercase();
    let mut c = match e {
        Some(e) => format!("CHGJOBSCDE JOB({}) ENTRYNBR({})", e.name, e.entry),
        None => format!("ADDJOBSCDE JOB({})", up("job")),
    };
    let _ = write!(c, " CMD({}) FRQ({})", f.str("cmd").trim(), up("frq"));
    let date = up("date");
    let _ = write!(
        c,
        " SCDDATE({})",
        if date.is_empty() { "*NONE" } else { &date }
    );
    let days = up("days");
    let _ = write!(
        c,
        " SCDDAY({})",
        if days.is_empty() { "*NONE" } else { &days }
    );
    let time = f.str("time").trim().replace(':', "");
    let _ = write!(
        c,
        " SCDTIME('{}')",
        format!("{time:0<6}").get(..6).unwrap_or("000000")
    );
    let jobq = up("jobq");
    if !jobq.is_empty() {
        let _ = write!(c, " JOBQ({jobq})");
    }
    let _ = write!(c, " TEXT({})", quote(f.str("text").trim()));
    c
}

/// `/jobscde/add` and `/jobscde/<name>/<n>/change`: the entry's form.
fn edit(rq: &Rq<'_>, e: Option<&ScheduleEntry>) -> Response {
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect("/jobscde");
        }
        if let Some(e) = e
            && let Err(r) = allowed(rq, e, "Change job schedule entry")
        {
            return r;
        }
        return run(rq, &command(e, &f), "/jobscde");
    }
    let blank = ScheduleEntry {
        frequency: "*WEEKLY".into(),
        days: vec!["*ALL".into()],
        time: "02:00".into(),
        jobq: "QGPL/QBATCH".into(),
        ..ScheduleEntry::default()
    };
    let v = e.unwrap_or(&blank);
    let name = match e {
        Some(e) => mono(&e.name),
        None => input("job", "", 10),
    };
    let date = v.date.map(|d| rq.stamp(d).0).unwrap_or_default();
    let rows = vec![
        Prop::new("Job name", name).required(),
        Prop::new("Command", input_cmd("cmd", &v.command, 60)).required(),
        Prop::new(
            "Frequency",
            pick(
                "frq",
                &strings(&["*ONCE", "*WEEKLY", "*MONTHLY"]),
                &v.frequency,
            ),
        ),
        Prop::new(
            "Schedule date",
            format!("{} MM/DD/YY, blank for none", input("date", &date, 8)),
        ),
        Prop::new(
            "Schedule days",
            format!(
                "{} *ALL or *MON *TUE ...",
                input("days", &v.days.join(" "), 30)
            ),
        ),
        Prop::new(
            "Schedule time",
            format!("{} HH:MM", input("time", &v.time, 5)),
        )
        .required(),
        Prop::new("Job queue", input_cmd("jobq", &v.jobq, 21)),
        Prop::new("Text", input("text", &v.text, 40)),
    ];
    let body = form(
        &rq.ctx,
        "",
        &format!(
            "{}{}",
            props_table(rq.ctx.product, &[PropGroup { title: None, rows }], true),
            buttons(&[Button::new("ok", "OK"), Button::new("cancel", "Cancel")])
        ),
    );
    let h1 = match e {
        Some(e) => format!("Change Job Schedule Entry: {}", e.name),
        None => "Add Job Schedule Entry".to_string(),
    };
    page(
        rq,
        &["Scheduled jobs", &h1],
        &h1,
        None,
        body,
        "jobscde",
        None,
    )
}

/// Another user's entry needs `*JOBCTL`.
fn allowed(rq: &Rq<'_>, e: &ScheduleEntry, function: &str) -> Result<(), Response> {
    let t = Target {
        shown: format!("{} {}", e.name, e.entry),
        name: e.name.clone(),
        lib: "QUSRSYS".into(),
        typ: "*JOBSCD".into(),
    };
    need(
        rq,
        e.user == rq.user(),
        Need::JobCtl,
        function,
        &t,
        &["Work management", "Scheduled jobs"],
    )
}

/// Hold, Release, Submit now or Remove (`confirm`).
fn act(rq: &Rq<'_>, e: &ScheduleEntry, op: &str) -> Response {
    let id = format!("JOB({}) ENTRYNBR({})", e.name, e.entry);
    let (what, command, sentence) = match op {
        "hold" => (
            "Hold Job Schedule Entry",
            format!("HLDJOBSCDE {id}"),
            "Press OK to hold the entries listed below; they submit nothing until released.",
        ),
        "release" => (
            "Release Job Schedule Entry",
            format!("RLSJOBSCDE {id}"),
            "Press OK to release the entries listed below.",
        ),
        "submit" => {
            let mut c = format!("SBMJOB CMD({}) JOB({}) JOBQ({})", e.command, e.name, e.jobq);
            if e.user != rq.user() {
                let _ = write!(c, " USER({})", e.user);
            }
            (
                "Submit Job Now",
                c,
                "Press OK to submit the entries' jobs now; their schedule does not change.",
            )
        }
        _ => (
            "Remove Job Schedule Entry",
            format!("RMVJOBSCDE {id}"),
            "A removed entry submits no more jobs. Press OK to remove the entries listed below, or Cancel to return without removing.",
        ),
    };
    if rq.post() {
        if let Err(r) = allowed(rq, e, what) {
            return r;
        }
        return confirmed(rq, &command, "/jobscde");
    }
    confirm_page(
        rq,
        &Confirm {
            what,
            crumb: &["Scheduled jobs"],
            sentence: sentence.into(),
            cols: &["Job name", "Command", "Schedule", "Next submit", "User"],
            rows: vec![vec![
                mono(&e.name),
                mono(&short(&e.command)),
                esc(&when(rq, e)),
                esc(&next(rq, e)),
                esc(&e.user),
            ]],
            options: String::new(),
            command,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_commands_cut_on_characters() {
        assert_eq!(short("DSPLIB QGPL"), "DSPLIB QGPL");
        let ascii = format!("SNDMSG MSG('{}') TOUSR(QSYSOPR)", "A".repeat(40));
        assert_eq!(short(&ascii), "SNDMSG ...");
        // Non-ASCII text across the cut point (byte 52 inside a character).
        let wide = format!("SNDMSG MSG('{}') TOUSR(QSYSOPR)", "é".repeat(45));
        assert_eq!(short(&wide), "SNDMSG ...");
        let exact = "é".repeat(COMMAND_SHOWN);
        assert_eq!(short(&exact), exact);
    }
}
