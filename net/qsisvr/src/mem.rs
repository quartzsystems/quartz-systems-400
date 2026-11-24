// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A system in memory: the [`Host`], [`Opener`] and [`Keeper`] the tests
//! and the fuzzer run the server over.

use std::sync::{Mutex, PoisonError};

use qsi::PeerKind;
use qsi::base::{
    DumpKind, Identity, Keylock, Level, Power, PowerState, Refcode, RefcodeList, Resources, Space,
    State, SystemDump, Time,
};
use qsi::host::console::Capture;
use qsi::service::{self, Line, ProblemStatus, Severity, Source, SourceKind, Summary};
use qsisess::Note;

use crate::host::{Flow, Host, Opener, Pipe, Refusal, refusal};
use crate::trust::{Attempt, Failure, Keeper};

/// The system in memory.
#[derive(Debug)]
pub struct MemHost {
    /// The keylock.
    pub keylock: Keylock,
    /// The IPL is complete.
    pub complete: bool,
    /// Power downs asked for.
    pub powered: Vec<Power>,
    /// Function 21's asked for.
    pub service_tools: u32,
    /// The dumps.
    pub dumps: Vec<SystemDump>,
    /// The problems: summary, notes.
    pub problems: Vec<(Summary, Vec<Line>)>,
    /// The codes, oldest first.
    pub codes: Vec<Refcode>,
}

impl Default for MemHost {
    fn default() -> Self {
        Self::new()
    }
}

fn code(seq: u64, code: &str) -> Refcode {
    Refcode {
        seq,
        code: code.into(),
        words: Vec::new(),
        text: format!("Code {code}"),
        time_ms: 1_800_000_000_000 + seq,
    }
}

impl MemHost {
    /// A running system with a few codes and one open problem.
    #[must_use]
    pub fn new() -> Self {
        let codes = vec![
            code(1, "C1001000"),
            code(2, "C6001015"),
            code(3, "C6001090"),
        ];
        let problem = Summary {
            number: 2_632_400_001,
            status: ProblemStatus::Open,
            first_seen: 1_800_000_000,
            last_seen: 1_800_000_000,
            count: 1,
            refcode: code(0, "B1001000"),
            source: Source {
                kind: SourceKind::System,
                id: 0,
                name: "QS400".into(),
            },
            description: "A test problem".into(),
            msgid: "CPF9898".into(),
            holds: false,
            closed_by: String::new(),
            closed_at: 0,
        };
        Self {
            keylock: Keylock::Normal,
            complete: true,
            powered: Vec::new(),
            service_tools: 0,
            dumps: Vec::new(),
            problems: vec![(problem, Vec::new())],
            codes,
        }
    }

    fn find(&mut self, number: u64) -> Result<usize, Refusal> {
        self.problems
            .iter()
            .position(|(s, _)| s.number == number)
            .ok_or_else(|| refusal(service::msgid::NO_PROBLEM, "No such problem."))
    }
}

impl Host for MemHost {
    fn identity(&mut self) -> Result<Identity, Refusal> {
        Ok(Identity {
            kind: PeerKind::Standalone,
            name: "QS400".into(),
            release: "V1R2M0".into(),
            serial: "S0000001".into(),
            model: "9406".into(),
            build: "test".into(),
        })
    }

    fn state(&mut self) -> Result<State, Refusal> {
        Ok(State {
            power: if self.complete {
                PowerState::Running
            } else {
                PowerState::Starting
            },
            refcode: self.codes.last().cloned(),
            release: "V1R2M0".into(),
            slot: "A".into(),
            keylock: Some(self.keylock),
        })
    }

    fn power(&mut self, power: &Power, _user: &str) -> Result<(), Refusal> {
        self.powered.push(power.clone());
        Ok(())
    }

    fn refcodes(&mut self, before: Option<u64>, limit: u32) -> Result<RefcodeList, Refusal> {
        Ok(crate::history::page(&self.codes, before, limit))
    }

    fn time(&mut self) -> Result<Time, Refusal> {
        Ok(Time {
            unix_ns: 1_800_000_000_000_000_000,
            offset_s: 0,
            zone: "QN0000UTC".into(),
        })
    }

    fn fix_level(&mut self) -> Result<Level, Refusal> {
        Ok(Level {
            level: "C6320110".into(),
            date: 0,
        })
    }

    fn resources(&mut self) -> Result<Resources, Refusal> {
        Ok(Resources {
            processors: 2,
            cpu_used: 830,
            memory_kb: 8 * 1024 * 1024,
            memory_used_kb: 5 * 1024 * 1024,
            asp_mb: 64_000,
            asp_used: 2210,
            temp_mb: 420,
            total_jobs: 120,
            active_jobs: 98,
            signed_on: 1,
            batch_running: 4,
            threads: 310,
            up_secs: 7_200,
        })
    }

    fn dumps(&mut self) -> Result<Vec<SystemDump>, Refusal> {
        Ok(self.dumps.clone())
    }

    fn take_dump(&mut self, why: &str, user: &str) -> Result<String, Refusal> {
        let name = format!("QSD{:07}", self.dumps.len() + 1);
        self.dumps.push(SystemDump {
            name: name.clone(),
            kind: DumpKind::ServiceData,
            kind_name: crate::families::DUMP_KIND.into(),
            bytes: 100,
            time: 1_800_000_000,
            by: format!("{why} ({user})"),
        });
        Ok(name)
    }

