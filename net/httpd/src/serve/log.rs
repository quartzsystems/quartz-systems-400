// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The instance's logs: access log lines in a `LogFormat`, error log
//! lines filtered by `LogLevel`, and `LogMaint path files size-MB
//! [HH:MM]`. With a size of 0 a log rotates daily: its first write
//! after the day's time (00:00 UTC unless given) moves the file to
//! `<name>.YYYYMMDDhhmmss`, stamped with its last write; with a size it
//! rotates instead when a write would take it past that many MB,
//! stamped now. Only the newest `files` rotated files stay (0 keeps
//! them all).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use super::site::{Maint, Plan};
use super::time;
use crate::files::Files;

/// `LogLevel`, most severe first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// `emerg`.
    Emerg,
    /// `alert`.
    Alert,
    /// `crit`.
    Crit,
    /// `error`.
    Error,
    /// `warn`.
    Warn,
    /// `notice`.
    Notice,
    /// `info`.
    Info,
    /// `debug`.
    Debug,
}

const LEVELS: [(&str, Level); 8] = [
    ("emerg", Level::Emerg),
    ("alert", Level::Alert),
    ("crit", Level::Crit),
    ("error", Level::Error),
    ("warn", Level::Warn),
    ("notice", Level::Notice),
    ("info", Level::Info),
    ("debug", Level::Debug),
];

impl Level {
    /// From its name.
    pub fn parse(s: &str) -> Option<Self> {
        LEVELS.iter().find(|(n, _)| n.eq_ignore_ascii_case(s)).map(|(_, l)| *l)
    }

    /// Its name.
    pub fn name(self) -> &'static str {
        LEVELS.iter().find(|(_, l)| *l == self).map_or("", |(n, _)| n)
    }
}

/// What the access log knows of one request.
#[derive(Clone, Debug, Default)]
pub struct Record {
    /// The client's address.
    pub client: String,
    /// The server's address.
    pub local: String,
    /// The server's port.
    pub port: u16,
    /// The signed-on user, if any.
    pub user: Option<String>,
    /// Seconds since 1970 the request arrived.
    pub at: u64,
    /// The method.
    pub method: String,
    /// The URL path.
    pub path: String,
    /// The query, without `?`.
    pub query: String,
    /// `HTTP/1.1`.
    pub protocol: String,
    /// The request headers (lower case names).
    pub headers: Vec<(String, String)>,
    /// The response headers.
    pub out_headers: Vec<(String, String)>,
    /// The final status.
    pub status: u16,
    /// Body bytes sent.
    pub bytes: u64,
    /// Microseconds it took.
    pub micros: u64,
    /// The site's `ServerName`.
    pub server: String,
    /// The file served.
    pub file: String,
}

impl Record {
    fn request_line(&self) -> String {
        let q = if self.query.is_empty() { String::new() } else { format!("?{}", self.query) };
        format!("{} {}{q} {}", self.method, self.path, self.protocol)
    }
}

fn dash(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}

/// One access log line (without the line end) in `format`.
pub fn format_line(format: &str, r: &Record) -> String {
    let mut out = String::new();
    let mut it = format.chars().peekable();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut arg = None;
        if it.peek() == Some(&'{') {
            it.next();
            let mut a = String::new();
            for c in it.by_ref() {
                if c == '}' {
                    break;
                }
                a.push(c);
            }
            arg = Some(a);
        }
        if it.peek() == Some(&'>') || it.peek() == Some(&'<') {
            it.next();
        }
        let Some(k) = it.next() else {
            out.push('%');
            break;
        };
        let header = |list: &[(String, String)], name: &str| {
            list.iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map_or("-".to_string(), |(_, v)| v.clone())
        };
        match k {
            'h' | 'a' => out.push_str(&r.client),
            'A' => out.push_str(&r.local),
            'l' => out.push('-'),
            'u' => out.push_str(dash(r.user.as_deref().unwrap_or(""))),
            't' => out.push_str(&time::clf(r.at)),
            'r' => out.push_str(&r.request_line()),
            's' => {
                let _ = write!(out, "{}", r.status);
            }
            'b' => {
                if r.bytes == 0 {
                    out.push('-');
                } else {
                    let _ = write!(out, "{}", r.bytes);
                }
            }
            'B' => {
                let _ = write!(out, "{}", r.bytes);
            }
            'i' => out.push_str(&header(&r.headers, arg.as_deref().unwrap_or(""))),
            'o' => out.push_str(&header(&r.out_headers, arg.as_deref().unwrap_or(""))),
            'D' => {
                let _ = write!(out, "{}", r.micros);
            }
            'T' => {
                let _ = write!(out, "{}", r.micros / 1_000_000);
            }
            'U' => out.push_str(&r.path),
            'q' => {
                if !r.query.is_empty() {
                    let _ = write!(out, "?{}", r.query);
                }
            }
            'm' => out.push_str(&r.method),
            'H' => out.push_str(&r.protocol),
            'v' => out.push_str(dash(&r.server)),
            'p' => {
                let _ = write!(out, "{}", r.port);
            }
            'f' => out.push_str(dash(&r.file)),
            '%' => out.push('%'),
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}

/// Seconds since 1970, as the server sees them.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The system clock.
pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
    })
}

