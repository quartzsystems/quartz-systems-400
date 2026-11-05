// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the services read outside the object store: the job table, the
//! system values, the performance figures, the spool, the network. The
//! storage service implements [`SystemHost`] over the state it already
//! keeps for `WRKACTJOB`, `WRKSYSSTS`, `WRKSPLF` and the rest; there is
//! no second collector. A method answers `None` when the host cannot
//! reach what it asks for from where the statement runs.

/// A system value as the host has it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SysValue {
    /// The name.
    pub name: String,
    /// A numeric value.
    pub number: Option<i64>,
    /// A character value; list elements joined by a blank.
    pub text: Option<String>,
    /// Quartz's one-line description.
    pub description: String,
    /// The group (`*DATTIM`, `*SEC` ...).
    pub group: String,
    /// The shipped value as text.
    pub shipped: String,
    /// A user may change it.
    pub changeable: bool,
}

/// One active job with the figures `WRKACTJOB` shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveJob {
    /// The job as the job table holds it.
    pub job: wm::table::JobInfo,
    /// Its attributes, when the service has them.
    pub attrs: Option<wm::job::JobAttrs>,
    /// CPU used since it started, milliseconds.
    pub cpu_ms: u64,
    /// CPU in the interval, tenths of a percent.
    pub cpu_tenths: u32,
    /// Threads.
    pub threads: u32,
    /// Temporary storage, megabytes.
    pub temp_mb: u64,
    /// Auxiliary I/O requests since it started.
    pub aux_io: u64,
    /// Auxiliary I/O in the interval.
    pub elapsed_aux_io: u64,
    /// Its pool's system number, 0 for none.
    pub pool_number: u16,
    /// Interactive transactions in the interval and their total
    /// response time, milliseconds.
    pub transactions: u64,
    /// Total response time, milliseconds.
    pub response_ms: u64,
    /// The interval the elapsed figures cover, milliseconds.
    pub elapsed_ms: u64,
}

/// What `WRKSYSSTS` shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SystemStatus {
    /// Jobs in the system: active, on job queues, with output.
    pub total_jobs: u32,
    /// `QMAXJOB`.
    pub max_jobs: u32,
    /// Active jobs.
    pub active_jobs: u32,
    /// Interactive jobs as a percent of the maximum.
    pub interactive_pct: f64,
    /// Interactive jobs signed on.
    pub signed_on: u32,
    /// Batch jobs running, waiting to run, ending, in message wait,
    /// held while running, held on a job queue, on a held job queue, on
    /// an unassigned job queue.
    pub batch: [u32; 8],
    /// The interval, milliseconds.
    pub elapsed_ms: u64,
    /// CPU used in the interval, percent.
    pub cpu_pct: f64,
    /// Processors the system sees.
    pub cpus: u32,
    /// Main storage, kilobytes.
    pub main_storage_kb: u64,
    /// System ASP capacity, megabytes.
    pub asp_mb: u64,
    /// System ASP used, percent.
    pub asp_used_pct: f64,
    /// Temporary storage now and at most, megabytes.
    pub temp_mb: u64,
    /// At most since the IPL, megabytes.
    pub temp_max_mb: u64,
    /// Threads of every active job.
    pub threads: u32,
    /// The system is in the restricted state.
    pub restricted: bool,
    /// The host name.
    pub host_name: String,
    /// Machine type, model and serial number.
    pub machine: (String, String, String),
    /// `NORMAL` or `MANUAL`.
    pub ipl_mode: String,
    /// The IPL type (`QIPLTYPE`).
    pub ipl_type: String,
    /// Jobs on job queues, with output only, waiting for their job log.
    pub table: [u32; 3],
    /// The job scheduler is active.
    pub scheduler_active: bool,
}

/// A job log asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobLog {
    /// The job's qualified name and its messages, oldest first.
    Found(String, Vec<wm::joblog::Entry>),
    /// No active job has the name.
    NoSuchJob,
}