    fn delete_dump(&mut self, name: &str, _user: &str) -> Result<(), Refusal> {
        let before = self.dumps.len();
        self.dumps.retain(|d| d.name != name);
        if self.dumps.len() == before {
            return Err(refusal(qsi::base::msgid::NO_DUMP, "No such dump."));
        }
        Ok(())
    }

    fn dump_space(&mut self) -> Result<Space, Refusal> {
        Ok(Space {
            bytes: self.dumps.iter().map(|d| d.bytes).sum(),
            limit: 0,
            dumps: u32::try_from(self.dumps.len()).unwrap_or(u32::MAX),
            max_dumps: 0,
        })
    }

    fn problems(
        &mut self,
        request: &service::Request,
        user: &str,
    ) -> Result<service::Reply, Refusal> {
        use service::{Reply as R, Request as Q};
        Ok(match request {
            Q::List(_) => R::List(service::Page {
                problems: self.problems.iter().map(|(s, _)| s.clone()).collect(),
                more: None,
            }),
            Q::ListRated(_) => R::ListRated(service::RatedPage {
                problems: self
                    .problems
                    .iter()
                    .map(|(s, _)| service::Rated {
                        severity: Severity::Attention,
                        summary: s.clone(),
                    })
                    .collect(),
                more: None,
            }),
            Q::Get(g) | Q::GetRated(g) => {
                let i = self.find(g.number)?;
                let (s, log) = self.problems[i].clone();
                let problem = service::Problem {
                    summary: s,
                    resources: Vec::new(),
                    log,
                };
                if matches!(request, Q::Get(_)) {
                    R::Get(problem)
                } else {
                    R::GetRated(service::RatedProblem {
                        severity: Severity::Attention,
                        problem,
                    })
                }
            }
            Q::Close(c) => {
                let i = self.find(c.number)?;
                let s = &mut self.problems[i].0;
                if s.status == ProblemStatus::Closed {
                    return Err(refusal(service::msgid::CLOSED, "Closed already."));
                }
                s.status = ProblemStatus::Closed;
                s.closed_by = user.into();
                R::Close(s.clone())
            }
            Q::Note(n) => {
                let i = self.find(n.number)?;
                let line = Line {
                    time: 1_800_000_000,
                    text: format!("{user}: {}", n.text),
                };
                self.problems[i].1.push(line.clone());
                R::Note(line)
            }
        })
    }

    fn service_tools(&mut self, _user: &str) -> Result<(), Refusal> {
        self.service_tools += 1;
        Ok(())
    }

    fn screen(&mut self) -> Result<Capture, Refusal> {
        Ok(Capture {
            serial: false,
            frame: None,
            text: "Sign On".into(),
        })
    }
}

/// A pipe that sends what it holds, then closes.
#[derive(Debug)]
pub struct BytesPipe(pub Vec<u8>);

impl Pipe for BytesPipe {
    fn poll(&mut self) -> Flow {
        if self.0.is_empty() {
            Flow::End(None)
        } else {
            Flow::Bytes(std::mem::take(&mut self.0))
        }
    }

    fn put(&mut self, _bytes: &[u8]) -> Result<(), Note> {
        Ok(())
    }
}

/// Streams in memory: a console that is never there, dumps of their
/// name's bytes.
#[derive(Debug, Default)]
pub struct MemOpener;

impl Opener for MemOpener {
    fn console(&mut self, _take_over: bool) -> Result<Box<dyn Pipe>, Refusal> {
        Err(refusal(
            qsi::host::host::msgid::SERVICE_DOWN,
            "The console cannot be reached.",
        ))
    }

    fn dump(&mut self, name: &str) -> Result<(u64, Box<dyn Pipe>), Refusal> {
        let bytes = name.as_bytes().repeat(100);
        Ok((bytes.len() as u64, Box::new(BytesPipe(bytes))))
    }
}

#[derive(Debug, Default)]
struct KeeperState {
    password: Option<String>,
    attempts: u32,
    pins: Vec<Attempt>,
    failures: Vec<(Attempt, Failure)>,
}

/// The password and the pins in memory, five attempts.
#[derive(Debug, Default)]
pub struct MemKeeper(Mutex<KeeperState>);

impl MemKeeper {
    /// With `password` out.
    #[must_use]
    pub fn with_password(password: &str) -> Self {
        let k = Self::default();
        k.0.lock().unwrap_or_else(PoisonError::into_inner).password = Some(password.into());
        k
    }

    /// The consoles pinned.
    #[must_use]
    pub fn pins(&self) -> Vec<Attempt> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pins
            .clone()
    }

    /// The failures so far.
    #[must_use]
    pub fn failures(&self) -> Vec<(Attempt, Failure)> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .failures
            .clone()
    }

    /// Whether a password is out.
    #[must_use]
    pub fn out(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .password
            .is_some()
    }
}

impl Keeper for MemKeeper {
    fn attempt(&self) -> Option<String> {
        let mut s = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        s.attempts += 1;
        if s.attempts > 5 {
            s.password = None;
        }
        s.password.clone()
    }

    fn failed(&self, attempt: &Attempt, why: Failure) {
        let mut s = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        s.failures.push((attempt.clone(), why));
    }

    fn paired(&self, attempt: &Attempt) -> bool {
        let mut s = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if s.pins.len() >= 2 {
            return false;
        }
        s.pins.push(attempt.clone());
        s.password = None;
        true
    }
}
