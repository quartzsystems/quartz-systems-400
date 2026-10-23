// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The list displays Display Module Source opens: Work with Module List
//! (F14), Work with Watch (F18) and Work with Debugged Threads
//! (`THREAD`), measured on the pub400 goldens; Work with Module
//! Breakpoints (F13), which pub400 shows only with a source view, laid
//! out as the module list is; and the command window (F21).

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, cmdline, input, keys, leader, message, more, put, system, title};

/// The keys of the list displays with a command line, on `row`.
fn list_keys(s: &mut Screen, row: usize) {
    keys(
        s,
        &[
            (row, 2, "F3=Exit"),
            (row, 12, "F4=Prompt"),
            (row, 24, "F5=Refresh"),
            (row, 37, "F9=Retrieve"),
            (row, 51, "F12=Cancel"),
        ],
    );
}

/// One option field on an entry row.
fn opt(s: &mut Screen, row: usize, list: &str, index: usize) {
    input(
        s,
        row,
        3,
        1,
        Target::ListOption {
            list: list.into(),
            row_index: index,
        },
        "",
    );
}

/// A breakpoint entry: line and condition.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BkptRow {
    /// The line.
    pub line: usize,
    /// The condition, blank for none.
    pub condition: String,
    /// The thread, for a thread breakpoint.
    pub thread: Option<u64>,
}

/// The module a breakpoint list is for.
#[derive(Clone, Copy, Debug, Default)]
pub struct Header<'a> {
    /// The system name.
    pub sysname: &'a str,
    /// The program.
    pub program: &'a str,
    /// Its library.
    pub library: &'a str,
    /// The module.
    pub module: &'a str,
    /// `*PGM` or `*SRVPGM`.
    pub kind: &'a str,
}

/// Entries a breakpoint page shows (rows 11 to 18).
pub const BKPT_ROOM: usize = 8;

/// Work with Module Breakpoints: `1=Add 4=Clear`, a blank entry first
/// for adding (named fields `LINE` and `COND`), then one entry a
/// breakpoint from `first`.
#[must_use]
pub fn wrkmodbkp(
    h: &Header<'_>,
    rows: &[BkptRow],
    first: usize,
    cmd: &str,
    msg: Option<&str>,
) -> Screen {
    let mut s = blank();
    title(&mut s, "Work with Module Breakpoints");
    system(&mut s, h.sysname);
    put(&mut s, 3, 2, Attr::Normal, &leader("Program", 2, 21));
    put(&mut s, 3, 23, Attr::Normal, ":");
    put(&mut s, 3, 26, Attr::Normal, h.program);
    put(&mut s, 3, 40, Attr::Normal, &leader("Library", 40, 55));
    put(&mut s, 3, 57, Attr::Normal, ":");
    put(&mut s, 3, 60, Attr::Normal, h.library);
    put(&mut s, 4, 4, Attr::Normal, &leader("Module", 4, 21));
    put(&mut s, 4, 23, Attr::Normal, ":");
    put(&mut s, 4, 26, Attr::Normal, h.module);
    put(&mut s, 4, 40, Attr::Normal, &leader("Type", 40, 55));
    put(&mut s, 4, 57, Attr::Normal, ":");
    put(&mut s, 4, 60, Attr::Normal, h.kind);
    put(&mut s, 6, 2, Attr::Blue, "Type options, press enter.");
    keys(&mut s, &[(7, 4, "1=Add"), (7, 12, "4=Clear")]);
    put(&mut s, 9, 2, Attr::HighIntensity, "Opt");
    put(&mut s, 9, 10, Attr::HighIntensity, "Line");
    put(&mut s, 9, 19, Attr::HighIntensity, "Condition");
    opt(&mut s, 10, "BKPTS", usize::MAX);
    input(&mut s, 10, 10, 6, Target::Var("LINE".into()), "");
    input(&mut s, 10, 19, 60, Target::Var("COND".into()), "");
    for (i, r) in rows.iter().enumerate().skip(first).take(BKPT_ROOM) {
        let row = 11 + i - first;
        opt(&mut s, row, "BKPTS", i);
        put(&mut s, row, 10, Attr::Normal, &r.line.to_string());
        let cond = match r.thread {
            Some(t) => format!("{} (thread {t:08X})", r.condition)
                .trim()
                .to_string(),
            None => r.condition.clone(),
        };
        put(&mut s, row, 19, Attr::Normal, &cond);
    }
    more(&mut s, 19, first + BKPT_ROOM >= rows.len());
    cmdline(&mut s, 20, cmd);
    list_keys(&mut s, 22);
    message(&mut s, msg);
    s.cursor = Some((10, 3));
    s
}

/// One entry of the module list: a program, or one of its modules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModRow {
    /// Program or module name.
    pub name: String,
    /// The program's library (blank on a module).
    pub library: String,
    /// `*PGM`, `*SRVPGM` or `*MODULE`.
    pub kind: String,
    /// The module shown now.
    pub selected: bool,
}