/// The instance's log files, written through [`Files`].
pub struct Logs {
    files: Arc<dyn Files>,
    clock: Clock,
    maint: Vec<Maint>,
    /// When each `LogMaint` log was last written.
    last: Mutex<HashMap<String, u64>>,
}

fn split(path: &str) -> (&str, &str) {
    match path.rsplit_once('/') {
        Some(("", n)) => ("/", n),
        Some((d, n)) => (d, n),
        None => ("/", path),
    }
}

/// The latest daily rotation time (`at` seconds after midnight) at or
/// before `now`.
fn boundary(now: u64, at: u64) -> u64 {
    let today = now / 86_400 * 86_400 + at;
    if today > now { today.saturating_sub(86_400) } else { today }
}

impl Logs {
    /// The logs of `plan`.
    pub fn new(files: Arc<dyn Files>, clock: Clock, plan: &Plan) -> Self {
        Self {
            files,
            clock,
            maint: plan.maint.clone(),
            last: Mutex::new(HashMap::new()),
        }
    }

    /// Now.
    pub fn now(&self) -> u64 {
        (self.clock)()
    }

    /// Add `line` (and a line end) to `path`, rotating first when its
    /// `LogMaint` says so.
    pub fn write(&self, path: &str, line: &str) {
        let mut l = line.to_string();
        l.push('\n');
        if let Some(m) = self.maint.iter().find(|m| m.path == path).cloned() {
            self.rotate(&m, l.len() as u64);
        }
        if self.files.append(path, l.as_bytes()).is_err() {
            let _ = self.files.mkdir_all(split(path).0);
            let _ = self.files.append(path, l.as_bytes());
        }
    }

    /// An error log line: `[stamp] [level] [client a] text`.
    pub fn error(&self, path: &str, level: Level, client: Option<&str>, text: &str) {
        let mut l = format!("[{}] [{}] ", time::error_stamp(self.now()), level.name());
        if let Some(c) = client {
            let _ = write!(l, "[client {c}] ");
        }
        l.push_str(text);
        self.write(path, &l);
    }

    /// Rotate and prune every `LogMaint` log (at start).
    pub fn maintain(&self) {
        for m in self.maint.clone() {
            self.rotate(&m, 0);
        }
    }

    /// Rotate `m`'s log when due before `adding` more bytes; prune.
    fn rotate(&self, m: &Maint, adding: u64) {
        let now = self.now();
        let mut last = self.last.lock().expect("logs");
        let current = self.files.stat(&m.path).ok().filter(|e| !e.dir);
        let written = match (last.get(&m.path), &current) {
            (Some(t), _) => *t,
            (None, Some(e)) if e.modified > 0 => e.modified,
            _ => now,
        };
        let due = match &current {
            None => false,
            Some(_) if m.size == 0 => written < boundary(now, m.at),
            Some(e) => e.size > 0 && e.size + adding > m.size,
        };
        last.insert(m.path.clone(), now);
        drop(last);
        if due {
            let stamp = if m.size == 0 { written } else { now };
            let to = format!("{}.{}", m.path, time::stamp(stamp));
            if self.files.exists(&to) {
                // Two rotations in one second: keep both.
                if let Ok(d) = self.files.read(&m.path) {
                    let _ = self.files.append(&to, &d);
                    let _ = self.files.remove(&m.path);
                }
            } else {
                let _ = self.files.rename(&m.path, &to);
            }
        }
        if due || adding == 0 {
            self.prune(m);
        }
    }

