// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What `WRKSYSACT` (Performance Tools) shows in display file
//! `QPYSYSAC`: the four views' headings and rows and the three key
//! sets, laid out on the V7R1M0 display's columns
//! (`tools/conformance/golden/phase12/perf/wrksysact*.txt`).

/// The views F11 steps through.
pub const VIEWS: usize = 4;
/// The key sets F24 steps through.
pub const KEY_SETS: usize = 3;
/// The copyright row of the first display.
pub const COPYRIGHT: &str = "(C) COPYRIGHT QUARTZ SYSTEMS 2026.";
/// List rows on one page.
pub const PAGE: usize = 6;

/// One job or task.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// Job or task name.
    pub name: String,
    /// User; blank for a task.
    pub user: String,
    /// Job number, six digits; blank for a task.
    pub number: String,
    /// Thread, eight hex digits.
    pub thread: String,
    /// Run priority.
    pub priority: u8,
    /// CPU used, tenths of a percent.
    pub cpu: u32,
    /// Database CPU, tenths of a percent.
    pub db_cpu: u32,
    /// Synchronous database reads.
    pub db_reads: u64,
    /// Synchronous database writes.
    pub db_writes: u64,
    /// Synchronous other reads.
    pub other_reads: u64,
    /// Synchronous other writes.
    pub other_writes: u64,
    /// Asynchronous I/O (this release has none).
    pub async_io: u64,
    /// Storage allocated in the interval, pages.
    pub allocated: u64,
    /// Storage freed, pages.
    pub deallocated: u64,
    /// Time waiting, tenths of a second.
    pub wait: u64,
}

impl Entry {
    /// Synchronous I/O.
    #[must_use]
    pub fn sync_io(&self) -> u64 {
        self.db_reads + self.db_writes + self.other_reads + self.other_writes
    }
}

/// Tenths as IBM shows them: `.5`, `1.0`, `104.5`.
#[must_use]
pub fn tenths(t: u64) -> String {
    let (w, f) = (t / 10, t % 10);
    if w == 0 {
        format!(".{f}")
    } else {
        format!("{w}.{f}")
    }
}

/// The option legend, from column 4.
#[must_use]
pub fn legend(view: usize) -> &'static str {
    if view == 3 {
        "1=Monitor job   5=Work with job   6=Wait detail"
    } else {
        "1=Monitor job   5=Work with job"
    }
}

/// The three heading rows of a view, from column 2.
#[must_use]
pub fn headings(view: usize) -> [&'static str; 3] {
    match view {
        1 | 2 => [
            if view == 1 {
                "                                                --------Synchronous---------"
            } else {
                "                                                --------Asynchronous--------"
            },
            "     Job or                                       DB     DB   Non-DB  Non-DB",
            "Opt  Task         User        Number  Thread     Read  Write   Read    Write",
        ],
        3 => [
            "",
            "     Job or                                     --------Storage--------  Total",
            "Opt  Task         User        Number  Thread     Allocated  Deallocated   Wait",
        ],
        _ => [
            "                                                            Total  Total   DB",
            "     Job or                                            CPU   Sync  Async   CPU",
            "Opt  Task         User        Number  Thread    Pty   Util   I/O    I/O   Util",
        ],
    }
}

/// An entry's row in a view, from column 7.
#[must_use]
pub fn row(view: usize, e: &Entry) -> String {
    let lead = format!("{:<13}{:<12}{:<8}{:<8}", e.name, e.user, e.number, e.thread);
    match view {
        1 => format!(
            "{lead}{:>7}{:>7}{:>8}{:>8}",
            e.db_reads, e.db_writes, e.other_reads, e.other_writes
        ),
        2 => format!("{lead}{:>7}{:>7}{:>8}{:>8}", 0, 0, 0, e.async_io),
        3 => format!(
            "{lead}{:>12}{:>13}{:>7}",
            e.allocated,
            e.deallocated,
            tenths(e.wait.min(9999))
        ),
        _ => format!(
            "{lead}{:>5}{:>7}{:>7}{:>7}{:>6}",
            e.priority,
            tenths(u64::from(e.cpu)),
            e.sync_io(),
            e.async_io,
            tenths(u64::from(e.db_cpu))
        ),
    }
    .trim_end()
    .to_string()
}

/// The two key rows of key set `set` in `view`, from column 2.
#[must_use]
pub fn keys(view: usize, set: usize) -> (String, String) {
    match set {
        1 => (
            "F14=Display jobs only   F15=Display tasks only   F16=Resequence".into(),
            "F24=More keys".into(),
        ),
        2 => (
            "F20=Work with active jobs   F21=Work with system status".into(),
            "F22=Work with subsystems    F23=Work with disk status   F24=More keys".into(),
        ),
        _ => (
            format!(
                "F3=Exit   F10=Update list   F11=View {}   F12=Cancel   F19=Automatic refresh",
                (view + 1) % VIEWS + 1
            ),
            "F24=More keys".into(),
        ),
    }
}

/// The entries a view lists: those at or over the CPU filter (tenths of
/// a percent), busiest first (or by I/O when `by_io`).
#[must_use]
pub fn listed(entries: &[Entry], filter: u32, by_io: bool) -> Vec<Entry> {
    let mut v: Vec<Entry> = entries
        .iter()
        .filter(|e| e.cpu >= filter || (by_io && e.sync_io() > 0))
        .cloned()
        .collect();
    if by_io {
        v.sort_by_key(|e| std::cmp::Reverse(e.sync_io()));
    } else {
        v.sort_by_key(|e| std::cmp::Reverse(e.cpu));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_sit_on_the_columns() {
        let e = Entry {
            name: "QPADEV0001".into(),
            user: "USERNAM".into(),
            number: "009069".into(),
            thread: "0000003B".into(),
            priority: 1,
            cpu: 5,
            other_reads: 16,
            ..Entry::default()
        };
        assert_eq!(
            row(0, &e),
            "QPADEV0001   USERNAM     009069  0000003B    1     .5     16      0    .0"
        );
        assert_eq!(
            row(1, &e),
            "QPADEV0001   USERNAM     009069  0000003B      0      0      16       0"
        );
        assert_eq!(tenths(1045), "104.5");
        assert_eq!(
            keys(3, 0).0,
            "F3=Exit   F10=Update list   F11=View 1   F12=Cancel   F19=Automatic refresh"
        );
        assert_eq!(
            listed(&[e.clone(), Entry { cpu: 0, ..e }], 1, false).len(),
            1
        );
    }
}