/// Entries a module list page shows (rows 9 to 18).
pub const MOD_ROOM: usize = 10;

/// Work with Module List: `1=Add program 4=Remove program 5=Display
/// module source 8=Work with module breakpoints`, an entry row for
/// adding (`PGM`, `LIB`, `TYPE`), then programs with their modules
/// indented under them.
#[must_use]
pub fn wrkmodlst(
    sysname: &str,
    rows: &[ModRow],
    first: usize,
    cmd: &str,
    msg: Option<&str>,
) -> Screen {
    let mut s = blank();
    title(&mut s, "Work with Module List");
    system(&mut s, sysname);
    put(&mut s, 3, 2, Attr::Blue, "Type options, press enter.");
    keys(
        &mut s,
        &[
            (4, 4, "1=Add program"),
            (4, 20, "4=Remove program"),
            (4, 39, "5=Display module source"),
            (5, 4, "8=Work with module breakpoints"),
        ],
    );
    put(&mut s, 7, 2, Attr::HighIntensity, "Opt");
    put(&mut s, 7, 10, Attr::HighIntensity, "Program/module");
    put(&mut s, 7, 29, Attr::HighIntensity, "Library");
    put(&mut s, 7, 44, Attr::HighIntensity, "Type");
    opt(&mut s, 8, "MODS", usize::MAX);
    input(&mut s, 8, 11, 12, Target::Var("PGM".into()), "");
    input(&mut s, 8, 29, 10, Target::Var("LIB".into()), "*LIBL");
    input(&mut s, 8, 44, 7, Target::Var("TYPE".into()), "*PGM");
    for (i, r) in rows.iter().enumerate().skip(first).take(MOD_ROOM) {
        let row = 9 + i - first;
        opt(&mut s, row, "MODS", i);
        let module = r.kind == "*MODULE";
        put(
            &mut s,
            row,
            if module { 13 } else { 11 },
            Attr::Normal,
            &r.name,
        );
        put(&mut s, row, 29, Attr::Normal, &r.library);
        put(&mut s, row, 44, Attr::Normal, &r.kind);
        if r.selected {
            put(&mut s, row, 56, Attr::Normal, "Selected");
        }
    }
    more(&mut s, 19, first + MOD_ROOM >= rows.len());
    cmdline(&mut s, 20, cmd);
    list_keys(&mut s, 22);
    message(&mut s, msg);
    s.cursor = Some((8, 3));
    s
}

/// One watch: number, variable, where it is and the bytes watched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WatchRow {
    /// Its number.
    pub n: usize,
    /// The variable.
    pub variable: String,
    /// Where it is, in hexadecimal.
    pub address: String,
    /// Bytes.
    pub len: usize,
}

/// Entries a watch page shows (rows 8 to 19).
pub const WATCH_ROOM: usize = 12;

/// Work with Watch: `4=Clear 5=Display`.
#[must_use]
pub fn wrkwatch(
    sysname: &str,
    rows: &[WatchRow],
    first: usize,
    cmd: &str,
    msg: Option<&str>,
) -> Screen {
    let mut s = blank();
    title(&mut s, "Work with Watch");
    system(&mut s, sysname);
    put(&mut s, 3, 2, Attr::Blue, "Type options, press enter.");
    keys(&mut s, &[(4, 4, "4=Clear"), (4, 14, "5=Display")]);
    put(&mut s, 6, 2, Attr::HighIntensity, "Opt");
    put(&mut s, 6, 10, Attr::HighIntensity, "Num");
    put(&mut s, 6, 19, Attr::HighIntensity, "Variable");
    put(&mut s, 6, 44, Attr::HighIntensity, "Address");
    put(&mut s, 6, 65, Attr::HighIntensity, "Length");
    for (i, r) in rows.iter().enumerate().skip(first).take(WATCH_ROOM) {
        let row = 8 + i - first;
        opt(&mut s, row, "WATCH", i);
        put(&mut s, row, 10, Attr::Normal, &r.n.to_string());
        put(&mut s, row, 19, Attr::Normal, &r.variable);
        put(&mut s, row, 44, Attr::Normal, &r.address);
        put(&mut s, row, 65, Attr::Normal, &r.len.to_string());
    }
    if rows.is_empty() {
        put(&mut s, 8, 4, Attr::Normal, "(No watches exist.)");
    }
    more(&mut s, 20, first + WATCH_ROOM >= rows.len());
    cmdline(&mut s, 21, cmd);
    list_keys(&mut s, 23);
    message(&mut s, msg);
    s.cursor = Some((8, 3));
    s
}

/// One thread: identifier and status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadRow {
    /// Identifier, eight hexadecimal digits.
    pub id: String,
    /// `*ENABLED` and the like.
    pub status: String,
}

