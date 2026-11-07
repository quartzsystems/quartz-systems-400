// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's commands: the two results, the recent commands
//! and `CWELLMAN`'s saved commands of `web/run-command.html` and
//! `web/my-commands.html`, and `WRKACTJOB`'s definition as
//! `web/command-prompt.html` prompts it.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qsweb::msg::{Kind, Message};

use crate::env::cmd::{Cmd, CmdDef, How, LogMessage, Outcome, Param, Ran, Saved};
use crate::util::secs;

/// Milliseconds of 10/29/26 at `h:m:s.ms` UTC.
fn at(h: u32, m: u32, s: u32, ms: u64) -> u64 {
    secs(2026, 10, 29, h, m, s) * 1000 + ms
}

fn log(severity: u8, id: &str, text: &str, program: &str, sent_ms: u64) -> LogMessage {
    LogMessage {
        severity,
        id: id.into(),
        text: text.into(),
        program: program.into(),
        sent_ms,
    }
}

/// The connection job the sample commands ran in.
const JOB: &str = "123533/QUSER/QAPCNN";

fn results() -> Vec<Ran> {
    vec![
        Ran {
            command: "CHGDEVDSP DEVD(QPADEV0003) TEXT('Cody - web 5250')".into(),
            job: JOB.into(),
            outcome: Outcome::Completed,
            messages: vec![
                log(0, "CPC2191", "Object QPADEV0003 in library QSYS type *DEVD changed.", "QDCCDEVD", at(18, 5, 12, 117)),
                log(
                    0,
                    "CPI2101",
                    "Job 123488/CWELLMAN/QPADEV0003 submitted to job queue QBATCH in library QGPL.",
                    "QWTPCRJA",
                    at(18, 5, 13, 4),
                ),
            ],
        },
        Ran {
            command: "CALL PGM(HLINV/INVLOAD) PARM('*ALL')".into(),
            job: JOB.into(),
            outcome: Outcome::Failed,
            messages: vec![
                log(40, "CPF9801", "Object INVLOAD in library HLINV not found.", "QLICKOBJ", at(18, 6, 40, 220)),
                log(40, "CPF0001", "Error found on CALL command.", "QCACHECK", at(18, 6, 40, 221)),
            ],
        },
    ]
}

const RECENT: &[&str] = &[
    "CHGDEVDSP DEVD(QPADEV0003) TEXT('Cody - web 5250')",
    "DSPJOB JOB(123503/CWELLMAN/INVBATCH) OPTION(*STSA)",
    "SBMJOB CMD(CALL PGM(HLINV/INVLOAD) PARM('*ALL')) JOB(INVBATCH)",
    "WRKSPLF SELECT(CWELLMAN)",
    "DSPMSG QSYSOPR",
];

fn saved() -> Vec<Saved> {
    let s = |name: &str, command: &str, how, last: u64, share: &str| Saved {
        name: name.into(),
        command: command.into(),
        how,
        last,
        share: share.into(),
    };
    vec![
        s("HTTP jobs", "WRKACTJOB SBS(QHTTPSVR)", How::Interactive, secs(2026, 10, 29, 18, 5, 0), ""),
        s(
            "Load inventory",
            "SBMJOB CMD(CALL PGM(HLINV/INVLOAD) PARM('*ALL')) JOB(INVBATCH) JOBQ(HLINV/INVLOAD)",
            How::Batch,
            secs(2026, 10, 29, 17, 58, 0),
            "WAREHOUSE",
        ),
        s("Operator messages", "DSPMSG MSGQ(QSYSOPR)", How::Interactive, secs(2026, 10, 29, 13, 0, 0), "*PUBLIC"),
        s(
            "Restart web store",
            "ENDTCPSVR SERVER(*HTTP) HTTPSVR(WEBSTORE) + STRTCPSVR SERVER(*HTTP) HTTPSVR(WEBSTORE)",
            How::Interactive,
            secs(2026, 10, 27, 22, 42, 0),
            "ADMINS",
        ),
        s("Check fixes", "CHKPTF UPDSVR(*QUARTZ)", How::Batch, secs(2026, 10, 26, 8, 0, 0), ""),
    ]
}

/// `WRKACTJOB` as the prototype prompts it.
fn wrkactjob() -> CmdDef {
    let p = |keyword: &str, prompt: &str, choices: &str, values: &[&str], default: Option<&str>, max: u16, size: u32| Param {
        keyword: keyword.into(),
        prompt: prompt.into(),
        choices: choices.into(),
        values: values.iter().map(ToString::to_string).collect(),
        default: default.map(ToString::to_string),
        max,
        size,
        required: false,
        additional: false,
    };
    CmdDef {
        name: "WRKACTJOB".into(),
        lib: "QSYS".into(),
        text: "Work with Active Jobs".into(),
        params: vec![
            p("OUTPUT", "Output", "*, *PRINT", &["*", "*PRINT"], Some("*"), 1, 6),
            p("RESET", "Reset status statistics", "*NO, *YES", &["*NO", "*YES"], Some("*NO"), 1, 4),
            p("SBS", "Subsystem", "Name, *ALL", &[], Some("*ALL"), 20, 10),
            p("CPUPCTLMT", "CPU percent limit", "0.0-99.9, *NONE", &[], Some("*NONE"), 1, 8),
            p("RSPLMT", "Response time limit", "0.0-999.9, *NONE", &[], Some("*NONE"), 1, 8),
            p(
                "SEQ",
                "Sequence",
                "",
                &["*SBS", "*CPU", "*JOB", "*USER", "*STS", "*TYPE", "*FUNCTION", "*IO"],
                Some("*SBS"),
                1,
                9,
            ),
            p("JOB", "Job name", "Name, generic*, *ALL", &[], Some("*ALL"), 1, 10),
            p("INTERVAL", "Interval (seconds)", "1-999 (batch only)", &[], None, 1, 4),
        ],
    }
}

