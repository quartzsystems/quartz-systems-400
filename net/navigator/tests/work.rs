// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management: the pages against the golden pages, and what their
//! actions do. Golden corrections are listed in docs/NAVIGATOR.md.

mod common;

use common::{Browser, assert_golden};
use navigator::env::Audit;

const ADMIN: &str = "/job/123456/QTMHHTTP/ADMIN";

/// A page's text, which must be served with 200.
fn ok(b: &mut Browser, route: &str) -> String {
    let r = b.get(route);
    assert_eq!(r.status, 200, "{route}: {}", r.text());
    r.text()
}

/// Close the tree's Work management group, as a user who did.
fn close_group(b: &mut Browser) {
    b.get("/nav?close=Work%20management&back=/qs400/navigator/jobs");
}

fn ran(b: &Browser) -> Vec<String> {
    b.fx.work.ran.lock().unwrap().clone()
}

#[test]
fn active_jobs() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/jobs"), "nav/active-jobs.html", &[]);
    // The note about the ended job shows once.
    assert!(!ok(&mut b, "/jobs").contains("CPF1321"));
}

#[test]
fn active_jobs_empty() {
    let mut b = Browser::signed_on();
    ok(&mut b, "/jobs");
    close_group(&mut b);
    b.fx.work.state.lock().unwrap().jobs.clear();
    assert_golden(&ok(&mut b, "/jobs"), "nav/active-jobs-empty.html", &[]);
}

#[test]
fn active_jobs_error() {
    let mut b = Browser::signed_on();
    ok(&mut b, "/jobs");
    close_group(&mut b);
    b.fx.work.state.lock().unwrap().jobs.truncate(12);
    let r = b.post("/job/123460/QSECOFR/QPADEV0001/hold", &[("ok", "OK")]);
    assert_eq!(r.header("Location"), Some("/qs400/navigator/jobs"));
    assert_golden(&ok(&mut b, "/jobs"), "nav/active-jobs-error.html", &[]);
}

#[test]
fn filters_sort_and_pages() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/jobs?sts=MSGW");
    assert!(
        t.contains("QPADEV0001") && t.contains("INVBATCH") && t.contains("2 entries"),
        "{t}"
    );
    assert!(t.contains("<option selected>MSGW</option>"));
    let t = ok(&mut b, "/jobs?sbs=QBATCH&user=QPGM*");
    assert!(t.contains("NIGHTLY") && !t.contains("INVBATCH"));
    assert!(
        t.contains("href=\"?sbs=QBATCH&amp;user=QPGM*&amp;sort=name\""),
        "sort links keep the filter"
    );
    let t = ok(&mut b, "/jobs?sort=cpu");
    let first = t.find("NIGHTLY").unwrap();
    assert!(first < t.find("RGZPFM01").unwrap());
    let t = ok(&mut b, "/jobs?page=13");
    assert!(t.contains("Page 13 of 13") && t.contains("318 entries"));
    // The home page's links land on the list.
    assert!(ok(&mut b, "/home").contains("/qs400/navigator/jobs?sts=MSGW"));
}

#[test]
fn end_job_confirm() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/job/123503/CWELLMAN/INVBATCH/end");
    assert_golden(&t, "nav/end-job-confirm.html", &[]);
    // A GET changes nothing; the POST runs the command its fields make.
    assert!(ran(&b).is_empty());
    let r = b.post(
        "/job/123503/CWELLMAN/INVBATCH/end",
        &[
            ("opt", "*IMMED"),
            ("delay", "30"),
            ("spl", "*YES"),
            ("log", "*SAME"),
            ("adl", "*NONE"),
            ("ok", "OK"),
        ],
    );
    assert_eq!(r.header("Location"), Some("/qs400/navigator/jobs"));
    assert_eq!(
        ran(&b),
        ["ENDJOB JOB(123503/CWELLMAN/INVBATCH) OPTION(*IMMED) SPLFILE(*YES)"]
    );
    let t = ok(&mut b, "/jobs");
    assert!(t.contains("CPC1125") && t.contains("317"), "{t}");
    // Cancel runs nothing.
    b.post("/job/123501/QPGMR/NIGHTLY/end", &[("cancel", "Cancel")]);
    assert_eq!(ran(&b).len(), 1);
}

