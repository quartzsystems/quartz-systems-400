// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! TCP/IP in the storage service: the configuration (the `QUSRSYS`
//! files, [`store`]) cached with the run-time state (`tcpcfg::State`),
//! the kernel reached through `tcpcfg::net::kernel::Kernel`, and the
//! answers to the `tcp` request (`stg::tcp`). The configuration
//! commands (`crate::cpp::tcpcmd`) change it through [`change`] and
//! [`operate`].

pub mod store;

use std::cell::RefCell;

use stg::tcp::{ServerInfo, TcpOp, TcpReply};
use tcpcfg::apply::{self, Ctx, StartOpts};
use tcpcfg::files::Table;
use tcpcfg::line::KernelNames;
use tcpcfg::msg::{self, Note};
use tcpcfg::net::NetOps;
use tcpcfg::net::kernel::Kernel;
use tcpcfg::{Config, State};

use crate::ipl::system::{System, now_unix};

/// The configuration, the state and the kernel.
pub struct Runtime {
    /// The configuration as the files hold it.
    pub cfg: Config,
    /// What runs.
    pub st: State,
    /// The kernel.
    pub net: Kernel,
}

thread_local! {
    /// The service is one thread; the runtime lives here between
    /// requests, loaded on first use.
    static RT: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Run `f` with the runtime, loading the configuration on first use.
pub fn with<T>(system: &mut System, f: impl FnOnce(&mut System, &mut Runtime) -> T) -> Result<T, String> {
    let taken = RT.with(|r| r.borrow_mut().take());
    let mut rt = match taken {
        Some(rt) => rt,
        None => Runtime {
            cfg: store::load(system)?,
            st: State::default(),
            net: Kernel::new(),
        },
    };
    let out = f(system, &mut rt);
    RT.with(|r| *r.borrow_mut() = Some(rt));
    Ok(out)
}

/// The lines TCP/IP sees until communications configuration answers:
/// one per kernel link (`LIND(ETH0)` is `eth0`).
// BLOCKED(phase10): the *LIND objects (net/cmncfg) implement LineState.
fn lines(net: &mut Kernel) -> KernelNames {
    KernelNames::new(net.links().unwrap_or_default().into_iter().map(|l| l.name))
}

/// Run an engine step over the runtime.
pub fn operate(
    system: &mut System,
    f: impl FnOnce(&mut Ctx<'_>) -> tcpcfg::Result<Vec<Note>>,
) -> tcpcfg::Result<Vec<Note>> {
    with(system, |_, rt| {
        let mut l = lines(&mut rt.net);
        let mut c = Ctx {
            cfg: &rt.cfg,
            st: &mut rt.st,
            net: &mut rt.net,
            lines: &mut l,
            now: now_unix(),
        };
        f(&mut c)
    })
    .map_err(|e| tcpcfg::Error::new("CPF9898", e))?
}

/// A configuration change: `f` changes a copy; the table is written
/// back, the copy becomes the configuration and, when TCP/IP is
/// active, the change is applied. `f`'s text and the notes come back.
pub fn change(
    system: &mut System,
    table: Table,
    f: impl FnOnce(&mut Config) -> tcpcfg::Result<String>,
) -> tcpcfg::Result<(String, Vec<Note>)> {
    let mut next = with(system, |_, rt| rt.cfg.clone()).map_err(|e| tcpcfg::Error::new("CPF9898", e))?;
    let text = f(&mut next)?;
    store::save(system, &next, table).map_err(|e| tcpcfg::Error::new("CPF9898", e))?;
    let notes = with(system, |_, rt| {
        rt.cfg = next;
        let mut l = lines(&mut rt.net);
        let mut c = Ctx {
            cfg: &rt.cfg,
            st: &mut rt.st,
            net: &mut rt.net,
            lines: &mut l,
            now: now_unix(),
        };
        apply::on_change(&mut c, table)
    })
    .map_err(|e| tcpcfg::Error::new("CPF9898", e))?;
    Ok((text, notes))
}

/// The configuration as it is.
pub fn config(system: &mut System) -> Result<Config, String> {
    with(system, |_, rt| rt.cfg.clone())
}

fn done(notes: Vec<Note>, servers: Vec<String>) -> TcpReply {
    TcpReply::Done { notes, servers }
}

/// Answer a `tcp` request.
pub fn request(system: &mut System, op: TcpOp) -> TcpReply {
    match answer(system, op) {
        Ok(r) => r,
        Err(e) => TcpReply::Failed {
            note: Note::diag("CPF9898", e),
        },
    }
}

#[allow(clippy::too_many_lines)] // one arm per operation
fn answer(system: &mut System, op: TcpOp) -> Result<TcpReply, String> {
    Ok(match op {
        TcpOp::Start { strsvr, strifc, strip6 } => {
            // STRTCP reads the files afresh: they may have been changed
            // by other means.
            let cfg = store::load(system)?;
            let r = with(system, |_, rt| {
                rt.cfg = cfg;
                let mut l = lines(&mut rt.net);
                let mut c = Ctx {
                    cfg: &rt.cfg,
                    st: &mut rt.st,
                    net: &mut rt.net,
                    lines: &mut l,
                    now: now_unix(),
                };
                apply::start_tcp(&mut c, StartOpts { strifc, strip6 })
                    .map(|n| (n, if strsvr { rt.cfg.servers.autostart() } else { Vec::new() }))
            })?;
            match r {
                Ok((notes, servers)) => done(notes, servers),
                Err(e) => TcpReply::failed(&e),
            }
        }
        TcpOp::End { endsvr, ending, .. } => {
            if ending {
                let servers = with(system, |_, rt| {
                    if rt.st.tcp == tcpcfg::state::TcpStatus::Inactive {
                        return Err(tcpcfg::Error::new(msg::NOT_ACTIVE, "TCP/IP is not active."));
                    }
                    let all: Vec<String> = rt
                        .cfg
                        .servers
                        .attrs
                        .iter()
                        .map(|a| a.server.clone())
                        .chain(rt.cfg.servers.user.iter().map(|u| u.special.clone()))
                        .collect();
                    Ok(if endsvr { all } else { Vec::new() })
                })?;
                match servers {
                    Ok(s) => done(vec![Note::info(msg::ENDING, "TCP/IP ending.")], s),
                    Err(e) => TcpReply::failed(&e),
                }
            } else {
                match operate(system, apply::end_tcp) {
                    Ok(n) => done(n, Vec::new()),
                    Err(e) => TcpReply::failed(&e),
                }
            }
        }
        TcpOp::Tick => match operate(system, |c| Ok(apply::refresh(c))) {
            Ok(n) => done(n, Vec::new()),
            Err(e) => TcpReply::failed(&e),
        },
        TcpOp::Snapshot => TcpReply::Snapshot {
            snap: with(system, |_, rt| rt.st.snapshot(&rt.cfg))?,
        },
        TcpOp::Panel { what, args } => {
            let data = with(system, |_, rt| tcpcfg::panel::data(&rt.cfg, &rt.st, &what, &args))?;
            match data {
                Some(data) => TcpReply::Panel { data },
                None => TcpReply::Failed {
                    note: Note::diag(msg::NOT_FOUND, format!("The entry was not found: {}", args.join(" "))),
                },
            }
        }
        TcpOp::HostConfig => TcpReply::HostConfig {
            cfg: with(system, |_, rt| tcpcfg::resolver::HostConfig::from_config(&rt.cfg))?,
        },
        TcpOp::PortRules => TcpReply::PortRules {
            rules: with(system, |_, rt| tcpcfg::resolver::port_rules(&rt.cfg))?,
        },
        TcpOp::Services => TcpReply::Services {
            table: with(system, |_, rt| tcpcfg::resolver::services(&rt.cfg))?,
        },
        TcpOp::Servers => TcpReply::Servers {
            list: with(system, |_, rt| {
                rt.cfg
                    .servers
                    .attrs
                    .iter()
                    .map(|a| ServerInfo {
                        server: a.server.clone(),
                        autostart: a.autostart,
                        bag: a.bag.clone(),
                    })
                    .collect()
            })?,
        },
        TcpOp::SetServer { server, autostart, bag } => {
            let r = change(system, Table::Servers, |cfg| {
                let a = cfg.servers.get_mut(&server);
                if let Some(on) = autostart {
                    a.autostart = on;
                }
                for (k, v) in &bag {
                    a.set(k, v);
                }
                Ok(String::new())
            });
            match r {
                Ok((_, notes)) => done(notes, Vec::new()),
                Err(e) => TcpReply::failed(&e),
            }
        }
        TcpOp::Report { what } => TcpReply::Lines {
            lines: with(system, |_, rt| apply::report(&rt.cfg, &rt.st, &mut rt.net, &what))?,
        },
    })
}

/// `svc.tcp [ifc|rte|port|attr|host]`: what the kernel has against
/// the configuration, one line each.
pub fn console(system: &mut System, line: &str) -> String {
    let what = line.split_whitespace().nth(1).unwrap_or("").to_ascii_lowercase();
    if !matches!(what.as_str(), "" | "ifc" | "rte" | "port" | "attr" | "host") {
        return "svc.tcp [ifc|rte|port|attr|host]
".into();
    }
    match with(system, |_, rt| apply::report(&rt.cfg, &rt.st, &mut rt.net, &what)) {
        Ok(lines) => lines.iter().fold(String::new(), |mut s, l| {
            s.push_str(l);
            s.push('\n');
            s
        }),
        Err(e) => format!("CPF9898 {e}
"),
    }
}
