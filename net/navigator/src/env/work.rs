// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management: active jobs, job queues, subsystems, output queues
//! and spooled files, writers, the job scheduler. Every read is what a
//! `WRK*` or `DSP*` command shows the user; every change is a CL command
//! run as the user through [`Work::run`], so it checks its own authority
//! and answers with its own message.

use qsweb::msg::Message;

/// One active job, as `WRKACTJOB` lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Job {
    /// `123456`.
    pub number: String,
    /// The user profile it started under.
    pub user: String,
    /// The job name.
    pub name: String,
    /// `BCH`, `BCI`, `INT`, `PJ`, `SBS`, `SYS`, `ASJ`, `WTR`, ...
    pub kind: String,
    /// Its subsystem, blank for system jobs outside one.
    pub subsystem: String,
    /// `RUN`, `MSGW`, `DEQW`, ...
    pub status: String,
    /// CPU % over the elapsed interval, tenths.
    pub cpu: u32,
    /// `PGM-QZHBMAIN`, `CMD-WRKACTJOB`, blank.
    pub function: String,
    /// The user the job runs as now.
    pub current_user: String,
}

impl Job {
    /// `number/user/name`.
    pub fn qualified(&self) -> String {
        format!("{}/{}/{}", self.number, self.user, self.name)
    }
}

/// `WRKACTJOB`'s data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveJobs {
    /// The system's CPU % over the interval, tenths.
    pub cpu: u32,
    /// The elapsed interval, seconds.
    pub elapsed: u32,
    /// The jobs, in the system's order.
    pub jobs: Vec<Job>,
    /// Messages for this user about jobs it ended that have finished
    /// since its last look (shown once).
    pub notes: Vec<Message>,
}

/// What `DSPJOB` shows of one job beyond its list entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobProps {
    /// The list entry.
    pub job: Job,
    /// Entered the system, seconds since 1970.
    pub entered: u64,
    /// Started.
    pub started: u64,
    /// The job that submitted or started it (`number/user/name`).
    pub submitted_by: String,
    /// How (`autostart job entry`), blank when not known.
    pub submitted_how: String,
    /// `lib/name`.
    pub jobd: String,
    /// `lib/name`.
    pub jobq: String,
    /// `lib/name`.
    pub outq: String,
    /// `RUNPTY`.
    pub run_priority: u32,
    /// `TIMESLICE`, milliseconds.
    pub time_slice: u32,
    /// `LOG`: `4 00 *SECLVL`.
    pub logging: String,
    /// `LOGCLPGM(*YES)`.
    pub log_cl: bool,
    /// `INQMSGRPY`.
    pub inquiry: String,
    /// `CCSID`.
    pub ccsid: String,
    /// `LANGID`.
    pub language: String,
    /// `CNTRYID`.
    pub country: String,
    /// The performance figures.
    pub perf: JobPerf,
}

/// `DSPJOB`'s run attributes and `WRKACTJOB`'s interval figures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobPerf {
    /// CPU used since the job started, milliseconds.
    pub cpu_ms: u64,
    /// Seconds since it started.
    pub elapsed: u64,
    /// Asynchronous disk I/O.
    pub io_async: u64,
    /// Synchronous disk I/O.
    pub io_sync: u64,
    /// Page faults.
    pub faults: u64,
    /// Threads.
    pub threads: u32,
    /// Temporary storage, megabytes.
    pub temp_mb: u64,
    /// The most temporary storage used, megabytes.
    pub temp_peak_mb: u64,
    /// The elapsed interval, seconds.
    pub interval: u32,
    /// CPU % in the interval, tenths.
    pub interval_cpu: u32,
    /// Disk I/O in the interval.
    pub interval_io: u64,
    /// Transactions in the interval.
    pub transactions: u64,
    /// Average response time, tenths of a second; `None` when the job
    /// has none (batch).
    pub response: Option<u32>,
    /// The system pool number.
    pub pool: u32,
    /// The pool's name (`*BASE`).
    pub pool_name: String,
    /// Its activity level.
    pub activity: u32,
    /// Its size, megabytes.
    pub pool_mb: u64,
}