    /// Delete rotated files of `m` beyond the newest `files`.
    fn prune(&self, m: &Maint) {
        if m.files == 0 {
            return;
        }
        let (dir, name) = split(&m.path);
        let Ok(list) = self.files.list(dir) else {
            return;
        };
        let pre = format!("{name}.");
        let mut rotated: Vec<String> = list
            .into_iter()
            .filter(|e| !e.dir && e.name.strip_prefix(&pre).is_some_and(time::is_stamp))
            .map(|e| e.name)
            .collect();
        rotated.sort_by(|a, b| b.cmp(a));
        for n in rotated.into_iter().skip(usize::try_from(m.files).unwrap_or(usize::MAX)) {
            let p = if dir == "/" { format!("/{n}") } else { format!("{dir}/{n}") };
            let _ = self.files.remove(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conf::parse;
    use crate::files::MemFiles;

    #[test]
    fn formats() {
        let r = Record {
            client: "10.0.0.9".into(),
            user: Some("BOB".into()),
            at: 1_793_232_000,
            method: "GET".into(),
            path: "/a b".into(),
            query: "x=1".into(),
            protocol: "HTTP/1.1".into(),
            headers: vec![("user-agent".into(), "t/1".into())],
            status: 200,
            bytes: 0,
            micros: 1_500_000,
            port: 80,
            ..Record::default()
        };
        assert_eq!(
            format_line("%h %l %u %t \"%r\" %>s %b \"%{Referer}i\" \"%{User-Agent}i\" %D %T %p %q %%", &r),
            "10.0.0.9 - BOB [29/Oct/2026:00:00:00 +0000] \"GET /a b?x=1 HTTP/1.1\" 200 - \"-\" \"t/1\" 1500000 1 80 ?x=1 %"
        );
        assert_eq!(Level::parse("WARN"), Some(Level::Warn));
        assert!(Level::Error < Level::Warn);
    }

    fn names(files: &MemFiles) -> Vec<String> {
        files.list("/www/x/logs").unwrap().into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn rotates_daily_at_the_time_and_keeps_files() {
        const DAY: u64 = 86_400;
        let base = 20_000 * DAY + 3 * 3600;
        let files = Arc::new(MemFiles::at(base));
        files.put("/www/x/logs/access_log", b"day0\n");
        for d in [1u64, 2, 3] {
            files.put(&format!("/www/x/logs/access_log.{}", time::stamp(base - d * DAY)), b"old\n");
        }
        // Daily at 02:00, two rotated files kept.
        let plan = Plan::new("X", "/www/x", &parse("LogMaint logs/access_log 2 0 02:00\n").unwrap());
        let now = Arc::new(Mutex::new(base + DAY - 7200));
        let n2 = Arc::clone(&now);
        let logs = Logs::new(files.clone(), Arc::new(move || *n2.lock().unwrap()), &plan);
        // 01:00 the next day: before 02:00, no rotation yet.
        logs.write("/www/x/logs/access_log", "a");
        assert_eq!(files.read("/www/x/logs/access_log").unwrap(), b"day0\na\n");
        // 02:30: rotated, stamped with the last write; two old ones go.
        *now.lock().unwrap() = base + DAY + 1800;
        logs.write("/www/x/logs/access_log", "b");
        let rotated = format!("access_log.{}", time::stamp(base + DAY - 7200));
        assert_eq!(names(&files), ["access_log", &format!("access_log.{}", time::stamp(base - DAY)), &rotated]);
        assert_eq!(files.read(&format!("/www/x/logs/{rotated}")).unwrap(), b"day0\na\n");
        assert_eq!(files.read("/www/x/logs/access_log").unwrap(), b"b\n");
        logs.error("/www/x/logs/access_log", Level::Error, Some("1.2.3.4"), "boom");
        let t = String::from_utf8(files.read("/www/x/logs/access_log").unwrap()).unwrap();
        assert!(t.ends_with("] [error] [client 1.2.3.4] boom\n"), "{t}");
    }

    #[test]
    fn rotates_by_size() {
        let files = Arc::new(MemFiles::at(1000));
        files.put("/www/x/logs/error_log", b"x");
        files.put_sized("/www/x/logs/error_log", b"x", 1024 * 1024);
        let plan = Plan::new("X", "/www/x", &parse("LogMaint logs/error_log 0 1\n").unwrap());
        let logs = Logs::new(files.clone(), Arc::new(|| 5000), &plan);
        logs.write("/www/x/logs/error_log", "next");
        assert_eq!(names(&files), ["error_log", &format!("error_log.{}", time::stamp(5000))]);
        assert_eq!(files.read("/www/x/logs/error_log").unwrap(), b"next\n");
    }
}
