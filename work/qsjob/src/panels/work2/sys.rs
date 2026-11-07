// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statistics displays: `WRKSYSSTS` (`QGWCSSTS`), `WRKDSKSTS`
//! (`QGWCDSKS`), `WRKSYSACT` (display file `QPYSYSAC`), and `WRKSHRPOOL`
//! (`QGWCPOOL`). A shared pool's size and activity level typed over
//! either pool list change the pool (`CHGSHRPOOL`) on Enter.

use std::fmt::Write as _;

use cmd::Resolved;
use mi::Job;

use super::{Step, dialog, exec, fetch, load, others, parm, work};
use crate::command::Fail;
use crate::console::host::Host;
use crate::display::surface::Surface;
use crate::display::uim::{Response, UimDialog};

const POOLS: &str = "POOLS";

/// The date and time row of `TOPSEP=DATETIME`.
fn stamp(host: &Host, d: &mut UimDialog) {
    d.set("*DATETIME", &crate::panels::work::stamp(host));
}

/// Megabytes typed over a size, as kilobytes.
fn kb(text: &str) -> Option<u64> {
    let t = text.trim().replace(',', ".");
    let v: f64 = t.parse().ok()?;
    if !(0.0..1.0e9).contains(&v) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in range, checked above
    Some((v * 1024.0).round() as u64)
}

/// Keep each pool entry's size and activity level as shown, to see on
/// Enter what was typed over them.
fn remember(d: &mut UimDialog, act: &str) {
    for row in &mut d.list_mut(POOLS).rows {
        let get = |k: &str| {
            row.iter()
                .find(|(v, _)| v == k)
                .map(|(_, x)| x.clone())
                .unwrap_or_default()
        };
        let (size, level) = (get("SIZE"), get(act));
        row.push(("SIZE0".into(), size));
        row.push(("ACT0".into(), level));
    }
}

/// Run `CHGSHRPOOL` for each shared pool whose size or activity level
/// was typed over; whether anything was.
fn apply(job: &mut Job, host: &Host, d: &mut UimDialog, name: &str, act: &str) -> bool {
    let rows = d.list_mut(POOLS).rows.clone();
    let mut changed = false;
    for row in rows {
        let get = |k: &str| {
            row.iter()
                .find(|(v, _)| v == k)
                .map(|(_, x)| x.trim().to_string())
                .unwrap_or_default()
        };
        let pool = get(name);
        let mut parms = String::new();
        if get("SIZE") != get("SIZE0") {
            let Some(k) = kb(&get("SIZE")) else {
                d.error("CPD9B96", &[&pool]);
                return true;
            };
            let _ = write!(parms, " SIZE({k})");
        }
        if get(act) != get("ACT0") {
            let _ = write!(parms, " ACTLVL({})", get(act));
        }
        if parms.is_empty() {
            continue;
        }
        changed = true;
        if pool.starts_with('*') {
            exec(job, host, d, &format!("CHGSHRPOOL POOL({pool}){parms}"));
        } else {
            d.error("CPD9B97", &[&pool]);
        }
    }
    changed
}

/// `WRKSYSSTS [RESET(*YES)]`: F10 restarts the measurement, F11 shows
/// the transitions.
pub fn wrksyssts(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let mut d = dialog(host, "QGWCSSTS")?;
    let mut reset = parm(r, "RESET").is_some_and(|v| v == "*YES");
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let data = fetch(host, "SYSSTS", &[if reset { "*YES" } else { "*NO" }])?;
        reset = false;
        load(d, POOLS, data);
        remember(d, "MAXACT");
        stamp(host, d);
        Ok(())
    };
    let mut keys =
        |job: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
            Ok(match resp {
                Response::Return(1) => {
                    apply(job, host, d, "POOLNAME", "MAXACT");
                    Step::Reload
                }
                Response::Return(10) => {
                    // The next look measures from now.
                    fetch(host, "SYSSTS", &["*YES"])?;
                    Step::Reload
                }
                Response::Return(11) => {
                    d.view = 1 - d.view.min(1);
                    d.set("VIEW", &(d.view + 1).to_string());
                    Step::Stay
                }
                other => others(d, &other),
            })
        };
    work(
        job,
        host,
        surface,
        &mut d,
        "WRKSYSSTS",
        &mut reload,
        &mut keys,
    )
}