/// Work With Debugged Threads: the subset, the current and stopped
/// threads, `2=Change current 3=Disable 6=Enable`, three entries a row.
#[must_use]
pub fn threads(
    current: &str,
    stopped: &str,
    rows: &[ThreadRow],
    cmd: &str,
    msg: Option<&str>,
) -> Screen {
    let mut s = blank();
    title(&mut s, "Work With Debugged Threads");
    put(&mut s, 3, 2, Attr::Normal, "Current subset . . . :");
    put(&mut s, 3, 27, Attr::Normal, "All threads");
    put(&mut s, 4, 2, Attr::Normal, "Current thread . . . :");
    put(&mut s, 4, 27, Attr::Normal, current);
    put(&mut s, 4, 43, Attr::Normal, "Stopped thread . . . :");
    put(&mut s, 4, 68, Attr::Normal, stopped);
    put(&mut s, 6, 2, Attr::Blue, "Type options, press Enter.");
    keys(
        &mut s,
        &[
            (7, 4, "2=Change current"),
            (7, 23, "3=Disable"),
            (7, 35, "6=Enable"),
        ],
    );
    for c in [2, 29, 56] {
        put(&mut s, 9, c, Attr::HighIntensity, "Opt");
        put(&mut s, 9, c + 5, Attr::HighIntensity, "Thread");
        put(&mut s, 9, c + 15, Attr::HighIntensity, "Sts");
    }
    for (i, r) in rows.iter().enumerate().take(27) {
        let row = 10 + i / 3;
        let c = [2, 29, 56][i % 3];
        input(
            &mut s,
            row,
            c + 1,
            1,
            Target::ListOption {
                list: "THREADS".into(),
                row_index: i,
            },
            "",
        );
        put(&mut s, row, c + 5, Attr::Normal, &r.id);
        put(&mut s, row, c + 15, Attr::Normal, &r.status);
    }
    more(&mut s, 19, true);
    cmdline(&mut s, 20, cmd);
    list_keys(&mut s, 22);
    keys(&mut s, &[(22, 64, "F15=Subset")]);
    message(&mut s, msg);
    s.cursor = Some((21, 7));
    s
}

/// The command window F21 opens over a display: rows 18 to 24 of
/// `under` replaced by a bordered window with a command line.
#[must_use]
pub fn command_window(under: &Screen, cmd: &str, msg: Option<&str>) -> Screen {
    let mut s = blank();
    s.texts = under.texts.iter().filter(|t| t.row < 18).cloned().collect();
    put(&mut s, 18, 2, Attr::Blue, &".".repeat(78));
    for r in 19..=23 {
        put(&mut s, r, 2, Attr::Blue, ":");
        put(&mut s, r, 79, Attr::Blue, ":");
    }
    put(&mut s, 24, 2, Attr::Blue, &format!(":{}:", ".".repeat(76)));
    put(&mut s, 19, 37, Attr::HighIntensity, "Command");
    put(&mut s, 21, 5, Attr::Normal, "===>");
    input(&mut s, 21, 10, 67, Target::CmdLine, cmd);
    keys(
        &mut s,
        &[
            (22, 5, "F4=Prompt"),
            (22, 17, "F9=Retrieve"),
            (22, 31, "F12=Cancel"),
        ],
    );
    if let Some(m) = msg {
        let m: String = m.chars().take(72).collect();
        put(&mut s, 20, 5, Attr::HighIntensity, &m);
    }
    s.cursor = Some((21, 10));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoints_list() {
        let rows = vec![
            BkptRow {
                line: 7,
                condition: String::new(),
                thread: None,
            },
            BkptRow {
                line: 12,
                condition: "&X = 5".into(),
                thread: None,
            },
        ];
        let h = Header {
            sysname: "S",
            program: "P",
            library: "QGPL",
            module: "P",
            kind: "*PGM",
        };
        let t = wrkmodbkp(&h, &rows, 0, "", None).text_rows();
        assert!(t[0].contains("Work with Module Breakpoints"));
        assert!(t[11].contains("&X = 5"));
        assert!(t[18].contains("Bottom"));
    }

    #[test]
    fn module_list() {
        let rows = vec![
            ModRow {
                name: "P".into(),
                library: "QGPL".into(),
                kind: "*PGM".into(),
                selected: false,
            },
            ModRow {
                name: "P".into(),
                library: String::new(),
                kind: "*MODULE".into(),
                selected: true,
            },
        ];
        let t = wrkmodlst("S", &rows, 0, "", None).text_rows();
        assert!(t[9].contains("Selected"));
        assert_eq!(&t[9][12..13], "P");
    }

    #[test]
    fn window_keeps_the_top() {
        let mut under = blank();
        put(&mut under, 1, 30, Attr::HighIntensity, "Top");
        put(&mut under, 22, 2, Attr::Blue, "F3=Exit");
        let t = command_window(&under, "", None).text_rows();
        assert!(t[0].contains("Top"));
        assert!(!t[21].contains("F3=Exit"));
        assert!(t[18].contains("Command"));
    }
}