/// A job log message (`DSPJOBLOG`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LogEntry {
    /// `CPF1124`.
    pub id: String,
    /// `Information`, `Diagnostic`, `Escape`, `Completion`, ...
    pub kind: String,
    /// Severity.
    pub severity: u8,
    /// `10/29/26`.
    pub date: String,
    /// `06:00:01.117332`.
    pub time: String,
    /// The sending program, its library and instruction.
    pub from: (String, String, String),
    /// The receiving program, its library and instruction.
    pub to: (String, String, String),
    /// The first-level text.
    pub text: String,
}

/// A library list entry (`DSPJOB OPTION(*LIBL)`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LibEntry {
    /// The sequence number, `None` for the current library.
    pub seq: Option<u32>,
    /// The library.
    pub lib: String,
    /// `SYS`, `PRD`, `CUR`, `USR`.
    pub kind: String,
    /// Its text.
    pub text: String,
}

/// A lock the job holds or waits for (`DSPJOB OPTION(*JOBLCK)`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lock {
    /// The object (a path for a stream file).
    pub object: String,
    /// Its library, blank for a path.
    pub lib: String,
    /// `*JOBD`.
    pub typ: String,
    /// `*SHRRD`.
    pub lock: String,
    /// `HELD`, `WAIT`.
    pub status: String,
    /// `*JOB`, `*THREAD`.
    pub scope: String,
    /// The thread for a thread-scoped lock.
    pub thread: String,
}

/// One thread's call stack (`DSPJOB OPTION(*PGMSTK)`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Thread {
    /// `00000001`.
    pub id: String,
    /// The job's initial thread.
    pub initial: bool,
    /// The entries, oldest first.
    pub frames: Vec<Frame>,
}

/// A call stack entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    /// The program.
    pub program: String,
    /// Its library.
    pub lib: String,
    /// The module, blank for OPM.
    pub module: String,
    /// The procedure.
    pub procedure: String,
    /// The statement.
    pub statement: String,
    /// The activation group.
    pub actgrp: String,
}

/// A job queue (`WRKJOBQ`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobQueue {
    /// The queue.
    pub name: String,
    /// Its library.
    pub lib: String,
    /// Held (`HLDJOBQ`).
    pub held: bool,
    /// The subsystem that takes jobs from it, blank for none.
    pub subsystem: String,
    /// Jobs waiting.
    pub waiting: u32,
    /// Of them, held.
    pub held_jobs: u32,
    /// Of them, scheduled.
    pub scheduled: u32,
    /// The job queue entry's `MAXACT`, `None` for `*NOMAX`.
    pub max_active: Option<u32>,
    /// Its text.
    pub text: String,
}

/// A subsystem (`WRKSBS`, `DSPSBSD`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subsystem {
    /// The description's name.
    pub name: String,
    /// Its library.
    pub lib: String,
    /// Active.
    pub active: bool,
    /// Active jobs in it.
    pub jobs: u32,
    /// Its pools' storage (`*BASE`, `*INTERACT`).
    pub pools: Vec<String>,
    /// `MAXJOBS`, `None` for `*NOMAX`.
    pub max_jobs: Option<u32>,
    /// Its text.
    pub text: String,
}

/// A subsystem description in full (`DSPSBSD`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubsystemDesc {
    /// The list entry.
    pub sbs: Subsystem,
    /// When it started, while active.
    pub since: Option<u64>,
    /// Its monitor job, while active.
    pub monitor: Option<String>,
    /// `SGNDSPF` as `lib/name`.
    pub signon: String,
    /// The job queues it takes jobs from, `lib/name`.
    pub job_queues: Vec<String>,
    /// Pool definitions.
    pub pools: Vec<SbsPool>,
    /// Routing entries.
    pub routing: Vec<Routing>,
    /// Autostart job entries.
    pub autostart: Vec<Autostart>,
    /// Workstation entries.
    pub workstations: Vec<Workstation>,
    /// Prestart job entries.
    pub prestart: Vec<Prestart>,
}