/// `WRKSHRPOOL`: the shared pools.
pub fn wrkshrpool(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    let mut d = dialog(host, "QGWCPOOL")?;
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        load(d, POOLS, fetch(host, "SHRPOOL", &[])?);
        remember(d, "ACTLVL");
        Ok(())
    };
    let mut keys =
        |job: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
            Ok(match resp {
                Response::Return(1) => {
                    apply(job, host, d, "POOL", "ACTLVL");
                    Step::Reload
                }
                Response::Return(11) => {
                    d.view = 1 - d.view.min(1);
                    Step::Stay
                }
                other => others(d, &other),
            })
        };
    work(
        job,
        host,
        surface,
        &mut d,
        "WRKSHRPOOL",
        &mut reload,
        &mut keys,
    )
}

/// `WRKDSKSTS [RESET(*YES)]`: F10 restarts the measurement.
pub fn wrkdsksts(
    job: &mut Job,
    host: &Host,
    surface: &mut Surface,
    r: &Resolved,
) -> Result<String, Fail> {
    let mut d = dialog(host, "QGWCDSKS")?;
    let mut reset = parm(r, "RESET").is_some_and(|v| v == "*YES");
    let mut reload = |host: &Host, d: &mut UimDialog| -> Result<(), Fail> {
        let data = fetch(host, "DSKSTS", &[if reset { "*YES" } else { "*NO" }])?;
        reset = false;
        load(d, "UNITS", data);
        stamp(host, d);
        Ok(())
    };
    let mut keys =
        |_: &mut Job, host: &Host, _: &mut Surface, d: &mut UimDialog, resp: Response| {
            Ok(match resp {
                Response::Return(10) => {
                    fetch(host, "DSKSTS", &["*YES"])?;
                    Step::Reload
                }
                other => others(d, &other),
            })
        };
    work(
        job,
        host,
        surface,
        &mut d,
        "WRKDSKSTS",
        &mut reload,
        &mut keys,
    )
}

/// `WRKSYSACT`'s display file: the object, else the shipped source.
fn sysact_file(host: &Host) -> Option<dspf::DisplayFile> {
    let opened = host
        .link
        .borrow_mut()
        .with(|l| stg::Client::dsp_open(l, "QPYSYSAC".into(), "QSYS".into(), "*JOB".into()))
        .ok()
        .and_then(|(b, _): (Vec<u8>, String)| dspf::decode(&b).ok());
    opened.or_else(|| {
        let (src, _) = dspf::Source::from_text(dspf::sys::QPYSYSAC);
        dspf::compile_dspf(&src, &dspf::NoRefs, &dspf::CompileOpts::default()).definition
    })
}

/// `WRKSYSACT`'s header values and entries.
type SysActData = (Vec<(String, String)>, Vec<perf::sysact::Entry>);

/// The service's figures for `WRKSYSACT`: the header values and the
/// entries.
fn sysact_entries(host: &Host) -> Result<SysActData, Fail> {
    let (vars, rows) = fetch(host, "SYSACT", &[])?;
    let entries = rows
        .iter()
        .map(|r| {
            let get = |k: &str| {
                r.iter()
                    .find(|(v, _)| v == k)
                    .map(|(_, x)| x.clone())
                    .unwrap_or_default()
            };
            let n = |k: &str| get(k).trim().parse::<u64>().unwrap_or(0);
            perf::sysact::Entry {
                name: get("JOB"),
                user: get("USER"),
                number: get("NUMBER"),
                thread: get("THREAD"),
                priority: u8::try_from(n("PTY")).unwrap_or(0),
                cpu: u32::try_from(n("CPUT")).unwrap_or(0),
                db_cpu: 0,
                db_reads: n("DBR"),
                db_writes: n("DBW"),
                other_reads: n("NDBR"),
                other_writes: 0,
                async_io: 0,
                allocated: n("ALLOC"),
                deallocated: n("DEALLOC"),
                wait: n("WAIT"),
            }
        })
        .collect();
    Ok((vars, entries))
}

