// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's printer output: the eight spooled files of
//! `web/printer-output.html`, one of `QPGMR`'s, the printers of
//! `web/printers.html`, and the first page of `INVRPT` as
//! `nav/spooled-view.html` shows it. Another user's files need
//! `*SPLCTL` or `*JOBCTL`, as the system's check does.

use std::fmt::Write as _;
use std::sync::Mutex;

use qsweb::msg::{Kind, Message};

use super::{PROFILES, text};
use crate::env::print::{Act, Print, Printer, Splf};
use crate::util::secs;

/// A sample spooled file: file, user, user data, status, pages,
/// copies, form type, output queue, created, job.
pub type Sample = [&'static str; 10];

/// The user's spooled files, newest first.
pub const SPOOLED: &[Sample] = &[
    ["INVRPT", "CWELLMAN", "INVBATCH", "RDY", "12", "1", "*STD", "QGPL/QPRINT", "10/29/26 06:14", "123402/CWELLMAN/INVBATCH"],
    ["QPJOBLOG", "CWELLMAN", "INVLOAD", "RDY", "3", "1", "*STD", "QUSRSYS/QEZJOBLOG", "10/29/26 06:15", "123402/CWELLMAN/INVBATCH"],
    ["QPRINT", "CWELLMAN", "", "HLD", "1", "1", "*STD", "QGPL/QPRINT", "10/28/26 18:00", "123311/CWELLMAN/QPADEV0003"],
    ["QSYSPRT", "CWELLMAN", "DSPJOB", "RDY", "7", "1", "*STD", "QGPL/QPRINT", "10/28/26 17:02", "123311/CWELLMAN/QPADEV0003"],
    ["INVLABEL", "CWELLMAN", "LABELS", "RDY", "118", "2", "*LABEL", "QGPL/PRT02", "10/27/26 09:40", "123290/CWELLMAN/INVBATCH"],
    ["QPDSPJOB", "CWELLMAN", "", "SAV", "2", "1", "*STD", "QGPL/QPRINT", "10/24/26 11:11", "123101/CWELLMAN/QPADEV0003"],
    ["QPJOBLOG", "CWELLMAN", "QPADEV0003", "MSGW", "1", "1", "*STD", "QUSRSYS/QEZJOBLOG", "10/22/26 09:50", "123050/CWELLMAN/QPADEV0003"],
    ["ASSETEXP", "CWELLMAN", "EXPORT", "WTR", "44", "1", "*STD", "HLINV/INVRPT", "10/20/26 15:00", "122990/CWELLMAN/INVBATCH"],
];

/// Their file numbers in their jobs.
const NUMBERS: [u32; 8] = [402, 403, 1, 2, 1, 1, 1, 1];

/// Another user's file, for `*ALL`.
pub const OTHER: Sample = ["QSYSPRT", "QPGMR", "MONTHEND", "RDY", "4", "1", "*STD", "QGPL/QPRINT", "10/26/26 23:30", "123200/QPGMR/MONTHEND"];

/// Page 1 of `INVRPT`.
pub const INVRPT_PAGE1: &str = r"QPRINT    INVRPT      INVENTORY REPORT BY LOCATION                       QSHQ400   10/29/26  06:14:02   PAGE    1
                                                                            HLINV/INVRPT      USER CWELLMAN

LOCATION  ASSET TAG   MANUFACTURER      MODEL                 SERIAL              TYPE        STATUS   ACQUIRED    NOTES
--------  ----------  ----------------  --------------------  ------------------  ----------  -------  ----------  ------------------------
QS-HQ-R1  QS000101    QUANTA            T4048-IX8             QTFCEV2340021       SWITCH      ACTIVE   2025-03-11  SPINE-1 SONIC
QS-HQ-R1  QS000102    QUANTA            T4048-IX8             QTFCEV2340022       SWITCH      ACTIVE   2025-03-11  SPINE-2 SONIC
QS-HQ-R1  QS000110    EDGECORE          AS5712-54X            571254X2041B0019    SWITCH      ACTIVE   2024-11-02  LEAF-1
QS-HQ-R1  QS000111    EDGECORE          AS5712-54X            571254X2041B0020    SWITCH      ACTIVE   2024-11-02  LEAF-2
QS-HQ-R2  QS000201    DELL              R740XD                7XK2Q33             SERVER      ACTIVE   2024-06-20  PVE-01 CEPH
QS-HQ-R2  QS000202    DELL              R740XD                7XK2Q34             SERVER      ACTIVE   2024-06-20  PVE-02 CEPH
QS-HQ-R2  QS000203    DELL              R740XD                7XK2Q35             SERVER      ACTIVE   2024-06-20  PVE-03 CEPH
QS-HQ-R2  QS000210    DELL              QLE2562 HBA           BFE1234567          ADAPTER     ACTIVE   2026-09-20  QS-HQ-VTL
QS-HQ-R3  QS000301    SUPERMICRO        SYS-5019D-FN8TP       S412345X9A01234     SERVER      ACTIVE   2023-02-14  LAB BUILD BOX
QS-HQ-R3  QS000302    WEIDMULLER        UR20-FBC-EIP          2024-11-00341       IO-COUPLER  ACTIVE   2026-10-01  LAB RACK
QS-BR1-R1 QS000401    DELL              S4048-ON              8H3K2P2             SWITCH      ACTIVE   2024-09-08  BRANCH CORE
QS-BR1-R1 QS000402    WATCHGUARD        M370                  80BA0F2B1C33        FIREWALL    ACTIVE   2024-09-08  BRANCH EDGE

                                                   TOTAL ASSETS THIS LOCATION GROUP:    12
";

/// A local time on the sample system, from `MM/DD/YY HH:MM` and seconds.
fn local(stamp: &str, s: u32) -> u64 {
    let n = |r: std::ops::Range<usize>| stamp.get(r).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    secs(2000 + i64::from(n(6..8)), n(0..2), n(3..5), n(9..11), n(12..14), s) + 4 * 3600
}

fn splf(s: &Sample, number: u32) -> Splf {
    let first = s[0] == "INVRPT";
    let pages: u32 = s[4].parse().unwrap_or(1);
    Splf {
        job: s[9].into(),
        file: s[0].into(),
        number,
        user: s[1].into(),
        user_data: s[2].into(),
        status: s[3].into(),
        pages,
        current_page: 0,
        copies: s[5].parse().unwrap_or(1),
        form_type: s[6].into(),
        outq: s[7].into(),
        created: local(s[8], if first { 2 } else { 0 }),
        priority: 5,
        device_type: "*SCS".into(),
        page_len: 66,
        page_width: 132,
        lpi: 6,
        cpi: 10,
        overflow: 60,
        hold: false,
        save: false,
        record_len: 133,
        records: pages * 66,
        printer: "*OUTQ".into(),
        writer: if first { "PRT01".into() } else { String::new() },
    }
}

/// Page `n` of `f` as text.
pub fn page_text(f: &Splf, n: u32) -> String {
    if f.file == "INVRPT" && f.number == 402 && n == 1 {
        return INVRPT_PAGE1.to_string();
    }
    let q = f.outq.rsplit('/').next().unwrap_or("QPRINT");
    let title = format!("{} OUTPUT", f.file);
    let mut s = format!("{q:<10}{:<12}{title:<55}QSHQ400   10/29/26  06:14:02   PAGE {n:>4}\n\n", f.file);
    for l in 1..=20 {
        let _ = writeln!(s, "  {:<8}  LINE {l:>3} OF PAGE {n:>3}  JOB {}", f.file, f.job);
    }
    s
}

/// The sample printer output.
pub struct FixturePrint {
    /// Every user's spooled files.
    pub files: Mutex<Vec<Splf>>,
    /// The actions run, as commands.
    pub ran: Mutex<Vec<String>>,
}

impl Default for FixturePrint {
    fn default() -> Self {
        let mut v: Vec<Splf> = SPOOLED.iter().zip(NUMBERS).map(|(s, n)| splf(s, n)).collect();
        v.push(splf(&OTHER, 1));
        Self {
            files: Mutex::new(v),
            ran: Mutex::new(Vec::new()),
        }
    }
}

fn msg(id: &str, data: &[&str]) -> Message {
    let t = text::second_level(id, data).map(|s| s.text).unwrap_or_default();
    Message::new(Kind::Error, id, &t).with_data(data)
}

/// Whether `user` may work with `owner`'s files.
fn may(user: &str, owner: &str) -> bool {
    user == owner
        || PROFILES
            .iter()
            .any(|p| p.name == user && p.special.iter().any(|s| *s == "*SPLCTL" || *s == "*JOBCTL" || *s == "*ALLOBJ"))
}

impl Print for FixturePrint {
    fn spooled(&self, user: &str, owner: Option<&str>) -> Result<Vec<Splf>, Message> {
        if let Some(o) = owner
            && !may(user, o)
        {
            return Err(msg("CPF3303", &[user, o]));
        }
        Ok(self
            .files
            .lock()
            .expect("splf")
            .iter()
            .filter(|f| owner.is_none_or(|o| f.user == o) && may(user, &f.user))
            .cloned()
            .collect())
    }

    fn users(&self, _user: &str) -> Vec<String> {
        vec!["CWELLMAN".into(), "QPGMR".into()]
    }

    fn queues(&self, _user: &str) -> Vec<String> {
        vec!["QGPL/QPRINT".into(), "QGPL/PRT02".into(), "QGPL/QPDF".into()]
    }

    fn find(&self, user: &str, id: &str) -> Result<Splf, Message> {
        let f = self
            .files
            .lock()
            .expect("splf")
            .iter()
            .find(|f| f.id() == id)
            .cloned()
            .ok_or_else(|| msg("CPF3309", &[id]))?;
        if !may(user, &f.user) {
            return Err(msg("CPF3303", &[user, &f.outq]));
        }
        Ok(f)
    }

    fn text(&self, user: &str, f: &Splf) -> Result<Vec<String>, Message> {
        let f = self.find(user, &f.id())?;
        Ok((1..=f.pages.max(1)).map(|n| page_text(&f, n)).collect())
    }

    fn pdf(&self, user: &str, f: &Splf) -> Result<Vec<u8>, Message> {
        let pages: Vec<spool::scs::Page> = self
            .text(user, f)?
            .iter()
            .map(|t| {
                let mut p = spool::scs::Page::default();
                for (i, l) in t.lines().enumerate() {
                    p.lines.insert(u16::try_from(i + 1).unwrap_or(u16::MAX), l.to_string());
                }
                p
            })
            .collect();
        Ok(spool::pdf::render(&pages, spool::scs::decode::Format::default()))
    }

    fn act(&self, user: &str, f: &Splf, act: &Act) -> Result<(), Message> {
        let f = self.find(user, &f.id())?;
        let cmd = match act {
            Act::Hold { page_end } => format!("HLDSPLF {} OPTION({})", f.parms(), if *page_end { "*PAGEEND" } else { "*IMMED" }),
            Act::Release => format!("RLSSPLF {}", f.parms()),
            Act::Delete => format!("DLTSPLF {}", f.parms()),
            Act::Move { outq, top } => format!("CHGSPLFA {} OUTQ({outq}){}", f.parms(), if *top { " top" } else { "" }),
        };
        self.ran.lock().expect("ran").push(cmd);
        let mut all = self.files.lock().expect("splf");
        let Some(i) = all.iter().position(|x| x.id() == f.id()) else {
            return Err(msg("CPF3309", &[&f.file]));
        };
        match act {
            Act::Hold { .. } => all[i].status = "HLD".into(),
            Act::Release => {
                if all[i].status != "HLD" && all[i].status != "SAV" {
                    return Err(msg("CPF3337", &[]));
                }
                all[i].status = "RDY".into();
            }
            Act::Delete => {
                all.remove(i);
            }
            Act::Move { outq, top } => {
                if all[i].status == "WTR" {
                    return Err(msg("CPF3330", &[&f.file]));
                }
                let mut moved = all.remove(i);
                moved.outq.clone_from(outq);
                if *top {
                    all.insert(0, moved);
                } else {
                    all.push(moved);
                }
            }
        }
        Ok(())
    }

    fn printers(&self, _user: &str) -> Vec<Printer> {
        let p = |name: &str, started: bool, outq: &str, location: &str, kind: &str| Printer {
            name: name.into(),
            started,
            outq: outq.into(),
            location: location.into(),
            kind: kind.into(),
        };
        vec![
            p("PRT01", true, "QGPL/QPRINT", "HQ server room", "Laser, 3812 emulation"),
            p("PRT02", false, "QGPL/PRT02", "Warehouse", "Label printer"),
            p("QPDFWTR", true, "QGPL/QPDF", "(PDF to /home/<user>/pdf)", "PDF"),
            p("RMTLASER", true, "QGPL/RMTLASER", "Branch QS-BR1", "Remote LPR"),
        ]
    }
}
