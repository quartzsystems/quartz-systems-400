// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Stalls of the serve loop: a request that takes longer than
//! [`SLOW`] is logged with its name and time; one still running after
//! [`STUCK`] is logged from a watchdog thread with where the loop's
//! thread waits in the kernel and the page cache's dirty and writeback
//! sizes, so a stall shows its cause on the console log.

use std::fmt::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// A request this slow is logged when it ends.
const SLOW: Duration = Duration::from_secs(2);
/// A request still running this long is logged while it runs, again
/// each time as long again.
const STUCK: Duration = Duration::from_secs(5);

/// The request in progress: its name and start.
static CURRENT: Mutex<Option<(String, Instant)>> = Mutex::new(None);
/// The loop's thread.
static TID: AtomicU64 = AtomicU64::new(0);

/// A `fmt::Write` that keeps the first `n` bytes and then stops the
/// formatting.
struct Cut(String, usize);

impl std::fmt::Write for Cut {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let room = self.1.saturating_sub(self.0.len());
        if s.len() <= room {
            self.0.push_str(s);
            return Ok(());
        }
        let mut end = room;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        self.0.push_str(&s[..end]);
        Err(std::fmt::Error)
    }
}

/// A request's name; an IFS request's operation with it.
fn label(req: &stg::Request) -> String {
    if let stg::Request::IfsReq { op } = req {
        let mut c = Cut(String::new(), 48);
        let _ = write!(c, "{op:?}");
        let s = c.0;
        let end = s.find([' ', '{', '(']).unwrap_or(s.len());
        return format!("IfsReq {}", &s[..end]);
    }
    req.name().to_string()
}

/// A request starts.
pub fn begin(req: &stg::Request) {
    let l = label(req);
    *CURRENT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some((l, Instant::now()));
}

/// It ended: logged when slow.
pub fn end() {
    let done = CURRENT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    if let Some((l, at)) = done {
        let took = at.elapsed();
        if took >= SLOW {
            eprintln!(
                "qsstg: slow request {l}: {} ms; {}",
                took.as_millis(),
                memory()
            );
        }
    }
}

/// The free, cached, dirty and writeback sizes (`/proc/meminfo`).
pub fn memory() -> String {
    let m = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    m.lines()
        .filter(|l| {
            matches!(
                l.split_whitespace().next(),
                Some("MemFree:" | "Cached:" | "Dirty:" | "Writeback:")
            )
        })
        .flat_map(str::split_whitespace)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where thread `tid` of this process waits: its name, state, wait
/// channel and kernel stack (innermost first) when the kernel shows them.
fn where_waits(tid: u64) -> (String, String, String, String) {
    let base = format!("/proc/self/task/{tid}");
    let read = |f: &str| std::fs::read_to_string(format!("{base}/{f}")).unwrap_or_default();
    let stat = read("stat");
    let state = stat
        .rsplit_once(") ")
        .and_then(|(_, r)| r.split_whitespace().next())
        .unwrap_or_default()
        .to_string();
    let stack: Vec<String> = read("stack")
        .lines()
        .filter_map(|l| l.split_whitespace().nth(1))
        .take(12)
        .map(str::to_string)
        .collect();
    (
        read("comm").trim().to_string(),
        state,
        read("wchan"),
        stack.join("<"),
    )
}

/// Start the watchdog: the calling thread is the loop's.
pub fn watch() {
    TID.store(
        u64::from(
            rustix::thread::gettid()
                .as_raw_nonzero()
                .get()
                .unsigned_abs(),
        ),
        Ordering::Relaxed,
    );
    let _ = std::thread::Builder::new()
        .name("qsstg-stall".into())
        .spawn(|| {
            let mut told: Option<Instant> = None;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let cur = CURRENT
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                let Some((l, at)) = cur else {
                    told = None;
                    continue;
                };
                let long = at.elapsed();
                if long < STUCK || told.is_some_and(|t| t.elapsed() < STUCK) {
                    continue;
                }
                told = Some(Instant::now());
                eprintln!(
                    "qsstg: request {l} running {} ms; {}",
                    long.as_millis(),
                    memory()
                );
                let me = TID.load(Ordering::Relaxed);
                if let Ok(d) = std::fs::read_dir("/proc/self/task") {
                    for t in d.flatten() {
                        if let Some(tid) =
                            t.file_name().to_str().and_then(|n| n.parse::<u64>().ok())
                        {
                            let (comm, state, wchan, stack) = where_waits(tid);
                            let lp = if tid == me { "loop" } else { "" };
                            eprintln!(
                                "qsstg:   {lp:4} {tid} {comm} {state} wchan {wchan} stack {stack}"
                            );
                        }
                    }
                }
            }
        });
}
