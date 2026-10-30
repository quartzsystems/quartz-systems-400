// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Environment variables and child processes.
//!
//! The job-level environment lives in the job's process ([`job_vars`],
//! [`set_job`], [`remove_job`]); a job starts with a copy of the
//! system-level variables, which the storage service keeps and the job
//! reaches through a [`SysEnv`] the job installs. `ADDENVVAR`, the QSH
//! shell and the bound procedures (`getenv`, `putenv`, `Qp0zGetEnv`,
//! `Qp0zPutEnv`, `Qp0zDltEnv`, `Qp0zGetAllSysEnv`, `Qp0zGetSysEnv`,
//! `Qp0zPutSysEnv`, `Qp0zDltSysEnv`, in `QP0ZCPA` and `QP0ZSYSE`) all
//! share it.
//!
//! `spawn`, `spawnp`, `wait`, `waitpid` (`QP0ZSPWN`), `kill`
//! (`QP0SSRV1`), `getpid` and `getppid` (`QP0WSRV1`) and `Qp2RunPase`
//! (`QP2USER`) reach the job's [`Procs`], which starts child jobs and
//! PASE programs.

mod calls;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard};

pub(crate) use calls::procedures;

/// One variable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Var {
    /// The name, case kept.
    pub name: String,
    /// The value; `None` is `*NULL`.
    pub value: Option<String>,
    /// The CCSID it is tagged with; 0 is the job's.
    pub ccsid: i32,
}

/// How a set treats a variable of the same name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetMode {
    /// Fail when it exists (`ADDENVVAR REPLACE(*NO)`).
    Add,
    /// Fail when it does not (`CHGENVVAR`).
    Change,
    /// Either (`putenv`, `REPLACE(*YES)`).
    Put,
}

/// Why a change was refused: the message and its text.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{msgid} {text}")]
pub struct EnvFail {
    /// `CPFA980`, `CPFA981`, `CPFA98E` ...
    pub msgid: String,
    /// The text.
    pub text: String,
}

impl EnvFail {
    /// A refusal.
    #[must_use]
    pub fn new(msgid: &str, text: impl Into<String>) -> Self {
        Self {
            msgid: msgid.into(),
            text: text.into(),
        }
    }
}

/// The system-level variables, kept by the storage service.
pub trait SysEnv {
    /// Every variable.
    fn list(&self) -> Result<Vec<Var>, EnvFail>;
    /// Add or change one.
    fn set(&self, var: Var, mode: SetMode) -> Result<(), EnvFail>;
    /// Remove one, or all for `*ALL`.
    fn remove(&self, name: &str) -> Result<(), EnvFail>;
}

/// A child process to start (`spawn()`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpawnReq {
    /// The path, or the file name to search `PATH` for.
    pub path: String,
    /// `spawnp`: search `PATH`.
    pub search: bool,
    /// The arguments.
    pub argv: Vec<String>,
    /// The environment; `None` inherits the job's.
    pub envp: Option<Vec<String>>,
    /// The descriptor map (`SPAWN_FDCLOSED` is -1).
    pub fd_map: Vec<i32>,
    /// The inheritance flags.
    pub flags: u32,
}

/// The job's child processes.
pub trait Procs {
    /// Start a child job; its process ID, or an `errno`.
    fn spawn(&self, req: SpawnReq) -> Result<i32, i32>;
    /// Wait for a child (-1 any): its process ID and status (the
    /// `<sys/wait.h>` encoding), `None` with `nohang` while it runs.
    fn wait(&self, pid: i32, nohang: bool) -> Result<Option<(i32, i32)>, i32>;
    /// Send a signal (IBM numbering).
    fn kill(&self, pid: i32, sig: i32) -> Result<(), i32>;
    /// The job's process ID and its parent's.
    fn ids(&self) -> (i32, i32);
    /// Run a PASE program to its end (`Qp2RunPase`): its exit status,
    /// or an `errno`.
    fn run_pase(&self, path: &str, argv: Vec<String>, env: Vec<String>) -> Result<i32, i32> {
        let _ = (path, argv, env);
        Err(3470)
    }
}

struct JobEnv {
    vars: Vec<Var>,
    seeded: bool,
}

static JOB: Mutex<JobEnv> = Mutex::new(JobEnv {
    vars: Vec::new(),
    seeded: false,
});

thread_local! {
    static SYSTEM: RefCell<Option<Rc<dyn SysEnv>>> = const { RefCell::new(None) };
    static PROCS: RefCell<Option<Rc<dyn Procs>>> = const { RefCell::new(None) };
}

fn job_env() -> MutexGuard<'static, JobEnv> {
    JOB.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Give the job its system-level variables' keeper.
pub fn install_system(s: Rc<dyn SysEnv>) {
    SYSTEM.with(|c| *c.borrow_mut() = Some(s));
}

/// The system-level variables' keeper, if the job installed one.
#[must_use]
pub fn system() -> Option<Rc<dyn SysEnv>> {
    SYSTEM.with(|c| c.borrow().clone())
}

/// Give the job its child processes' keeper.
pub fn install_procs(p: Rc<dyn Procs>) {
    PROCS.with(|c| *c.borrow_mut() = Some(p));
}

/// The child processes' keeper, if the job installed one.
#[must_use]
pub fn procs() -> Option<Rc<dyn Procs>> {
    PROCS.with(|c| c.borrow().clone())
}

