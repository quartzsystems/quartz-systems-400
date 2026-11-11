// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Print shares over the job's spooling: each print file is a spooled
//! file on the share's output queue, made as the signed-on profile (the
//! job runs as it), its data as the client sent it. The share's spooled
//! file type is the spooled file's device type: `*USERASCII` (the
//! printer's own language, which the writer sends as it is), `*SCS`,
//! `*AFPDS`, or for `*AUTO` `*SCS` when the data starts as SCS does and
//! `*USERASCII` otherwise.

use std::sync::Arc;

use netserver::print::{OutQueue, PrintError, PrintJob, Printer};
use netsvrcfg::{Config, Kind, SpoolType};
use stg::Client;
use stg::netsvr::NsOp;
use wm::obj::splf::SplfAttrs;

use super::svc::Svc;

/// `netsvrcfg::Counter::PrintJobs`.
const PRINT_JOBS: u8 = 1;

/// A print share's spooling attributes.
#[derive(Clone, Debug)]
struct Spooling {
    queue: OutQueue,
    kind: SpoolType,
    prtf: String,
}

/// The job's spooler.
pub struct JobPrinter {
    svc: Arc<Svc>,
    shares: Vec<Spooling>,
}

impl JobPrinter {
    /// The spooler for `c`'s print shares.
    pub fn new(svc: Arc<Svc>, c: &Config) -> Self {
        let shares = c
            .shares
            .values()
            .filter_map(|s| match &s.kind {
                Kind::Print(p) => Some(Spooling {
                    queue: OutQueue {
                        library: p.outq.library.clone(),
                        name: p.outq.name.clone(),
                    },
                    kind: p.spool_type,
                    prtf: if p.printer_file.is_empty() {
                        String::new()
                    } else {
                        format!("{}/{}", p.printer_file.library, p.printer_file.name)
                    },
                }),
                Kind::File(_) => None,
            })
            .collect();
        Self { svc, shares }
    }
}

/// The device type of data `first` for a share of `kind`, decided on
/// the first write.
fn devtype(kind: SpoolType, first: &[u8]) -> &'static str {
    match kind {
        SpoolType::Scs => "*SCS",
        SpoolType::Afp => "*AFPDS",
        // SCS opens with a control: SHF, SVF, a format feed or a set.
        SpoolType::Auto if matches!(first.first(), Some(0x2B | 0x0C | 0x34 | 0x15)) => "*SCS",
        SpoolType::UserAscii | SpoolType::Auto => "*USERASCII",
    }
}

fn other(e: impl std::fmt::Display) -> PrintError {
    PrintError::Other(e.to_string())
}

/// One spooled file being written; opened at its first write, since the
/// type of an `*AUTO` share follows the data.
struct Job {
    svc: Arc<Svc>,
    attrs: SplfAttrs,
    kind: SpoolType,
    id: Option<u32>,
}

impl Job {
    fn opened(&mut self, first: &[u8]) -> Result<u32, PrintError> {
        if let Some(id) = self.id {
            return Ok(id);
        }
        let mut attrs = self.attrs.clone();
        attrs.devtype = devtype(self.kind, first).into();
        let id = self
            .svc
            .call(move |l| l.borrow_mut().with(|c| c.netsvr_splf_open(attrs)))
            .ok_or(PrintError::Other(String::new()))?
            .map_err(|e| {
                if e.starts_with("CPF3303") || e.contains("not found") {
                    PrintError::NoQueue
                } else if e.starts_with("CPF2189") || e.starts_with("CPF2182") {
                    PrintError::NotAuthorized
                } else {
                    PrintError::Other(e)
                }
            })?;
        self.id = Some(id);
        Ok(id)
    }
}

impl PrintJob for Job {
    fn write(&mut self, data: &[u8]) -> Result<(), PrintError> {
        let id = self.opened(data)?;
        let data = data.to_vec();
        self.svc
            .call(move |l| l.borrow_mut().with(|c| c.splf_put(id, data, 0, 0)))
            .ok_or(PrintError::Other(String::new()))?
            .map_err(other)
    }

    fn close(mut self: Box<Self>) -> Result<(), PrintError> {
        let id = self.opened(&[])?;
        self.svc
            .call(move |l| {
                let r = l.borrow_mut().with(|c| c.splf_close(id, false));
                if r.is_ok() {
                    l.borrow_mut().with(|c| {
                        c.netsvr(NsOp::Count {
                            counter: PRINT_JOBS,
                            n: 1,
                        })
                    });
                }
                r
            })
            .ok_or(PrintError::Other(String::new()))?
            .map_err(other)
    }

    fn abort(self: Box<Self>) -> Result<(), PrintError> {
        let Some(id) = self.id else {
            return Ok(());
        };
        self.svc
            .call(move |l| {
                l.borrow_mut().with(|c| c.splf_close(id, true))?;
                let found = l
                    .borrow_mut()
                    .with(|c| c.splf_list(String::new(), String::new()))
                    .into_iter()
                    .find(|a| a.id == id);
                let Some(a) = found else {
                    return Ok(());
                };
                let line = format!(
                    "dltsplf FILE({}) JOB({:06}/{}/{}) SPLNBR({})",
                    a.name, a.job.2, a.job.1, a.job.0, a.number
                );
                let out = l.borrow_mut().with(|c| c.exec(line));
                if out.text.trim_start().starts_with("CPF") {
                    return Err(out.text);
                }
                Ok(())
            })
            .ok_or(PrintError::Other(String::new()))?
            .map_err(other)
    }
}

impl Printer for JobPrinter {
    // The file is the signed-on profile's `QPRTJOB`'s: the service takes
    // the job's own profile (`netsvr_splf_open`), not this name.
    fn open(
        &self,
        _user: &str,
        queue: &OutQueue,
        document: &str,
    ) -> Result<Box<dyn PrintJob>, PrintError> {
        let share = self
            .shares
            .iter()
            .find(|s| s.queue == *queue)
            .cloned()
            .unwrap_or(Spooling {
                queue: queue.clone(),
                kind: SpoolType::UserAscii,
                prtf: String::new(),
            });
        let name = share
            .prtf
            .rsplit('/')
            .next()
            .filter(|n| !n.is_empty())
            .unwrap_or("QPRINT")
            .to_string();
        let lib = if share.queue.library.is_empty() {
            "*LIBL"
        } else {
            share.queue.library.as_str()
        };
        let attrs = SplfAttrs {
            name,
            usrdta: document.chars().take(10).collect(),
            copies: 1,
            outq: format!("{lib}/{}", share.queue.name),
            prtf: share.prtf.clone(),
            ..SplfAttrs::default()
        };
        Ok(Box::new(Job {
            svc: self.svc.clone(),
            attrs,
            kind: share.kind,
            id: None,
        }))
    }
}