#[test]
fn hold_and_release() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/job/123501/QPGMR/NIGHTLY/hold");
    assert!(t.contains("Confirm Hold Job") && t.contains("HLDJOB JOB(123501/QPGMR/NIGHTLY)"));
    b.post("/job/123501/QPGMR/NIGHTLY/hold", &[("ok", "OK")]);
    let t = ok(&mut b, "/jobs?job=NIGHTLY");
    assert!(
        t.contains("CPC1117") && t.contains("<span class=\"err\">HLD</span>"),
        "{t}"
    );
    b.post("/job/123501/QPGMR/NIGHTLY/release", &[("ok", "OK")]);
    assert!(ok(&mut b, "/jobs?job=NIGHTLY").contains("CPC1118"));
}

#[test]
fn another_users_job_needs_jobctl() {
    let mut b = Browser::as_user("WEBUSER");
    let r = b.post("/job/123501/QPGMR/NIGHTLY/hold", &[("ok", "OK")]);
    assert_eq!(r.status, 403);
    assert!(r.text().contains("CPF2207") && r.text().contains("*JOBCTL"));
    assert!(ran(&b).is_empty());
    assert!(
        b.fx.system
            .audit
            .lock()
            .unwrap()
            .iter()
            .any(|a| matches!(a, Audit::Refused { need, .. } if need == "*JOBCTL"))
    );
}

#[test]
fn job_props() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, &format!("{ADMIN}/general")),
        "nav/job-props-general.html",
        &[],
    );
    assert_golden(
        &ok(&mut b, &format!("{ADMIN}/performance")),
        "nav/job-props-performance.html",
        &[],
    );
    assert_golden(
        &ok(&mut b, &format!("{ADMIN}/liblist")),
        "nav/job-props-liblist.html",
        &[(
            "<input type=\"submit\" value=\"Add library...\"><input type=\"submit\" value=\"Remove\"><input type=\"submit\" value=\"Change current library...\">",
            "<input type=\"submit\" value=\"Add library...\" disabled><input type=\"submit\" value=\"Remove\" disabled><input type=\"submit\" value=\"Change current library...\" disabled>",
        )],
    );
    assert_golden(
        &ok(&mut b, &format!("{ADMIN}/joblog")),
        "nav/job-props-joblog.html",
        &[("7 messages.", "6 messages.")],
    );
    assert_golden(
        &ok(&mut b, &format!("{ADMIN}/locks")),
        "nav/job-props-locks.html",
        &[],
    );
    assert_golden(
        &ok(&mut b, &format!("{ADMIN}/callstack")),
        "nav/job-props-callstack.html",
        &[
            (
                "<p class=\"small\">Thread:",
                "<form method=\"get\"><p class=\"small\">Thread:",
            ),
            ("value=\"Go\"></p>", "value=\"Go\"></p></form>"),
        ],
    );
    let t = ok(&mut b, &format!("{ADMIN}/callstack?thr=00000012"));
    assert!(t.contains("worker") && !t.contains("serve_loop"));
    let t = ok(&mut b, &format!("{ADMIN}/joblog?lvl=Severity+20+and+above"));
    assert!(t.contains("HTP8110") && !t.contains("HTP8001") && t.contains("1 message."));
    assert!(ok(&mut b, &format!("{ADMIN}/joblog?dsp=1")).contains("Cause . . . . . :"));
    assert!(ok(&mut b, &format!("{ADMIN}/splf")).contains("123456/QTMHHTTP/ADMIN"));
    assert_eq!(b.get("/job/1/X/Y/general").status, 404);
}

