// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The fuzzing loop: pick a corpus entry, mutate it, run the target,
//! keep inputs that reach new coverage, save and minimise the ones that
//! panic, and stop the process on one that hangs.

use std::collections::HashSet;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::cov::{self, Map};
use crate::mutate::{Rng, mutate};
use crate::targets::Target;

/// A run's settings.
pub struct Options {
    /// How long to fuzz.
    pub time: Duration,
    /// The corpus directory (seeds and what the run keeps).
    pub corpus: PathBuf,
    /// Where crashing inputs go.
    pub crashes: PathBuf,
    /// One input longer than this is a hang.
    pub hang: Duration,
    /// The generator's seed; 0 for one from the clock.
    pub seed: u64,
}

/// What a run found.
#[derive(Debug, Default)]
pub struct Report {
    /// Inputs run.
    pub execs: u64,
    /// Corpus entries at the end.
    pub corpus: usize,
    /// Edges covered (0 when the build is not instrumented).
    pub covered: usize,
    /// Distinct crashes, by the panic's place, with the file saved.
    pub crashes: Vec<(String, PathBuf)>,
}

fn name(data: &[u8]) -> String {
    hex::encode(&Sha256::digest(data)[..8])
}

fn load(dir: &Path) -> Vec<Vec<u8>> {
    let mut v: Vec<(PathBuf, Vec<u8>)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| fs::read(e.path()).ok().map(|d| (e.path(), d)))
        .collect();
    v.sort();
    v.into_iter().map(|(_, d)| d).collect()
}

thread_local! {
    static LAST_PANIC: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// Silences panics and remembers where the last one happened.
fn quiet_panics() {
    panic::set_hook(Box::new(|info| {
        let place = info
            .location()
            .map_or_else(|| "?".to_string(), |l| format!("{}:{}", l.file(), l.line()));
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        LAST_PANIC.with(|p| *p.borrow_mut() = format!("{place}: {msg}"));
    }));
}

/// Runs `t` on `data`; `Some(place)` when it panicked.
pub fn run_one(t: &Target, data: &[u8]) -> Option<String> {
    match panic::catch_unwind(AssertUnwindSafe(|| (t.run)(data))) {
        Ok(()) => None,
        Err(_) => Some(LAST_PANIC.with(|p| p.borrow().clone())),
    }
}

/// The panic's place without its message (line numbers group crashes).
fn site(p: &str) -> String {
    p.split(": ").next().unwrap_or(p).to_string()
}

/// Shrinks a crashing input while it still panics at the same place.
pub fn minimise(t: &Target, data: &[u8], place: &str) -> Vec<u8> {
    let want = site(place);
    let mut best = data.to_vec();
    let mut chunk = best.len() / 2;
    while chunk > 0 {
        let mut i = 0;
        let mut shrunk = false;
        while i + chunk <= best.len() {
            let mut cand = best.clone();
            cand.drain(i..i + chunk);
            if run_one(t, &cand).is_some_and(|p| site(&p) == want) {
                best = cand;
                shrunk = true;
            } else {
                i += chunk;
            }
        }
        if !shrunk {
            chunk /= 2;
        }
    }
    best
}

/// Fuzzes `t` for `o.time`.
#[allow(clippy::too_many_lines)] // one loop with its watchdog
pub fn fuzz(t: &Target, o: &Options) -> std::io::Result<Report> {
    fs::create_dir_all(&o.corpus)?;
    quiet_panics();
    let seed = if o.seed == 0 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(1, |d| u64::try_from(d.as_nanos() & u128::from(u64::MAX)).unwrap_or(1))
    } else {
        o.seed
    };
    let mut rng = Rng::new(seed);
    let mut map = Map::default();
    let mut report = Report::default();
    let mut sites: HashSet<String> = HashSet::new();

    // The watchdog: an input still running after `hang` stops the process.
    let started = Arc::new(AtomicU64::new(0));
    let running: Arc<std::sync::Mutex<Vec<u8>>> = Arc::default();
    let epoch = Instant::now();
    {
        let started = Arc::clone(&started);
        let running = Arc::clone(&running);
        let hang = o.hang;
        let crashes = o.crashes.clone();
        let tname = t.name;
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(200));
                let s = started.load(Ordering::Relaxed);
                if s != 0 {
                    let now = u64::try_from(epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
                    if now.saturating_sub(s) > u64::try_from(hang.as_millis()).unwrap_or(u64::MAX) {
                        let input = running.lock().map(|r| r.clone()).unwrap_or_default();
                        let _ = fs::create_dir_all(&crashes);
                        let _ = fs::write(crashes.join(format!("hang-{}", name(&input))), &input);
                        eprintln!("fuzz {tname}: an input ran past {hang:?}; saved in {}", crashes.display());
                        std::process::exit(3);
                    }
                }
            }
        });
    }
    // The input in flight, kept in memory for the watchdog.
    let current = |data: &[u8]| {
        if let Ok(mut r) = running.lock() {
            r.clear();
            r.extend_from_slice(data);
        }
    };

    let mut corpus = load(&o.corpus);
    if corpus.is_empty() {
        corpus = t.seeds.iter().map(|s| s.to_vec()).collect();
        if corpus.is_empty() {
            corpus.push(Vec::new());
        }
        for c in &corpus {
            fs::write(o.corpus.join(name(c)), c)?;
        }
    }
    // The seeds once each, so their coverage counts.
    for c in &corpus {
        report.execs += 1;
        if let Some(p) = run_one(t, c) {
            sites.insert(site(&p));
            let path = save_crash(&o.crashes, &minimise(t, c, &p))?;
            report.crashes.push((p, path));
        }
        map.absorb();
    }

    let instrumented = cov::instrumented();
    let end = Instant::now() + o.time;
    let mut buf = Vec::new();
    while Instant::now() < end {
        for _ in 0..256 {
            let base = &corpus[rng.below(corpus.len())];
            let other = &corpus[rng.below(corpus.len())];
            buf.clear();
            buf.extend_from_slice(base);
            mutate(&mut rng, &mut buf, other, t.dict, t.max_len);
            current(&buf);
            started.store(
                u64::try_from(epoch.elapsed().as_millis()).unwrap_or(1).max(1),
                Ordering::Relaxed,
            );
            let r = run_one(t, &buf);
            started.store(0, Ordering::Relaxed);
            report.execs += 1;
            let new = map.absorb();
            if let Some(p) = r {
                if sites.insert(site(&p)) {
                    let min = minimise(t, &buf, &p);
                    let path = save_crash(&o.crashes, &min)?;
                    eprintln!("fuzz {}: crash at {p} -> {}", t.name, path.display());
                    report.crashes.push((p, path));
                }
                continue;
            }
            // Blind runs keep one input in 64, so the corpus still moves.
            if new > 0 || (!instrumented && rng.chance(64) && corpus.len() < 4096) {
                fs::write(o.corpus.join(name(&buf)), &buf)?;
                corpus.push(buf.clone());
            }
        }
    }
    report.corpus = corpus.len();
    report.covered = map.covered();
    Ok(report)
}

fn save_crash(dir: &Path, data: &[u8]) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let p = dir.join(format!("crash-{}", name(data)));
    fs::write(&p, data)?;
    Ok(p)
}