/// What `QPYSYSAC` shows: view, key set, first entry, order, and the
/// figures.
struct SysAct {
    view: usize,
    set: usize,
    top: usize,
    by_io: bool,
    shown_once: bool,
    vars: Vec<(String, String)>,
    entries: Vec<perf::sysact::Entry>,
}

impl SysAct {
    fn var(&self, k: &str) -> String {
        self.vars
            .iter()
            .find(|(v, _)| v == k)
            .map(|(_, x)| x.clone())
            .unwrap_or_default()
    }

    /// The entries listed, the page kept within them.
    fn shown(&mut self) -> Vec<perf::sysact::Entry> {
        use perf::sysact::{PAGE, listed};
        let shown = listed(&self.entries, 1, self.by_io);
        self.top = self.top.min(shown.len().saturating_sub(1) / PAGE * PAGE);
        shown
    }

    /// The format's output values.
    fn values(&mut self, host: &Host, sysname: &str) -> Vec<(String, String)> {
        use perf::sysact::{PAGE, headings, keys, legend, row};
        let shown = self.shown();
        let stamp = crate::panels::work::stamp(host);
        let mut st = stamp.split_whitespace();
        let (date, time) = (
            st.next().unwrap_or("").to_string(),
            st.next().unwrap_or("").to_string(),
        );
        let heads = headings(self.view);
        let (k1, k2) = keys(self.view, self.set);
        let cpus: u64 = self.var("VPROCS").parse().unwrap_or(1);
        let overall = self.var("OVERALL");
        let overall = overall
            .strip_prefix("0.")
            .map_or(overall.clone(), |f| format!(".{f}"));
        let mut values: Vec<(String, String)> = vec![
            ("SYSNAME".into(), sysname.to_string()),
            ("DATE".into(), date),
            ("TIME".into(), time),
            ("REFRESH".into(), format!("{:>5}", 5)),
            ("FILTER".into(), format!("{:>5}", ".10")),
            ("ELAPSED".into(), self.var("ELAPSED")),
            ("OVERALL".into(), format!("{overall:>6}")),
            ("DBCPU".into(), format!("{:>7}", ".0")),
            ("AVGRATE".into(), format!("{:>7}", "100.0")),
            ("CAPACITY".into(), format!("{:>8}", format!("{cpus}.00"))),
            ("LEGEND".into(), legend(self.view).into()),
            ("HEAD1".into(), heads[0].into()),
            ("HEAD2".into(), heads[1].into()),
            ("HEAD3".into(), heads[2].into()),
            (
                "MORE".into(),
                if self.top + PAGE < shown.len() {
                    "More..."
                } else {
                    "Bottom"
                }
                .into(),
            ),
            ("KEYS1".into(), k1),
            ("KEYS2".into(), k2),
            (
                "COPYR".into(),
                if self.shown_once {
                    String::new()
                } else {
                    perf::sysact::COPYRIGHT.into()
                },
            ),
        ];
        self.shown_once = true;
        for (i, e) in shown.iter().skip(self.top).take(PAGE).enumerate() {
            values.push((format!("ROW{:02}", i + 1), row(self.view, e)));
        }
        values
    }
}