#[test]
fn change_job() {
    let mut b = Browser::signed_on();
    let r = b.post(
        &format!("{ADMIN}/general"),
        &[
            ("pty", "30"),
            ("ts", "2000"),
            ("lvl", "4 00 *SECLVL"),
            ("logcl", "on"),
            ("inq", "*DFT"),
            ("apply", "Apply"),
        ],
    );
    assert_eq!(
        r.header("Location"),
        Some("/qs400/navigator/job/123456/QTMHHTTP/ADMIN/general")
    );
    assert_eq!(
        ran(&b),
        ["CHGJOB JOB(123456/QTMHHTTP/ADMIN) RUNPTY(30) INQMSGRPY(*DFT)"]
    );
    let t = ok(&mut b, &format!("{ADMIN}/general"));
    assert!(
        t.contains("CPC1128")
            && t.contains("value=\"30\"")
            && t.contains("<option selected>*DFT</option>")
    );
    // Nothing changed: nothing runs.
    b.post(
        &format!("{ADMIN}/general"),
        &[
            ("pty", "30"),
            ("ts", "2000"),
            ("lvl", "4 00 *SECLVL"),
            ("logcl", "on"),
            ("inq", "*DFT"),
            ("ok", "OK"),
        ],
    );
    assert_eq!(ran(&b).len(), 1);
}

#[test]
fn job_queues() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/jobq"), "nav/job-queues.html", &[]);
    assert!(ok(&mut b, "/jobq?s=Held").contains("1 entry"));
    let t = ok(&mut b, "/jobq/QGPL/QBATCH/confirm");
    assert!(t.contains("Confirm Clear Job Queue") && t.contains("CLRJOBQ JOBQ(QGPL/QBATCH)"));
    b.post("/jobq/QGPL/QBATCH/confirm", &[("ok", "OK")]);
    b.post("/jobq/QGPL/QPGMR/release", &[("ok", "OK")]);
    assert_eq!(
        ran(&b),
        ["CLRJOBQ JOBQ(QGPL/QBATCH)", "RLSJOBQ JOBQ(QGPL/QPGMR)"]
    );
    let t = ok(&mut b, "/jobq?s=Held");
    assert!(t.contains("CPC3421") && t.contains("0 entries"));
    // WEBUSER lacks *JOBCTL.
    let mut w = Browser::as_user("WEBUSER");
    assert_eq!(
        w.post("/jobq/QGPL/QBATCH/hold", &[("ok", "OK")]).status,
        403
    );
}

#[test]
fn subsystems() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/sbs"),
        "nav/subsystems.html",
        &[(
            "<input type=\"submit\" value=\"Start subsystem...\"><input type=\"submit\" value=\"Refresh\">",
            "<input type=\"submit\" name=\"start\" value=\"Start subsystem...\"><input type=\"submit\" name=\"refresh\" value=\"Refresh\">",
        )],
    );
    close_group(&mut b);
    assert_golden(
        &ok(&mut b, "/sbs/QHTTPSVR/general"),
        "nav/subsystem-props.html",
        &[],
    );
    assert_golden(
        &ok(&mut b, "/sbs/QHTTPSVR/pools"),
        "nav/subsystem-props-pools.html",
        &[(
            "<input type=\"submit\" value=\"Add pool...\"><input type=\"submit\" value=\"Change...\"><input type=\"submit\" value=\"Remove\">",
            "<input type=\"submit\" name=\"add\" value=\"Add pool...\"><input type=\"submit\" name=\"change\" value=\"Change...\"><input type=\"submit\" name=\"remove\" value=\"Remove\">",
        )],
    );
    assert_golden(
        &ok(&mut b, "/sbs/QHTTPSVR/routing"),
        "nav/subsystem-props-routing.html",
        &[],
    );
    assert_golden(
        &ok(&mut b, "/sbs/QHTTPSVR/autostart"),
        "nav/subsystem-props-autostart.html",
        &[],
    );
    assert!(ok(&mut b, "/sbs/QINTER/workstation").contains("QPADEV*"));
    assert!(ok(&mut b, "/sbs/QUSRWRK/prestart").contains("QZDASOINIT"));
}

