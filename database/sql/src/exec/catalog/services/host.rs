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

/// An installed product load (`DSPSFWRSC`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Product {
    /// `5770SS1`, `QS00SS1` ...
    pub id: String,
    /// `*BASE` or the option number.
    pub option: String,
    /// The load: `5050` for code, the language feature.
    pub load_id: String,
    /// A code load rather than a language one.
    pub code: bool,
    /// `VxRyMz`.
    pub release: String,
    /// The product library.
    pub library: String,
    /// Our description.
    pub text: String,
}

/// A PTF as the PTF database has it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ptf {
    /// `QSnnnnn`.
    pub id: String,
    /// The product.
    pub product: String,
    /// Its option.
    pub option: String,
    /// The product's release.
    pub release: String,
    /// `LOADED`, `APPLIED` ... as `PTF_INFO` names it.
    pub status: String,
    /// The next IPL's action as `PTF_INFO` names it.
    pub ipl_action: String,
    /// Applied at an IPL rather than at once.
    pub delayed: bool,
    /// Its save file is on the system.
    pub savf: bool,
    /// The PTF superseding it.
    pub superseded_by: String,
    /// The last status change, Unix seconds.
    pub changed: u64,
}

/// The objects a walk found: each path with its attributes or why
/// they could not be read.
pub type IfsWalk = Vec<(String, Result<IfsObject, String>)>;

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

/// A storage pool as `WRKSYSSTS` shows it, the elapsed figures over the
/// host's interval.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryPool {
    /// The system pool number.
    pub number: u16,
    /// `*MACHINE`, `*BASE`, `*SHRPOOLn`, or `SBS/n` for a private pool.
    pub name: String,
    /// The subsystem of a private pool: library, name.
    pub subsystem: Option<(String, String)>,
    /// Size, kilobytes.
    pub size_kb: u64,
    /// Reserved for the system, kilobytes.
    pub reserved_kb: u64,
    /// Maximum active threads.
    pub max_active: u32,
    /// Threads active now.
    pub active: u32,
    /// Threads ineligible now.
    pub ineligible: u32,
    /// The interval, milliseconds.
    pub elapsed_ms: u64,
    /// Database faults per second.
    pub db_faults: f64,
    /// Non-database faults per second.
    pub ndb_faults: f64,
    /// Database pages read per second.
    pub db_pages: f64,
    /// Non-database pages read per second.
    pub ndb_pages: f64,
    /// Transitions per minute: active to wait, wait to ineligible,
    /// active to ineligible.
    pub transitions: [f64; 3],
}

/// A disk unit as `WRKDSKSTS` shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskUnit {
    /// The unit number, from 1.
    pub unit: u16,
    /// `SSD`, `HDD`, `NVME` ...
    pub type_code: String,
    /// The device's model.
    pub model: String,
    /// Serial number.
    pub serial: String,
    /// `Active`, `Suspended` ...
    pub status: String,
    /// `None`, `Mirrored (A)` ...
    pub protection: String,
    /// Capacity, bytes.
    pub capacity: u64,
    /// Free, bytes.
    pub free: u64,
    /// Reads, writes, sectors read, sectors written, busy milliseconds
    /// since the IPL.
    pub totals: [u64; 5],
    /// The same over the interval.
    pub elapsed: [u64; 5],
    /// The interval, milliseconds.
    pub elapsed_ms: u64,
}

/// A hardware resource (`WRKHDWRSC`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HardwareResource {
    /// `CMB01`, `LIN01`, `DD001` ...
    pub name: String,
    /// `PROCESSOR`, `COMM`, `STORAGE`, `LOCAL WORK`, `PACKAGING` ...
    pub category: String,
    /// Operational rather than not detected.
    pub operational: bool,
    /// Type and model.
    pub type_model: (String, String),
    /// Serial number, empty when unknown.
    pub serial: String,
    /// Description.
    pub text: String,
    /// Location (PCI address or device).
    pub location: String,
    /// The resource it sits in, empty at the top.
    pub parent: String,
    /// A LAN port's MAC address, 12 hexadecimal digits.
    pub mac: String,
}

/// A tape media library device (`WRKMLBSTS`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MediaLibrary {
    /// The device description.
    pub device: String,
    /// `VARIED ON`, `VARIED OFF` ...
    pub status: String,
    /// Type and model.
    pub type_model: (String, String),
    /// Its resource.
    pub resource: String,
    /// Device text.
    pub text: String,
}

/// Collection Services' configuration (`CFGPFRCOL`) and the active
/// collection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CollectionServices {
    /// Active collection: library, name, profile, start (Unix seconds).
    pub active: Option<(String, String, String, u64)>,
    /// `CFGPFRCOL LIB`.
    pub library: String,
    /// The default profile.
    pub profile: String,
    /// Interval, seconds.
    pub interval: u32,
    /// `*MGTCOL` retention, hours; `None` for permanent.
    pub retention_hours: Option<u32>,
    /// Cycle time (hour of the day) and interval (hours).
    pub cycle: (u32, u32),
    /// `CRTDBF(*YES)`.
    pub create_files: bool,
    /// Categories collected.
    pub categories: Vec<String>,
}