/// `WRKSYSACT` (Performance Tools): display file `QPYSYSAC`. Enter and
/// F10 look again, F11 steps the views, F24 the key sets, F16 orders by
/// I/O, F19 looks again every five seconds until a key is pressed;
/// option 5 works with a job, F20 to F23 run the other displays.
pub fn wrksysact(job: &mut Job, host: &Host, surface: &mut Surface) -> Result<String, Fail> {
    use crate::display::surface::Waited;
    use crate::display::uim::LineMsg;
    use perf::sysact::{KEY_SETS, PAGE, VIEWS};
    use std::time::{Duration, Instant};
    use tn5250::ds::aid;
    let Some(df) = sysact_file(host) else {
        return Err(crate::panels::failed(
            "CPF9801 display file QPYSYSAC in QSYS not found",
        ));
    };
    let mut rt = dspf::rt::Runtime::new(df, surface.wide());
    let sysname = crate::display::dds::system_name(host);
    let env = crate::display::dds::env_of(host, &sysname);
    let Some(rec) = rt.file().record("WSA").cloned() else {
        return Ok(String::new());
    };
    let (vars, entries) = sysact_entries(host)?;
    let mut st = SysAct {
        view: 0,
        set: 0,
        top: 0,
        by_io: false,
        shown_once: false,
        vars,
        entries,
    };
    let mut auto = false;
    let mut message: Option<LineMsg> = None;
    loop {
        let values = st.values(host, &sysname);
        let pairs: Vec<(&str, &str)> = values
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let out = crate::display::dds::out_buffer(&rec, &pairs);
        let written = surface.with_model(|m| {
            let r = rt.write(m, &env, "WSA", &out, &[]);
            if let Some(msg) = message.take() {
                m.error_line(
                    &crate::display::msgline::text_of(host, &msg),
                    tn5250::ds::attr::WHITE,
                );
            }
            r
        });
        if written.is_err() {
            return Ok(String::new());
        }
        surface.flush(tn5250::screen::Read::MdtFields);
        let until = Instant::now() + Duration::from_secs(5);
        let got = surface.wait_or(&mut || auto && Instant::now() >= until);
        let Some(waited) = got else {
            return Ok(String::new());
        };
        let Waited::Key(a, _) = waited else {
            (st.vars, st.entries) = sysact_entries(host)?;
            continue;
        };
        auto = false;
        let opts: Vec<(usize, String)> = surface.with_model(|m| {
            m.fields()
                .iter()
                .filter(|f| f.col == 3 && (15..15 + PAGE).contains(&f.row))
                .map(|f| (f.row - 15, f.text().trim().to_string()))
                .filter(|(_, t)| !t.is_empty())
                .collect()
        });
        let shown = st.shown();
        match aid::function_of(a) {
            Some(3 | 12) => return Ok(String::new()),
            Some(10) => (st.vars, st.entries) = sysact_entries(host)?,
            Some(11) => st.view = (st.view + 1) % VIEWS,
            Some(24) => st.set = (st.set + 1) % KEY_SETS,
            Some(19) => auto = true,
            Some(14 | 15) => message = Some(LineMsg::error("CPD9D60", &[])),
            Some(16) => {
                st.by_io = !st.by_io;
                st.top = 0;
            }
            Some(20) => message = crate::panels::exec(job, host, "WRKACTJOB"),
            Some(21) => message = crate::panels::exec(job, host, "WRKSYSSTS"),
            Some(22) => message = crate::panels::exec(job, host, "WRKSBS"),
            Some(23) => message = crate::panels::exec(job, host, "WRKDSKSTS"),
            _ if a == aid::ROLL_UP => {
                if st.top + PAGE < shown.len() {
                    st.top += PAGE;
                }
            }
            _ if a == aid::ROLL_DOWN => st.top = st.top.saturating_sub(PAGE),
            _ => {
                for (i, opt) in opts {
                    let Some(e) = shown.get(st.top + i) else {
                        continue;
                    };
                    message = match opt.as_str() {
                        "5" => crate::panels::exec(
                            job,
                            host,
                            &format!("WRKJOB JOB({}/{}/{})", e.number, e.user, e.name),
                        ),
                        "1" | "6" => Some(LineMsg::error("CPD9D60", &[])),
                        _ => Some(LineMsg::error("CPD6A60", &[])),
                    };
                }
                (st.vars, st.entries) = sysact_entries(host)?;
            }
        }
    }
}