#[test]
fn subsystem_commands() {
    let mut b = Browser::signed_on();
    let t = ok(&mut b, "/sbs/QBATCH/end");
    assert!(
        t.contains("Confirm End Subsystem")
            && t.contains("ENDSBS SBS(QBATCH) OPTION(*CNTRLD) DELAY(*NOLIMIT)")
    );
    b.post("/sbs/QBATCH/end", &[("opt", "*IMMED"), ("ok", "OK")]);
    b.post("/sbs/QPGMR/start", &[("ok", "OK")]);
    b.post(
        "/sbs/QHTTPSVR/general",
        &[
            ("max", "*NOMAX"),
            ("dsp", "QSYS/QDSIGNON"),
            ("txt", "Web servers"),
            ("apply", "Apply"),
        ],
    );
    b.post(
        "/sbs/QHTTPSVR/routing",
        &[
            ("op", "add"),
            ("seq", "20"),
            ("cmp", "WEB"),
            ("start", "1"),
            ("pgm", "QSYS/QCMD"),
            ("cls", "QGPL/QBATCH"),
            ("max", ""),
            ("pool", "1"),
        ],
    );
    b.post("/sbs/QHTTPSVR/routing/confirm?seq=9999", &[("ok", "OK")]);
    b.post(
        "/sbs/QHTTPSVR/autostart",
        &[
            ("op", "add"),
            ("job", "EXTRA"),
            ("jobd", "QHTTPSVR/QZHBHTTP"),
        ],
    );
    b.post(
        "/sbs/QHTTPSVR/autostart/confirm?job=APACHEDFT",
        &[("ok", "OK")],
    );
    b.post(
        "/sbs/QHTTPSVR/pools",
        &[
            ("id", "2"),
            ("storage", "*SHRPOOL1"),
            ("act", "10"),
            ("apply", "Apply"),
        ],
    );
    assert_eq!(
        ran(&b),
        [
            "ENDSBS SBS(QBATCH) OPTION(*IMMED)",
            "STRSBS SBSD(QSYS/QPGMR)",
            "CHGSBSD SBSD(QHTTPSVR/QHTTPSVR) TEXT('Web servers')",
            "ADDRTGE SBSD(QHTTPSVR/QHTTPSVR) SEQNBR(20) CMPVAL('WEB' 1) PGM(QSYS/QCMD) CLS(QGPL/QBATCH) MAXACT(*NOMAX) POOLID(1)",
            "RMVRTGE SBSD(QHTTPSVR/QHTTPSVR) SEQNBR(9999)",
            "ADDAJE SBSD(QHTTPSVR/QHTTPSVR) JOB(EXTRA) JOBD(QHTTPSVR/QZHBHTTP)",
            "RMVAJE SBSD(QHTTPSVR/QHTTPSVR) JOB(APACHEDFT)",
            "CHGSBSD SBSD(QHTTPSVR/QHTTPSVR) POOLS((2 *SHRPOOL1 10))",
        ]
    );
    let t = ok(&mut b, "/sbs");
    assert!(t.contains("<a href=\"/qs400/navigator/sbs/QBATCH/start\">Start</a>"));
    let t = ok(&mut b, "/sbs/QHTTPSVR/routing");
    assert!(t.contains("WEB") && !t.contains("9999"));
    assert!(ok(&mut b, "/sbs/QHTTPSVR/pools").contains("*SHRPOOL1"));
    // The forms behind Add... and Change.
    assert_eq!(
        b.post("/sbs/QHTTPSVR/routing", &[]).header("Location"),
        Some("/qs400/navigator/sbs/QHTTPSVR/routing?seq=new")
    );
    assert!(ok(&mut b, "/sbs/QHTTPSVR/routing?seq=10").contains("Change routing entry"));
    assert!(ok(&mut b, "/sbs/QHTTPSVR/pools?edit=remove").contains("Remove pool"));
    assert!(ok(&mut b, "/sbs?start=1").contains("name=\"sbsd\""));
}

#[test]
fn output_queues_and_spooled_files() {
    let mut b = Browser::signed_on();
    assert_golden(&ok(&mut b, "/outq"), "nav/output-queues.html", &[]);
    close_group(&mut b);
    assert_golden(
        &ok(&mut b, "/outq/QGPL/QPRINT"),
        "nav/spooled-files.html",
        &[],
    );
    let t = ok(&mut b, "/outq/QGPL/QPRINT?s=HLD");
    assert!(
        t.contains("Page 1 of 1") && !t.contains("<td class=\"mono\">RDY</td>"),
        "only held files"
    );
    // Delete through the confirmation.
    let f = "/splf/123401/CWELLMAN/INVBATCH/QSYSPRT/401";
    assert!(ok(&mut b, &format!("{f}/confirm")).contains("Confirm Delete Spooled File"));
    b.post(&format!("{f}/confirm"), &[("ok", "OK")]);
    b.post(
        "/splf/123400/QPGMR/NIGHTLY/QPJOBLOG/400/move",
        &[("outq", "QGPL/PRT02"), ("ok", "OK")],
    );
    b.post("/outq/QGPL/PRT02/release", &[("ok", "OK")]);
    assert_eq!(
        ran(&b),
        [
            "DLTSPLF FILE(QSYSPRT) JOB(123401/CWELLMAN/INVBATCH) SPLNBR(401)",
            "CHGSPLFA FILE(QPJOBLOG) JOB(123400/QPGMR/NIGHTLY) SPLNBR(400) OUTQ(QGPL/PRT02)",
            "RLSOUTQ OUTQ(QGPL/PRT02)",
        ]
    );
    assert!(ok(&mut b, "/outq/QGPL/QPRINT").contains("146 entries"));
    assert!(ok(&mut b, "/outq/QGPL/QPRINT/props").contains("*FIFO"));
}