/// A pool of a subsystem description, with its current figures.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SbsPool {
    /// The subsystem's pool identifier.
    pub id: u32,
    /// `*BASE`, `*SHRPOOL1`, or a size for a private pool.
    pub storage: String,
    /// Activity level for a private pool.
    pub activity: Option<u32>,
    /// The system pool number.
    pub system_pool: u32,
    /// Its size, megabytes.
    pub size_mb: u64,
    /// Reserved, megabytes.
    pub reserved_mb: u64,
    /// Faults per second, tenths.
    pub faults: u32,
    /// Pages per second, tenths.
    pub pages: u32,
}

/// A routing entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Routing {
    /// `SEQNBR`.
    pub seq: u32,
    /// `CMPVAL`.
    pub compare: String,
    /// The compare value's start position.
    pub start: u32,
    /// `PGM` as program and library.
    pub program: String,
    /// The program's library.
    pub lib: String,
    /// `CLS` as `lib/name`.
    pub class: String,
    /// `MAXACT`, `None` for `*NOMAX`.
    pub max_active: Option<u32>,
    /// `POOLID`.
    pub pool: u32,
}

/// An autostart job entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Autostart {
    /// `JOB`.
    pub job: String,
    /// The job description.
    pub jobd: String,
    /// Its library.
    pub lib: String,
}

/// A workstation entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Workstation {
    /// `WRKSTN` name or `WRKSTNTYPE` type.
    pub name: String,
    /// Whether `name` is a type.
    pub by_type: bool,
    /// The job description, `lib/name` or `*USRPRF`.
    pub jobd: String,
    /// `MAXACT`, `None` for `*NOMAX`.
    pub max_active: Option<u32>,
    /// `AT`: `*SIGNON`, `*ENTER`.
    pub at: String,
}

/// A prestart job entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prestart {
    /// The program.
    pub program: String,
    /// Its library.
    pub lib: String,
    /// `USER`.
    pub user: String,
    /// `INLJOBS`.
    pub initial: u32,
    /// `THRESHOLD`.
    pub threshold: u32,
    /// `MAXJOBS`, `None` for `*NOMAX`.
    pub max_jobs: Option<u32>,
}

/// An output queue (`WRKOUTQ`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutQueue {
    /// The queue.
    pub name: String,
    /// Its library.
    pub lib: String,
    /// Spooled files on it.
    pub files: u32,
    /// The writer started to it, blank for none.
    pub writer: String,
    /// Held (`HLDOUTQ`).
    pub held: bool,
    /// `SEQ`: `*FIFO`, `*JOBNBR`.
    pub order: String,
    /// `OPRCTL(*YES)`.
    pub operator_control: bool,
    /// Its text.
    pub text: String,
}

/// A spooled file (`WRKSPLF`, `WRKOUTQ`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Splf {
    /// The file.
    pub file: String,
    /// Its owner.
    pub user: String,
    /// `USRDTA`.
    pub user_data: String,
    /// `RDY`, `HLD`, `SAV`, `WTR`, `MSGW`, ...
    pub status: String,
    /// Pages.
    pub pages: u32,
    /// Copies left.
    pub copies: u32,
    /// `FORMTYPE`.
    pub form_type: String,
    /// Created, seconds since 1970.
    pub created: u64,
    /// The job, `number/user/name`.
    pub job: String,
    /// The file number in its job.
    pub number: u32,
    /// The output queue, `lib/name`.
    pub outq: String,
}

/// A printer and its writer (`WRKWTR`, `WRKCFGSTS *DEV`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Writer {
    /// The device or, for a remote writer, the writer's name.
    pub device: String,
    /// Started; `held` says whether it is held.
    pub started: bool,
    /// Held (`HLDWTR`).
    pub held: bool,
    /// Device varied on; `None` for a writer without a device.
    pub varied_on: Option<bool>,
    /// The output queue, `lib/name`.
    pub outq: String,
    /// The spooled file printing.
    pub file: String,
    /// Its user.
    pub user: String,
    /// Pages left of it.
    pub pages_left: Option<u32>,
    /// `FORMTYPE`.
    pub form_type: String,
    /// `3812 / 1`, `*LAN 3812`, `Remote (LPR 10.0.10.90)`.
    pub model: String,
}

