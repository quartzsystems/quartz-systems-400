// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The export table as the daemons see it (`nfs::server::ExportTable`):
//! the service's table (`NfsOp::Exports`), each entry parsed, its root's
//! file ID found, its host lists matched through `/etc/netgroup` and
//! the system's host names. [`refresh`] reads it again when the table's
//! generation moves; a daemon's thread runs it once a second.

use std::fmt;
use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;

use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use ifs::ops::audit::FileId;
use ifs::ops::flags::{O_CCSID, O_RDONLY, O_TEXTDATA};
use nfs::exports::hosts::Client;
use nfs::exports::{DataCcsid, Export, Netgroups, PathCcsid, Resolver, file};
use nfs::server::{Anon, ExportTable, ExportView, HostOpts};
use stg::nfs::{NfsExport, NfsOp, NfsReply};

use super::host::Host;

/// Where the netgroups are.
pub const NETGROUP: &str = "/etc/netgroup";

/// How often the generation is asked.
pub const EVERY: Duration = Duration::from_secs(1);

type Names = Arc<dyn Resolver + Send + Sync>;

/// One export.
pub struct View {
    id: u32,
    root: FileId,
    export: Export,
    anon: Anon,
    groups: Arc<Netgroups>,
    names: Names,
}

impl fmt::Debug for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("View")
            .field("id", &self.id)
            .field("path", &self.export.path)
            .finish_non_exhaustive()
    }
}

impl View {
    fn access(&self, peer: IpAddr) -> nfs::exports::Access {
        let c = Client::new(peer, &*self.names);
        self.export.access_for(&c, &self.groups)
    }
}

/// A host option's CCSIDs: the service resolved `*ASCII` and
/// `*JOBCCSID`; were one left, 819 and 37 stand for them.
fn host_opts(h: &nfs::exports::HostOpt) -> HostOpts {
    HostOpts {
        data_ccsid: match h.data {
            DataCcsid::Binary => None,
            DataCcsid::Ascii => Some(819),
            DataCcsid::Job => Some(37),
            DataCcsid::Ccsid(n) => Some(n),
        },
        path_ccsid: match h.path {
            PathCcsid::Ascii => 819,
            PathCcsid::Job => 37,
            PathCcsid::Ccsid(n) => n,
        },
        sync: h.sync,
    }
}

impl ExportView for View {
    fn id(&self) -> u32 {
        self.id
    }
    fn path(&self) -> &str {
        &self.export.path
    }
    fn root_id(&self) -> FileId {
        self.root
    }
    fn allows(&self, peer: IpAddr) -> bool {
        self.access(peer).allowed
    }
    fn writable(&self, peer: IpAddr) -> bool {
        self.access(peer).write
    }
    fn root_access(&self, peer: IpAddr) -> bool {
        self.access(peer).root
    }
    fn anon(&self) -> Anon {
        self.anon
    }
    fn nosuid(&self) -> bool {
        self.export.options.nosuid
    }
    fn version_allowed(&self, vers: u32) -> bool {
        let v = self.export.options.versions();
        match vers {
            3 => v.v3,
            4 => v.v4,
            _ => false,
        }
    }
    fn flavors(&self) -> Vec<i32> {
        self.export
            .options
            .flavors()
            .into_iter()
            .map(|f| i32::try_from(f.number()).unwrap_or(1))
            .collect()
    }
    fn host_opts(&self, peer: IpAddr) -> HostOpts {
        let c = Client::new(peer, &*self.names);
        // A host no HOSTOPT names: data as stored, names in UTF-8,
        // written before the answer (`*SYNC`).
        self.export.host_opt(&c, &self.groups).map_or(
            HostOpts {
                sync: true,
                ..HostOpts::default()
            },
            host_opts,
        )
    }
    fn groups(&self) -> Vec<String> {
        self.export
            .options
            .access
            .iter()
            .flatten()
            .map(ToString::to_string)
            .collect()
    }
}

/// The table the daemon's threads share.
pub struct JobExports {
    views: RwLock<(Option<u64>, Vec<Arc<View>>)>,
    names: Names,
}

impl fmt::Debug for JobExports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobExports").finish_non_exhaustive()
    }
}

