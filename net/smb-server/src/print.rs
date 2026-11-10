//! QS/400 addition: the printers the `spoolss` pipe serves ([MS-RPRN]) and
//! the jobs printed through it (see `QS400-PATCHES.md`).
//!
//! [MS-RPRN]: https://learn.microsoft.com/openspecs/windows_protocols/ms-rprn

use std::time::SystemTime;

use async_trait::async_trait;

use crate::proto::auth::ntlm::Identity;

/// One printer as a client sees it: a print share of the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrinterInfo {
    /// The share name; the printer is `\\SERVER\share`.
    pub share: String,
    /// The printer driver name the client is told to use.
    pub driver: String,
    /// The share's text.
    pub comment: String,
    /// Where the printer is, free text.
    pub location: String,
    /// What the port column shows (the output queue, say `QGPL/QPRINT`).
    pub port: String,
    /// The form a new document starts with: one [`forms`] names.
    pub default_form: String,
    /// Jobs waiting on the printer now.
    pub jobs: u32,
    /// Driver files for clients without the driver, per environment.
    /// Empty when the driver is one clients have (the in-box class
    /// drivers): every environment is then answered with no files.
    pub driver_files: Vec<DriverFiles>,
}

/// The files of one driver for one client environment, as names in the
/// `print$` share (`x64\3\unidrv.dll`, say).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverFiles {
    /// `Windows x64`, `Windows NT x86` or `Windows ARM64`.
    pub environment: String,
    /// The driver version, 3 or 4 (MS-RPRN 2.2.1.3.1, cVersion).
    pub version: u32,
    /// The driver file.
    pub driver_path: String,
    /// The data file.
    pub data_file: String,
    /// The configuration file.
    pub config_file: String,
    /// The help file, or empty.
    pub help_file: String,
    /// Other files the driver needs.
    pub dependent_files: Vec<String>,
}

/// What `StartDocPrinter` names (MS-RPRN 2.2.1.7.1, DOC_INFO_1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocInfo {
    /// The document name, as the job shows it.
    pub document: String,
    /// The client machine (from the open's client info), or empty.
    pub machine: String,
}

/// A job as `GetJob` and `EnumJobs` show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobInfo {
    /// The job id (the spooled file number).
    pub id: u32,
    /// The owner.
    pub user: String,
    /// The client machine, or empty.
    pub machine: String,
    /// The document name.
    pub document: String,
    /// The status text, or empty.
    pub status_text: String,
    /// JOB_STATUS_* bits (MS-RPRN 2.2.3.12).
    pub status: u32,
    /// Pages, 0 when not known.
    pub pages: u32,
    /// Bytes.
    pub size: u32,
    /// When it was submitted.
    pub submitted: SystemTime,
}

/// Why a print operation failed, as the client is told.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PrintError {
    /// The user may not print there (ERROR_ACCESS_DENIED).
    #[error("access denied")]
    AccessDenied,
    /// No room for the job (ERROR_DISK_FULL).
    #[error("no room for the job")]
    Full,
    /// The printer cannot take jobs now (ERROR_NOT_READY).
    #[error("printer not ready")]
    NotReady,
}

impl PrintError {
    pub(crate) fn win32(self) -> u32 {
        match self {
            Self::AccessDenied => 5,
            Self::Full => 112,
            Self::NotReady => 21,
        }
    }
}

/// The printers of the server and the jobs sent to them.
///
/// Called from the `spoolss` pipe with the session's identity. The calls
/// run on the connection's task, one at a time per pipe.
#[async_trait]
pub trait PrintService: Send + Sync + 'static {
    /// The printers `who` may see and print to, in the order listed.
    async fn printers(&self, who: &Identity) -> Vec<PrinterInfo>;

    /// Starts a job on the printer `share` for `who`.
    async fn start_job(
        &self,
        who: &Identity,
        share: &str,
        doc: &DocInfo,
    ) -> Result<Box<dyn PrintJob>, PrintError>;

    /// The jobs of `who` on the printer `share`, oldest first.
    async fn jobs(&self, who: &Identity, share: &str) -> Vec<JobInfo>;
}

/// A job being written. Dropped without [`PrintJob::end`], it is abandoned
/// and nothing of it prints.
#[async_trait]
pub trait PrintJob: Send + Sync {
    /// The job id the client is told.
    fn id(&self) -> u32;

    /// Adds `data` to the job.
    async fn write(&mut self, data: &[u8]) -> Result<(), PrintError>;

    /// Ends the job with `pages` pages (0 when the client counted none);
    /// it then prints.
    async fn end(self: Box<Self>, pages: u32) -> Result<(), PrintError>;
}

/// A shared print service, as the server's configuration holds it.
#[derive(Clone)]
pub(crate) struct PrintServiceRef(pub(crate) std::sync::Arc<dyn PrintService>);

impl std::fmt::Debug for PrintServiceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrintService")
    }
}

impl std::ops::Deref for PrintServiceRef {
    type Target = dyn PrintService;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// The forms every printer has (`RpcEnumForms`), by name.
pub fn forms() -> impl Iterator<Item = &'static str> {
    crate::rpc::spoolss::forms::FORMS.iter().map(|f| f.name)
}