/// A job schedule entry (`WRKJOBSCDE`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScheduleEntry {
    /// `JOB`.
    pub name: String,
    /// The entry number.
    pub entry: u32,
    /// `CMD`.
    pub command: String,
    /// `*ONCE`, `*WEEKLY`, `*MONTHLY`.
    pub frequency: String,
    /// `SCDDATE`, seconds since 1970 at 00:00 of the day; `None` for
    /// `*NONE`.
    pub date: Option<u64>,
    /// `SCDDAY`: `*ALL`, `*MON` ...
    pub days: Vec<String>,
    /// `SCDTIME` as `HH:MM`.
    pub time: String,
    /// The next submission, seconds since 1970.
    pub next: Option<u64>,
    /// `JOBQ` as `lib/name`.
    pub jobq: String,
    /// `USER`.
    pub user: String,
    /// Held (`HLDJOBSCDE`).
    pub held: bool,
    /// `TEXT`.
    pub text: String,
}

/// The values a filter offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choices {
    /// Active jobs: the subsystems.
    Subsystems,
    /// Job queues: the libraries.
    JobqLibraries,
    /// Output queues: the libraries.
    OutqLibraries,
}

/// What the work management pages need. `user` is the signed-on user:
/// the live system reads and acts as that user.
pub trait Work: Send + Sync {
    /// `WRKACTJOB`'s jobs and interval; `reset` starts a new interval
    /// for `user` (Refresh).
    fn active_jobs(&self, user: &str, reset: bool) -> ActiveJobs;
    /// The values a filter's select offers.
    fn choices(&self, user: &str, what: Choices) -> Vec<String>;
    /// `DSPJOB` of `number/user/name`.
    fn job(&self, user: &str, job: &str) -> Option<JobProps>;
    /// `DSPJOBLOG`, oldest first.
    fn job_log(&self, user: &str, job: &str) -> Vec<LogEntry>;
    /// The job's library list.
    fn library_list(&self, user: &str, job: &str) -> Vec<LibEntry>;
    /// The job's locks.
    fn locks(&self, user: &str, job: &str) -> Vec<Lock>;
    /// The job's call stack, thread by thread.
    fn call_stack(&self, user: &str, job: &str) -> Vec<Thread>;
    /// `WRKJOBQ`.
    fn job_queues(&self, user: &str) -> Vec<JobQueue>;
    /// `WRKSBS`: every subsystem description, active or not.
    fn subsystems(&self, user: &str) -> Vec<Subsystem>;
    /// `DSPSBSD` of subsystem `name`.
    fn subsystem(&self, user: &str, name: &str) -> Option<SubsystemDesc>;
    /// `WRKOUTQ`.
    fn output_queues(&self, user: &str) -> Vec<OutQueue>;
    /// The spooled files on `lib/name` (or, with `job` not empty, of
    /// that job), in queue order.
    fn spooled_files(&self, user: &str, outq: &str, job: &str) -> Vec<Splf>;
    /// `DSPSPLF`: the file and its pages' text.
    fn spooled_text(
        &self,
        user: &str,
        job: &str,
        file: &str,
        number: u32,
    ) -> Option<(Splf, Vec<String>)>;
    /// The spooled file as PDF.
    fn spooled_pdf(&self, user: &str, job: &str, file: &str, number: u32) -> Option<Vec<u8>>;
    /// `WRKWTR` with the devices' status.
    fn writers(&self, user: &str) -> Vec<Writer>;
    /// `WRKJOBSCDE`.
    fn schedule(&self, user: &str) -> Vec<ScheduleEntry>;
    /// Run CL command `command` as `user`: its completion message, or
    /// the message it failed with.
    fn run(&self, user: &str, command: &str) -> Result<Message, Message>;
}