/// A registered exit point with its programs (`WRKREGINF`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitPoint {
    /// Name.
    pub name: String,
    /// Format.
    pub format: String,
    /// Registered rather than made by adding a program.
    pub registered: bool,
    /// It may be deregistered.
    pub allow_deregistration: bool,
    /// Its information may be changed.
    pub allow_change: bool,
    /// Maximum programs; `None` for no limit.
    pub max_programs: Option<i32>,
    /// Text.
    pub text: String,
    /// Its programs.
    pub programs: Vec<ExitProgram>,
}

/// An exit program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitProgram {
    /// The program number.
    pub number: i32,
    /// Library.
    pub library: String,
    /// Name.
    pub name: String,
    /// Its text.
    pub text: String,
    /// The exit program data.
    pub data: String,
    /// `*YES`, `*NO`, `*UNKNOWN`.
    pub threadsafe: String,
    /// `*SYSVAL`, `*RUN`, `*MSG`, `*NORUN`.
    pub action: String,
}

/// An environment variable, system level or the asking job's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvVar {
    /// System level rather than the job's.
    pub system: bool,
    /// Name.
    pub name: String,
    /// Value.
    pub value: String,
    /// CCSID.
    pub ccsid: u16,
}

/// A PTF group (`WRKPTFGRP`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PtfGroup {
    /// Name.
    pub name: String,
    /// Description.
    pub text: String,
    /// Level.
    pub level: u32,
    /// Target release.
    pub release: String,
    /// `INSTALLED`, `NOT INSTALLED` ...
    pub status: String,
    /// Its PTFs: identifier, product, status.
    pub ptfs: Vec<(String, String, String)>,
}

/// The asking job's connection, as `TCPIP_INFO` shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Connection {
    /// The host name.
    pub host_name: String,
    /// The client's address and port, when the job has a connection.
    pub client: Option<std::net::SocketAddr>,
    /// The server's side of it.
    pub server: Option<std::net::SocketAddr>,
}

/// An IFS object's authorities as `DSPAUT` shows them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IfsAuthorities {
    /// `*STMF`, `*DIR` ...
    pub kind: String,
    /// Owner.
    pub owner: String,
    /// Primary group, empty for none.
    pub group: String,
    /// Authorization list, empty for none.
    pub autl: String,
    /// `*PUBLIC`, the owner, the group, then the private authorities:
    /// who, the data authority name (`*RWX`, `*EXCLUDE` ...), the
    /// authority bits (`store::obj::header::Authority`).
    pub entries: Vec<(String, String, u32)>,
}

/// An open of an IFS object: the job (qualified name), its access
/// (read, write) and share mode (`R`, `W`, `RW`, `NONE`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IfsOpen {
    /// The job.
    pub job: String,
    /// Reads, writes.
    pub access: (bool, bool),
    /// The share mode.
    pub share: String,
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

    /// The installed product loads.
    fn products(&self) -> Option<Result<Vec<Product>, String>> {
        None
    }

    /// The PTFs.
    fn ptfs(&self) -> Option<Result<Vec<Ptf>, String>> {
        None
    }

    /// An IFS object and, when `subtree`, everything below it as a
    /// walk finds it: each path with its attributes or why they could
    /// not be read; `Err` with the errno text when the start cannot be.
    fn ifs_objects(&self, _path: &str, _subtree: bool) -> Option<Result<IfsWalk, String>> {
        None
    }

    /// A stream file's bytes and its CCSID.
    fn ifs_read(&self, _path: &str) -> Option<Result<(Vec<u8>, u16), String>> {
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

    /// The storage pools.
    fn memory_pools(&self) -> Option<Vec<MemoryPool>> {
        None
    }

    /// The disk units of ASP 1.
    fn disk_units(&self) -> Option<Vec<DiskUnit>> {
        None
    }

    /// The hardware resources.
    fn hardware(&self) -> Option<Result<Vec<HardwareResource>, String>> {
        None
    }

    /// The tape media libraries.
    fn media_libraries(&self) -> Option<Vec<MediaLibrary>> {
        None
    }

    /// Collection Services.
    fn collection_services(&self) -> Option<CollectionServices> {
        None
    }

    /// The exit points and their programs.
    fn exit_points(&self) -> Option<Result<Vec<ExitPoint>, String>> {
        None
    }

    /// The system's and the asking job's environment variables.
    fn environment_variables(&self) -> Option<Vec<EnvVar>> {
        None
    }

    /// The PTF groups.
    fn ptf_groups(&self) -> Option<Result<Vec<PtfGroup>, String>> {
        None
    }

    /// The asking job's connection and the host name.
    fn connection(&self) -> Option<Connection> {
        None
    }

    /// Resolve a host name, through `server` when given.
    fn dns_lookup(&self, _name: &str, _server: Option<&str>) -> Option<Result<Vec<std::net::IpAddr>, String>> {
        None
    }

    /// An IFS object's authorities.
    fn ifs_authorities(&self, _path: &str) -> Option<Result<IfsAuthorities, String>> {
        None
    }

    /// The opens of an IFS object.
    fn ifs_opens(&self, _path: &str) -> Option<Result<Vec<IfsOpen>, String>> {
        None
    }
}