/// What `NETSTAT` shows: interfaces, routes, connections, and the
/// service names of the ports the services table names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Netstat {
    /// The interfaces.
    pub interfaces: Vec<nettools::netstat::Ifc>,
    /// The routes.
    pub routes: Vec<nettools::netstat::Rte>,
    /// The connections.
    pub connections: Vec<nettools::netstat::Conn>,
    /// Port, TCP, service name.
    pub port_names: Vec<(u16, bool, String)>,
}

/// An IFS object's attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IfsObject {
    /// The path.
    pub path: String,
    /// `*STMF`, `*DIR`, `*SYMLNK` ...
    pub kind: String,
    /// Owner.
    pub owner: String,
    /// Primary group, empty for none.
    pub group: String,
    /// Bytes.
    pub size: u64,
    /// Allocated bytes.
    pub allocated: u64,
    /// Created, changed, accessed, data changed, Unix seconds.
    pub created: u64,
    /// Attributes changed.
    pub changed: u64,
    /// Last accessed.
    pub accessed: u64,
    /// Data changed.
    pub data_changed: u64,
    /// CCSID.
    pub ccsid: u16,
    /// Hard links.
    pub links: u32,
    /// The file ID.
    pub file_id: u64,
    /// Mode bits.
    pub mode: u32,
    /// The target of a symbolic link.
    pub link_target: String,
    /// Text.
    pub text: String,
    /// The object's authorization list, empty for none.
    pub autl: String,
}

/// An HTTP request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HttpRequest {
    /// `GET`, `POST` ...
    pub method: String,
    /// The URL.
    pub url: String,
    /// Headers.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
    /// Seconds to wait; 0 for the default.
    pub timeout: u32,
    /// Redirects to follow.
    pub redirects: u32,
    /// Accept any server certificate (`sslTolerate`).
    pub tolerate: bool,
}

/// An HTTP response.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HttpResponse {
    /// The status code.
    pub status: u16,
    /// Headers.
    pub headers: Vec<(String, String)>,
    /// The body.
    pub body: Vec<u8>,
}

/// The system outside the store, as the storage service reaches it.
pub trait SystemHost {
    /// The asking job's library list.
    fn library_list(&self) -> Option<wm::job::LibList> {
        None
    }

    /// Every system value.
    fn system_values(&self) -> Option<Vec<SysValue>> {
        None
    }

    /// Every active job.
    fn active_jobs(&self) -> Option<Vec<ActiveJob>> {
        None
    }

    /// What `WRKSYSSTS` shows.
    fn system_status(&self) -> Option<SystemStatus> {
        None
    }

    /// A job's log by its qualified name, `*` for the asking job.
    fn job_log(&self, _job: &str) -> Option<JobLog> {
        None
    }

    /// Every spooled file's attributes.
    fn spooled_files(&self) -> Option<Vec<wm::obj::splf::SplfAttrs>> {
        None
    }

    /// A spooled file's data (SCS) by its spool identifier.
    fn spooled_data(&self, _id: u32) -> Option<Result<Vec<u8>, String>> {
        None
    }

    /// What `NETSTAT` shows; `Err` when TCP/IP's configuration cannot
    /// be read.
    fn netstat(&self) -> Option<Result<Netstat, String>> {
        None
    }

    /// An IFS object and, to `depth` levels below a directory, what it
    /// holds; `Err` with the errno text when it cannot be read.
    fn ifs_objects(&self, _path: &str, _depth: u32) -> Option<Result<Vec<IfsObject>, String>> {
        None
    }

    /// A stream file's bytes.
    fn ifs_read(&self, _path: &str) -> Option<Result<Vec<u8>, String>> {
        None
    }

    /// Write a stream file: replace or append, CCSID of a new file.
    fn ifs_write(&self, _path: &str, _data: &[u8], _append: bool, _ccsid: u16) -> Option<Result<(), String>> {
        None
    }

    /// An HTTP request.
    fn http(&self, _req: &HttpRequest) -> Option<Result<HttpResponse, String>> {
        None
    }

    /// Run a CL command as the asking job: `Err` with the escape
    /// message's ID and text.
    fn command(&self, _cmd: &str) -> Option<Result<(), (String, String)>> {
        None
    }
}