#[test]
fn spooled_view() {
    let mut b = Browser::signed_on();
    let v = "/splf/123402/CWELLMAN/INVBATCH/INVRPT/402";
    assert_golden(
        &ok(&mut b, v),
        "nav/spooled-view.html",
        &[
            (
                "<a href=\"#\">&lt; Previous</a>",
                "<span class=\"grey\">&lt; Previous</span>",
            ),
            (
                "<a href=\"#\">Next &gt;</a>",
                "<a href=\"spooled-view.html\">Next &gt;</a>",
            ),
            (
                "<a href=\"#\">View as PDF</a>",
                "<a href=\"spooled-view.html\">View as PDF</a>",
            ),
            (
                "<a href=\"#\">Download text</a>",
                "<a href=\"spooled-view.html\">Download text</a>",
            ),
            (
                "<input type=\"submit\" value=\"Hold\"> <input type=\"submit\" value=\"Release\"> <input type=\"submit\" value=\"Delete\">",
                "<a href=\"spooled-files.html\">Hold</a> &nbsp; <a href=\"spooled-files.html\">Release</a> &nbsp; <a href=\"../shared/confirm.html\">Delete</a>",
            ),
        ],
    );
    let t = ok(&mut b, &format!("{v}?p=3"));
    assert!(t.contains("PAGE    3") && t.contains("value=\"3\""));
    let t = ok(&mut b, &format!("{v}?f=WATCHGUARD&p=2"));
    assert!(
        t.contains("value=\"1\"") && t.contains("BRANCH EDGE"),
        "Find wraps to page 1"
    );
    assert!(ok(&mut b, &format!("{v}?f=NOWHERE")).contains("CPI9801"));
    let r = b.get(&format!("{v}/pdf"));
    assert_eq!(
        (r.status, r.header("Content-Type")),
        (200, Some("application/pdf"))
    );
    assert!(r.bytes().starts_with(b"%PDF"));
    let r = b.get(&format!("{v}/text"));
    assert!(r.text().starts_with("QPRINT    INVRPT"));
    // 132 columns exactly as spooled.
    let page1 = navigator::fixture::fixture()
        .env
        .work
        .spooled_text("QSECOFR", "123402/CWELLMAN/INVBATCH", "INVRPT", 402)
        .unwrap()
        .1;
    assert!(ok(&mut b, v).contains(&qsweb::html::esc(&page1[0])));
}

#[test]
fn printers() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/writers"),
        "nav/printers.html",
        &[(
            "<input type=\"submit\" value=\"Start writer...\"><input type=\"submit\" value=\"Refresh\">",
            "<input type=\"submit\" name=\"start\" value=\"Start writer...\"><input type=\"submit\" name=\"refresh\" value=\"Refresh\">",
        )],
    );
    assert!(
        ok(&mut b, "/writers/PRT01/confirm?op=end").contains("ENDWTR WTR(PRT01) OPTION(*CNTRLD)")
    );
    b.post(
        "/writers/PRT01/confirm?op=end",
        &[("opt", "*IMMED"), ("ok", "OK")],
    );
    b.post("/writers/PRT01/confirm?op=varyoff", &[("ok", "OK")]);
    b.post("/writers/PRT03/varyon", &[("ok", "OK")]);
    b.post(
        "/writers/PRT03/start",
        &[("outq", "QGPL/QPRINT"), ("form", "*ALL"), ("ok", "OK")],
    );
    b.post("/writers/RMTLASER/hold", &[("ok", "OK")]);
    assert_eq!(
        ran(&b),
        [
            "ENDWTR WTR(PRT01) OPTION(*IMMED)",
            "VRYCFG CFGOBJ(PRT01) CFGTYPE(*DEV) STATUS(*OFF)",
            "VRYCFG CFGOBJ(PRT03) CFGTYPE(*DEV) STATUS(*ON)",
            "STRPRTWTR DEV(PRT03) OUTQ(QGPL/QPRINT)",
            "HLDWTR WTR(RMTLASER)",
        ]
    );
    let t = ok(&mut b, "/writers");
    assert!(t.contains("CPC3307"));
    assert!(ok(&mut b, "/writers/PRT03").contains("QGPL/QPRINT"));
}

