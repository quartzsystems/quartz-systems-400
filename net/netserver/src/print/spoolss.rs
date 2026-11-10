// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`Spoolss`]: the library's `PrintService` over the job's [`Printer`],
//! so a document printed through `\pipe\spoolss` becomes a spooled file
//! on the print share's output queue exactly as one written to the share
//! does. Printers are the job's print shares, for any signed-on profile;
//! each names its driver and shows its output queue as the port. The jobs
//! a client sees are the ones it is writing through this job.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use async_trait::async_trait;
use smb_server::Identity;
use smb_server::print::{
    DocInfo, DriverFiles, JobInfo, PrintError as SpoolError, PrintJob as SpoolJob, PrintService,
    PrinterInfo,
};
use tracing::warn;

use super::{OutQueue, PrintError, PrintJob, Printer};

/// `JOB_STATUS_SPOOLING` (MS-RPRN 2.2.3.12).
const JOB_STATUS_SPOOLING: u32 = 0x0000_0008;

/// The form documents start with.
const DEFAULT_FORM: &str = "Letter";

/// One print share as `spoolss` shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpoolPrinter {
    /// The share name.
    pub name: String,
    /// Its description.
    pub text: String,
    /// The output queue its documents spool to.
    pub queue: OutQueue,
    /// The printer driver clients are told to use.
    pub driver: String,
    /// The driver's files in `print$`, per environment; empty when
    /// clients have the driver.
    pub driver_files: Vec<DriverFiles>,
}

/// A document being written.
#[derive(Debug, Clone)]
struct Writing {
    id: u32,
    share: String,
    user: String,
    document: String,
    size: u32,
    submitted: SystemTime,
}

/// The print shares through `spoolss`.
pub struct Spoolss {
    printer: Arc<dyn Printer>,
    printers: Vec<SpoolPrinter>,
    writing: Arc<Mutex<Vec<Writing>>>,
    next: AtomicU32,
}

impl Spoolss {
    /// The `printers` (the job's print shares) over `printer`.
    pub fn new(printer: Arc<dyn Printer>, printers: Vec<SpoolPrinter>) -> Self {
        Self {
            printer,
            printers,
            writing: Arc::default(),
            next: AtomicU32::new(1),
        }
    }

    fn find(&self, share: &str) -> Option<&SpoolPrinter> {
        self.printers
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(share))
    }
}

/// The profile of a session; none for an anonymous one.
fn profile(who: &Identity) -> Option<&str> {
    match who {
        Identity::Anonymous => None,
        Identity::User { user, .. } => Some(user),
    }
}

/// The library's error for a spooler's.
fn spool_error(e: &PrintError) -> SpoolError {
    match e {
        PrintError::NotAuthorized => SpoolError::AccessDenied,
        PrintError::NoQueue | PrintError::Other(_) => SpoolError::NotReady,
    }
}

#[async_trait]
impl PrintService for Spoolss {
    async fn printers(&self, who: &Identity) -> Vec<PrinterInfo> {
        if profile(who).is_none() {
            return Vec::new();
        }
        let writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        self.printers
            .iter()
            .map(|p| PrinterInfo {
                share: p.name.clone(),
                driver: p.driver.clone(),
                comment: p.text.clone(),
                location: String::new(),
                port: format!("{}/{}", p.queue.library, p.queue.name),
                default_form: DEFAULT_FORM.into(),
                jobs: u32::try_from(writing.iter().filter(|w| w.share == p.name).count())
                    .unwrap_or(u32::MAX),
                driver_files: p.driver_files.clone(),
            })
            .collect()
    }

    async fn start_job(
        &self,
        who: &Identity,
        share: &str,
        doc: &DocInfo,
    ) -> Result<Box<dyn SpoolJob>, SpoolError> {
        let user = profile(who).ok_or(SpoolError::AccessDenied)?;
        let p = self.find(share).ok_or(SpoolError::NotReady)?;
        let job = self
            .printer
            .open(user, &p.queue, &doc.document)
            .map_err(|e| {
                warn!(user, queue = %p.queue.name, error = %e, "spoolss document not opened");
                spool_error(&e)
            })?;
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Writing {
                id,
                share: p.name.clone(),
                user: user.into(),
                document: doc.document.clone(),
                size: 0,
                submitted: SystemTime::now(),
            });
        Ok(Box::new(Document {
            id,
            job: Mutex::new(Some(job)),
            writing: self.writing.clone(),
        }))
    }

    async fn jobs(&self, who: &Identity, share: &str) -> Vec<JobInfo> {
        let Some(user) = profile(who) else {
            return Vec::new();
        };
        let writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        writing
            .iter()
            .filter(|w| w.user == user && w.share.eq_ignore_ascii_case(share))
            .map(|w| JobInfo {
                id: w.id,
                user: w.user.clone(),
                machine: String::new(),
                document: w.document.clone(),
                status_text: String::new(),
                status: JOB_STATUS_SPOOLING,
                pages: 0,
                size: w.size,
                submitted: w.submitted,
            })
            .collect()
    }
}

/// A `spoolss` document: the spooled file and its place in the list.
struct Document {
    id: u32,
    job: Mutex<Option<Box<dyn PrintJob>>>,
    writing: Arc<Mutex<Vec<Writing>>>,
}

impl Document {
    /// Takes the document out of the list of those being written.
    fn done(&self) {
        self.writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|w| w.id != self.id);
    }
}

#[async_trait]
impl SpoolJob for Document {
    fn id(&self) -> u32 {
        self.id
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), SpoolError> {
        let mut job = self.job.lock().unwrap_or_else(PoisonError::into_inner);
        let job = job.as_mut().ok_or(SpoolError::NotReady)?;
        job.write(data).map_err(|e| spool_error(&e))?;
        let added = u32::try_from(data.len()).unwrap_or(u32::MAX);
        if let Some(w) = self
            .writing
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter_mut()
            .find(|w| w.id == self.id)
        {
            w.size = w.size.saturating_add(added);
        }
        Ok(())
    }

    async fn end(self: Box<Self>, _pages: u32) -> Result<(), SpoolError> {
        self.done();
        let job = self
            .job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        match job {
            Some(job) => job.close().map_err(|e| spool_error(&e)),
            None => Ok(()),
        }
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        // Not ended: abandoned, nothing prints.
        let job = self
            .job
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(job) = job {
            self.done();
            if let Err(e) = job.abort() {
                warn!(error = %e, "spoolss document not deleted");
            }
        }
    }
}