/// The sample commands.
#[derive(Debug)]
pub struct FixtureCmd {
    /// Results by user.
    pub results: Mutex<BTreeMap<String, Vec<Ran>>>,
    /// Recent commands by user.
    pub recent: Mutex<BTreeMap<String, Vec<String>>>,
    /// Saved commands by user.
    pub saved: Mutex<BTreeMap<String, Vec<Saved>>>,
    /// The commands run, as `user: command`.
    pub ran: Mutex<Vec<String>>,
}

impl Default for FixtureCmd {
    fn default() -> Self {
        let me = super::USER.to_string();
        Self {
            results: Mutex::new(BTreeMap::from([(me.clone(), results())])),
            recent: Mutex::new(BTreeMap::from([(me.clone(), RECENT.iter().map(ToString::to_string).collect())])),
            saved: Mutex::new(BTreeMap::from([(me, saved())])),
            ran: Mutex::new(Vec::new()),
        }
    }
}

impl Cmd for FixtureCmd {
    fn run(&self, user: &str, command: &str, check: bool) -> Ran {
        self.ran.lock().expect("ran").push(format!("{user}: {command}"));
        let verb = command.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
        let now = super::now() * 1000;
        let (outcome, messages) = match verb.as_str() {
            "WRKACTJOB" | "DSPMSG" | "DSPJOB" | "WRKSPLF" | "CHGDEVDSP" | "CHKPTF" => {
                (if check { Outcome::Checked } else { Outcome::Completed }, Vec::new())
            }
            "SBMJOB" if !check => (
                Outcome::Completed,
                vec![log(0, "CPC1221", "Job 123540/CWELLMAN/QDFTJOBD submitted to job queue QBATCH in QGPL.", "QWTCCSBJ", now)],
            ),
            "SBMJOB" => (Outcome::Checked, Vec::new()),
            _ => (
                Outcome::NotFound,
                vec![log(30, "CPD0030", &format!("Command {verb} in library *LIBL not found."), "QCATRS", now)],
            ),
        };
        let ran = Ran {
            command: command.into(),
            job: format!("123463/{user}/ACCESSWEB"),
            outcome,
            messages,
        };
        self.results.lock().expect("results").entry(user.into()).or_default().push(ran.clone());
        ran
    }

    fn results(&self, user: &str) -> Vec<Ran> {
        self.results.lock().expect("results").get(user).cloned().unwrap_or_default()
    }

    fn clear(&self, user: &str) {
        self.results.lock().expect("results").remove(user);
    }

    fn cancel(&self, _user: &str) -> bool {
        false
    }

    fn job_queues(&self, _user: &str) -> Vec<String> {
        vec!["QGPL/QBATCH".into(), "QGPL/QPGMR".into()]
    }

    fn describe(&self, _user: &str, name: &str) -> Result<CmdDef, Message> {
        let n = name.rsplit('/').next().unwrap_or("").trim().to_ascii_uppercase();
        if n == "WRKACTJOB" {
            Ok(wrkactjob())
        } else {
            Err(Message::new(Kind::Error, "CPD0030", &format!("Command {n} in library *LIBL not found.")))
        }
    }

    fn recent(&self, user: &str) -> Vec<String> {
        self.recent.lock().expect("recent").get(user).cloned().unwrap_or_default()
    }

    fn put_recent(&self, user: &str, commands: &[String]) -> Result<(), Message> {
        self.recent.lock().expect("recent").insert(user.into(), commands.to_vec());
        Ok(())
    }

    fn saved(&self, user: &str) -> Vec<Saved> {
        self.saved.lock().expect("saved").get(user).cloned().unwrap_or_default()
    }

    fn put_saved(&self, user: &str, saved: &[Saved]) -> Result<(), Message> {
        self.saved.lock().expect("saved").insert(user.into(), saved.to_vec());
        Ok(())
    }

    fn shared_with(&self, user: &str, to: &[String]) -> Vec<(String, Saved)> {
        let all = self.saved.lock().expect("saved");
        all.iter()
            .filter(|(owner, _)| owner.as_str() != user)
            .flat_map(|(owner, v)| v.iter().filter(|s| to.contains(&s.share)).map(move |s| (owner.clone(), s.clone())))
            .collect()
    }
}