#[test]
fn scheduled_jobs() {
    let mut b = Browser::signed_on();
    assert_golden(
        &ok(&mut b, "/jobscde"),
        "nav/scheduled-jobs.html",
        &[(
            "<input type=\"submit\" value=\"Add...\"><input type=\"submit\" value=\"Refresh\">",
            "<input type=\"submit\" name=\"add\" value=\"Add...\"><input type=\"submit\" name=\"refresh\" value=\"Refresh\">",
        )],
    );
    assert_eq!(
        b.post("/jobscde", &[("add", "Add...")]).header("Location"),
        Some("/qs400/navigator/jobscde/add")
    );
    b.post(
        "/jobscde/add",
        &[
            ("job", "WEEKRPT"),
            ("cmd", "CALL PGM(HLINV/WEEKRPT)"),
            ("frq", "*WEEKLY"),
            ("date", ""),
            ("days", "*FRI"),
            ("time", "18:30"),
            ("jobq", "QGPL/QBATCH"),
            ("text", "Weekly report"),
            ("ok", "OK"),
        ],
    );
    b.post("/jobscde/NIGHTLY/1/hold", &[("ok", "OK")]);
    b.post("/jobscde/SAVDAILY/2/submit", &[("ok", "OK")]);
    b.post("/jobscde/CLNUPJOBLOG/7/confirm", &[("ok", "OK")]);
    assert_eq!(
        ran(&b),
        [
            "ADDJOBSCDE JOB(WEEKRPT) CMD(CALL PGM(HLINV/WEEKRPT)) FRQ(*WEEKLY) SCDDATE(*NONE) SCDDAY(*FRI) SCDTIME('183000') JOBQ(QGPL/QBATCH) TEXT('Weekly report')",
            "HLDJOBSCDE JOB(NIGHTLY) ENTRYNBR(1)",
            "SBMJOB CMD(SAVLIB LIB(*ALLUSR) DEV(TAP01) SAVACT(*LIB)) JOB(SAVDAILY) JOBQ(QGPL/QBATCH)",
            "RMVJOBSCDE JOB(CLNUPJOBLOG) ENTRYNBR(7)",
        ]
    );
    let t = ok(&mut b, "/jobscde");
    assert!(
        t.contains("WEEKRPT") && t.contains("*FRI 18:30") && !t.contains("jobscde/CLNUPJOBLOG"),
        "{t}"
    );
    assert!(ok(&mut b, "/jobscde/NIGHTLY/1/change").contains("CALL PGM(QGPL/NIGHTLY)"));
}

/// Every dangerous action's GET only shows its confirmation.
#[test]
fn gets_change_nothing() {
    let mut b = Browser::signed_on();
    for route in [
        "/job/123503/CWELLMAN/INVBATCH/end",
        "/job/123503/CWELLMAN/INVBATCH/hold",
        "/jobq/QGPL/QBATCH/confirm",
        "/sbs/QBATCH/end",
        "/outq/QGPL/QPRINT/confirm",
        "/splf/123400/QPGMR/NIGHTLY/QPJOBLOG/400/confirm",
        "/writers/PRT01/confirm?op=end",
        "/writers/PRT01/confirm?op=varyoff",
        "/jobscde/NIGHTLY/1/confirm",
    ] {
        ok(&mut b, route);
    }
    assert!(ran(&b).is_empty(), "{:?}", ran(&b));
}
