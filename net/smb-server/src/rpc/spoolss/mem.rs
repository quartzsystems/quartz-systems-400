//! A print service in memory, for the tests and the fuzz target: two
//! printers, and every job's bytes kept.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, UNIX_EPOCH};

use async_trait::async_trait;

use crate::print::{DocInfo, JobInfo, PrintError, PrintJob, PrintService, PrinterInfo};
use crate::proto::auth::ntlm::Identity;

/// A job as the service got it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Printed {
    pub id: u32,
    pub share: String,
    pub user: String,
    pub doc: DocInfo,
    pub data: Vec<u8>,
    /// `Some(pages)` once ended; `None` while open or when abandoned.
    pub ended: Option<u32>,
    pub abandoned: bool,
}

#[derive(Default, Clone)]
pub(crate) struct MemPrint {
    pub jobs: Arc<Mutex<Vec<Printed>>>,
}

fn user_of(who: &Identity) -> String {
    match who {
        Identity::Anonymous => String::new(),
        Identity::User { user, .. } => user.clone(),
    }
}

impl MemPrint {
    pub(crate) fn printed(&self) -> Vec<Printed> {
        self.jobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl PrintService for MemPrint {
    async fn printers(&self, who: &Identity) -> Vec<PrinterInfo> {
        if matches!(who, Identity::Anonymous) {
            return Vec::new();
        }
        let printer = |share: &str, driver: &str, form: &str| PrinterInfo {
            share: share.into(),
            driver: driver.into(),
            comment: format!("{share} text"),
            location: String::new(),
            port: format!("QUSRSYS/{share}"),
            default_form: form.into(),
            jobs: 0,
            driver_files: Vec::new(),
        };
        vec![
            printer("PRT01", "Microsoft PCL6 Class Driver", "Letter"),
            printer("LASER", "Microsoft PS Class Driver", "A4"),
        ]
    }

    async fn start_job(
        &self,
        who: &Identity,
        share: &str,
        doc: &DocInfo,
    ) -> Result<Box<dyn PrintJob>, PrintError> {
        let mut jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        let id = u32::try_from(jobs.len() + 1).unwrap_or(u32::MAX);
        jobs.push(Printed {
            id,
            share: share.into(),
            user: user_of(who),
            doc: doc.clone(),
            data: Vec::new(),
            ended: None,
            abandoned: false,
        });
        Ok(Box::new(MemJob {
            id,
            jobs: Arc::clone(&self.jobs),
            ended: false,
        }))
    }

    async fn jobs(&self, who: &Identity, share: &str) -> Vec<JobInfo> {
        let user = user_of(who);
        self.printed()
            .into_iter()
            .filter(|j| j.share == share && j.user == user && !j.abandoned)
            .map(|j| JobInfo {
                id: j.id,
                user: j.user,
                machine: j.doc.machine,
                document: j.doc.document,
                status_text: String::new(),
                status: 0,
                pages: j.ended.unwrap_or(0),
                size: u32::try_from(j.data.len()).unwrap_or(u32::MAX),
                submitted: UNIX_EPOCH + Duration::from_secs(1_794_259_625),
            })
            .collect()
    }
}

struct MemJob {
    id: u32,
    jobs: Arc<Mutex<Vec<Printed>>>,
    ended: bool,
}

impl MemJob {
    fn with(&self, f: impl FnOnce(&mut Printed)) {
        let mut jobs = self.jobs.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(job) = jobs.iter_mut().find(|j| j.id == self.id) {
            f(job);
        }
    }
}

#[async_trait]
impl PrintJob for MemJob {
    fn id(&self) -> u32 {
        self.id
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), PrintError> {
        self.with(|j| j.data.extend_from_slice(data));
        Ok(())
    }

    async fn end(mut self: Box<Self>, pages: u32) -> Result<(), PrintError> {
        self.ended = true;
        self.with(|j| j.ended = Some(pages));
        Ok(())
    }
}

impl Drop for MemJob {
    fn drop(&mut self) {
        if !self.ended {
            self.with(|j| j.abandoned = true);
        }
    }
}