/// Start the job's environment with `vars` (a child job's inherited
/// environment), unless it has started already.
pub fn seed(vars: Vec<Var>) {
    let mut e = job_env();
    if !e.seeded {
        e.vars = vars;
        e.seeded = true;
    }
}

/// Copy the system-level variables in at first use.
fn ensure_seeded() {
    if job_env().seeded {
        return;
    }
    let vars = system().and_then(|s| s.list().ok()).unwrap_or_default();
    seed(vars);
}

/// The job-level variables.
#[must_use]
pub fn job_vars() -> Vec<Var> {
    ensure_seeded();
    job_env().vars.clone()
}

/// One job-level variable.
#[must_use]
pub fn job_var(name: &str) -> Option<Var> {
    ensure_seeded();
    job_env().vars.iter().find(|v| v.name == name).cloned()
}

/// Apply an add or change to a list of variables at `level` (`*JOB`,
/// `*SYS`, for the messages).
pub fn change(vars: &mut Vec<Var>, var: Var, mode: SetMode, level: &str) -> Result<(), EnvFail> {
    let at = vars.iter().position(|v| v.name == var.name);
    match (at, mode) {
        (Some(_), SetMode::Add) => Err(EnvFail::new(
            "CPFA980",
            format!("An environment variable {} is already at level {level}.", var.name),
        )),
        (None, SetMode::Change) => Err(EnvFail::new(
            "CPFA981",
            format!("There is no environment variable {} at level {level}.", var.name),
        )),
        (Some(i), _) => {
            vars[i] = var;
            Ok(())
        }
        (None, _) => {
            vars.push(var);
            Ok(())
        }
    }
}

/// Add or change a job-level variable.
pub fn set_job(var: Var, mode: SetMode) -> Result<(), EnvFail> {
    ensure_seeded();
    change(&mut job_env().vars, var, mode, "*JOB")
}

/// Remove a job-level variable (`*ALL`: every one).
pub fn remove_job(name: &str) -> Result<(), EnvFail> {
    ensure_seeded();
    let mut e = job_env();
    if name == "*ALL" {
        e.vars.clear();
        return Ok(());
    }
    match e.vars.iter().position(|v| v.name == name) {
        Some(i) => {
            e.vars.remove(i);
            Ok(())
        }
        None => Err(EnvFail::new(
            "CPFA981",
            format!("There is no environment variable {name} at level *JOB."),
        )),
    }
}

/// The job's variables as `NAME=value` pairs (a `*NULL` value is
/// empty), for a shell or a child job.
#[must_use]
pub fn job_pairs() -> Vec<(String, String)> {
    job_vars()
        .into_iter()
        .map(|v| (v.name, v.value.unwrap_or_default()))
        .collect()
}

/// `NAME=value` split; the whole string is the name when there is no
/// `=`.
#[must_use]
pub fn split(s: &str) -> (String, Option<String>) {
    match s.split_once('=') {
        Some((n, v)) => (n.to_string(), Some(v.to_string())),
        None => (s.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        let mut v = Vec::new();
        let var = |n: &str, x: &str| Var {
            name: n.into(),
            value: Some(x.into()),
            ccsid: 0,
        };
        change(&mut v, var("A", "1"), SetMode::Add, "*JOB").unwrap();
        assert_eq!(
            change(&mut v, var("A", "2"), SetMode::Add, "*JOB").unwrap_err().msgid,
            "CPFA980"
        );
        change(&mut v, var("A", "2"), SetMode::Change, "*JOB").unwrap();
        assert_eq!(v[0].value.as_deref(), Some("2"));
        assert_eq!(
            change(&mut v, var("B", "2"), SetMode::Change, "*JOB").unwrap_err().msgid,
            "CPFA981"
        );
        change(&mut v, var("B", "3"), SetMode::Put, "*JOB").unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(split("X=y=z"), ("X".into(), Some("y=z".into())));
        assert_eq!(split("X"), ("X".into(), None));
    }

    struct Sys(std::cell::RefCell<Vec<Var>>);

    impl SysEnv for Sys {
        fn list(&self) -> Result<Vec<Var>, EnvFail> {
            Ok(self.0.borrow().clone())
        }
        fn set(&self, var: Var, mode: SetMode) -> Result<(), EnvFail> {
            change(&mut self.0.borrow_mut(), var, mode, "*SYS")
        }
        fn remove(&self, _name: &str) -> Result<(), EnvFail> {
            Ok(())
        }
    }

    #[test]
    fn a_job_starts_with_the_system_s_variables() {
        let sys = Sys(std::cell::RefCell::new(vec![Var {
            name: "QIBM_MULTI_THREADED".into(),
            value: Some("Y".into()),
            ccsid: 37,
        }]));
        install_system(Rc::new(sys));
        assert_eq!(job_var("QIBM_MULTI_THREADED").unwrap().value.as_deref(), Some("Y"));
        set_job(
            Var {
                name: "A".into(),
                value: Some("1".into()),
                ccsid: 0,
            },
            SetMode::Put,
        )
        .unwrap();
        assert!(job_pairs().contains(&("A".to_string(), "1".to_string())));
        remove_job("A").unwrap();
        assert_eq!(remove_job("A").unwrap_err().msgid, "CPFA981");
        // The system's copy is the system's: the job's change stays in the job.
        assert!(system().unwrap().list().unwrap().iter().all(|v| v.name != "A"));
    }
}