impl JobExports {
    /// An empty table matching host names through `names`.
    #[must_use]
    pub fn new(names: Box<dyn Resolver + Send + Sync>) -> Arc<Self> {
        Arc::new(Self {
            views: RwLock::new((None, Vec::new())),
            names: Arc::from(names),
        })
    }

    /// The generation of the table held, `None` before the first read.
    #[must_use]
    pub fn generation(&self) -> Option<u64> {
        self.views.read().unwrap_or_else(PoisonError::into_inner).0
    }

    /// Replace the table: entries the service gave, as of `generation`.
    pub fn set(&self, generation: u64, exports: &[NfsExport], fs: &dyn IfsClient) {
        let groups = Arc::new(netgroups(fs));
        let mut views = Vec::new();
        for e in exports {
            match view(e, fs, &groups, &self.names) {
                Ok(v) => views.push(Arc::new(v)),
                Err(why) => eprintln!("qsjob: NFS: export {} not served: {why}", e.entry),
            }
        }
        *self.views.write().unwrap_or_else(PoisonError::into_inner) = (Some(generation), views);
    }
}

impl ExportTable for JobExports {
    fn by_id(&self, id: u32) -> Option<Arc<dyn ExportView>> {
        self.views
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .1
            .iter()
            .find(|v| v.id == id)
            .map(|v| Arc::clone(v) as Arc<dyn ExportView>)
    }
    fn all(&self) -> Vec<Arc<dyn ExportView>> {
        self.views
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .1
            .iter()
            .map(|v| Arc::clone(v) as Arc<dyn ExportView>)
            .collect()
    }
}

/// `/etc/netgroup`, none when it is missing or cannot be used (each
/// export then matches its names as hosts).
fn netgroups(fs: &dyn IfsClient) -> Netgroups {
    let Ok(fd) = fs.open(
        NETGROUP,
        OpenArgs::new(O_RDONLY | O_TEXTDATA | O_CCSID).conv(1208),
    ) else {
        return Netgroups::default();
    };
    let data = ifs::client::read_all(fs, fd);
    let _ = fs.close(fd);
    let text = String::from_utf8_lossy(&data.unwrap_or_default()).into_owned();
    Netgroups::parse(&text).unwrap_or_else(|e| {
        eprintln!("qsjob: NFS: {NETGROUP} not used: {e}");
        Netgroups::default()
    })
}

/// One entry of the service's table as a view.
fn view(
    e: &NfsExport,
    fs: &dyn IfsClient,
    groups: &Arc<Netgroups>,
    names: &Names,
) -> Result<View, String> {
    let export = file::entry(1, &e.entry).map_err(|x| x.to_string())?;
    let st = fs
        .stat(&export.path, true)
        .map_err(|x| x.text().to_string())?;
    let anon = match export.options.anon {
        None => Anon::Default,
        Some(-1) => Anon::Refuse,
        Some(_) => e.anon_uid.map_or(Anon::Refuse, Anon::Uid),
    };
    Ok(View {
        id: e.id,
        root: FileId {
            fs: st.vfs,
            ino: st.ino,
            generation: st.ino_gen,
        },
        export,
        anon,
        groups: Arc::clone(groups),
        names: Arc::clone(names),
    })
}

/// Read the table again when its generation moved (or was never read).
/// Whether it was read.
pub fn refresh<H: Host>(host: &H, table: &JobExports, fs: &dyn IfsClient) -> bool {
    if let Some(have) = table.generation()
        && let NfsReply::Generation { generation } = host.op(NfsOp::Generation)
        && generation == have
    {
        return false;
    }
    match host.op(NfsOp::Exports) {
        NfsReply::Table {
            generation,
            exports,
        } => {
            table.set(generation, &exports, fs);
            true
        }
        _ => false,
    }
}

/// Keep `table` current until `stop`: a thread of the daemon with a
/// client of its own.
pub fn keep_current<H: Host>(host: &H, table: &JobExports, stop: &AtomicBool) {
    let fs = match host.fs() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("qsjob: NFS: the export table's link: {e}");
            return;
        }
    };
    let mut waited = EVERY;
    while !stop.load(Ordering::Relaxed) {
        if waited >= EVERY {
            refresh(host, table, &fs);
            waited = Duration::ZERO;
        }
        std::thread::sleep(Duration::from_millis(100));
        waited += Duration::from_millis(100);
    }
}
