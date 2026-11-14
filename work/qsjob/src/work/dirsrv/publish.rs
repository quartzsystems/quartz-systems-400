// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QGLDPUBE` (users) and `QGLDPUBA` (printers) in `QSYSWRK`, as
//! `QDIRSRV`: the publishing agents (docs/DIRSRV.md §10.1). The job
//! turns `dirsrv::publish::Runner` every few seconds over the storage
//! service's configuration, objects and changes (requests 1570-1572),
//! and ends when its subsystem or `ENDJOB` says so. Which agent it runs
//! comes from the job's name.

use std::sync::Arc;
use std::time::Duration;

use dirsrv::publish::config::{self, Kind};
use dirsrv::publish::{
    Change, Changes, Cursor, Listing, Object, PrinterInfo, PublishSource, Runner, SourceConfig,
    Status, TcpConnect, UserInfo,
};
use ipc::chan::Chan;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use stg::dirpub::{PubChange, PubCursor, PubObject, PubStatus};
use stg::{Client, Shared};
use wm::ctl::{FromJob, Start};

use super::super::netsvc::Ender;

/// The role the publishing jobs start in.
pub const ROLE: &str = "DIRPUB";

/// QS/400's application ID for `QIBM_GLD_DIRSRV_CLIENT`.
const APP_ID: &str = "QS400_DIRECTORY_CLIENT";

/// How long the job waits between turns.
const TURN: Duration = Duration::from_secs(10);

/// How long a connection or an answer from the target may take.
const TIMEOUT: Duration = Duration::from_secs(30);

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The storage service as the agent's source.
struct LinkSource<'a> {
    link: &'a Shared,
}

impl PublishSource for LinkSource<'_> {
    fn config(&mut self) -> Result<SourceConfig, String> {
        let c = self.link.borrow_mut().with(Client::dirpub_config)?;
        let config = config::parse(c.text.as_bytes()).map_err(|e| e.to_string())?;
        let passwords = c
            .passwords
            .into_iter()
            .filter_map(|(a, p)| Kind::from_name(&a).map(|k| (k, p)))
            .collect();
        Ok(SourceConfig {
            config,
            passwords,
            system: c.system,
        })
    }

    fn list(&mut self, kind: Kind) -> Result<Listing, String> {
        let l = self
            .link
            .borrow_mut()
            .with(|l| l.dirpub_list(kind.name().into()))?;
        Ok(Listing {
            cursor: cursor(l.cursor),
            objects: l.objects.into_iter().map(object).collect(),
        })
    }

    fn changes(&mut self, kind: Kind, since: Cursor, status: &Status) -> Result<Changes, String> {
        let since = PubCursor {
            epoch: since.epoch,
            seq: since.seq,
        };
        let st = pub_status(status);
        let c = self
            .link
            .borrow_mut()
            .with(|l| l.dirpub_events(kind.name().into(), since, st))?;
        Ok(Changes {
            next: cursor(c.next),
            resync: c.resync,
            changes: c.changes.into_iter().map(|c| change(kind, c)).collect(),
        })
    }
}

const fn cursor(c: PubCursor) -> Cursor {
    Cursor {
        epoch: c.epoch,
        seq: c.seq,
    }
}

fn object(o: PubObject) -> Object {
    match o {
        PubObject::User { name, text, mail } => Object::User(UserInfo { name, text, mail }),
        PubObject::Printer {
            device,
            text,
            outq,
            make_model,
            uri,
        } => Object::Printer(PrinterInfo {
            device,
            text,
            outq,
            make_model,
            uri,
        }),
    }
}

fn change(kind: Kind, c: PubChange) -> Change {
    match c {
        PubChange::Created { object: o } => Change::Created(object(o)),
        PubChange::Changed { object: o } => Change::Changed(object(o)),
        PubChange::Deleted { name } => Change::Deleted(kind, name),
    }
}

fn pub_status(s: &Status) -> PubStatus {
    PubStatus {
        enabled: s.enabled,
        connected: s.connected,
        last_success: s.last_success,
        last_error: s.last_error.clone(),
        pending: s.pending,
        published: s.published,
        errors: s.errors,
        next_try: s.next_try,
    }
}

/// TLS toward the target: the shipped roots and the `*SYSTEM` store's
/// trusted CAs, presenting the certificate assigned to the directory
/// client's application ID when there is one.
fn client_tls(link: &Shared) -> Option<Arc<rustls::ClientConfig>> {
    let mut l = link.borrow_mut();
    let policy = crate::work::tls::policy(&mut l);
    let Ok(Ok(trust)) = l.cert_app(APP_ID.into()) else {
        return crate::work::tls::client_config(&mut l).ok();
    };
    let mut roots = rustls::RootCertStore::empty();
    for r in certs::roots::roots() {
        let _ = roots.add(CertificateDer::from(r.der.clone()));
    }
    for r in &trust.roots {
        let _ = roots.add(CertificateDer::from(r.clone()));
    }
    let identity = if trust.assigned && !trust.chain.is_empty() {
        certs::x509::CertInfo::from_der(&trust.chain[0])
            .ok()
            .map(|info| tls::config::Identity {
                label: trust.label.clone(),
                chain: trust
                    .chain
                    .iter()
                    .cloned()
                    .map(CertificateDer::from)
                    .collect(),
                key: Arc::new(PrivateKeyDer::Pkcs8(trust.key.clone().into())),
                info,
            })
    } else {
        None
    };
    match tls::config::client_config(
        &policy,
        roots,
        identity.as_ref(),
        tls::config::ServerAuthMode::Full,
    ) {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("qsjob: publishing TLS: {e}");
            None
        }
    }
}

/// The job: `QGLDPUBE` or `QGLDPUBA` by name.
pub fn run(mut ctl: Chan, link: &Shared, start: &Start) -> ! {
    let Some(kind) = Kind::from_job(&start.name) else {
        crate::start::end(&mut ctl, &format!("{} is not a publishing job", start.name))
    };
    let ender = Ender::new(&ctl, start.number);
    stg::remote::status(link, wm::job::Status::Timw, "");
    let mut connect = TcpConnect::new(client_tls(link), TIMEOUT);
    let mut runner = Runner::new(kind);
    loop {
        runner.tick(&mut LinkSource { link }, &mut connect, now());
        if ender.ended(link, TURN) {
            break;
        }
    }
    runner.close();
    let _ = ctl.send(&FromJob::Ending {
        reason: "ENDJOB".into(),
    });
    std::process::exit(0)
}
