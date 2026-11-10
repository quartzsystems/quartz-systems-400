// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`NetServer`]: one `QZLSFILE` job's SMB server. It owns a current-thread
//! tokio runtime on a thread of its own, the library's `SmbServer` with
//! the job's shares, and the IFS thread; the job hands it sockets
//! ([`NetServer::serve_one`]), first the connection `QZLSSERVER` accepted
//! for it and later any reconnect routed back to it, and ends once
//! [`NetServer::idle_until_released`] returns.

use std::io;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread;
use std::time::Duration;

use smb_server::{Access, SessionView, Share, SmbError, SmbServer, TreeAdmission, TreeRequest};
use tokio::runtime::{Builder, Handle};
use tokio::sync::{Notify, oneshot};
use tracing::info;

use crate::ifs::{IfsShare, IfsWorker, LeaseLink, MakeClient, Observer, TextConversion};
use crate::print::drivers::{self, DRIVER_STORE, PRINT_DOLLAR};
use crate::print::spoolss::{SpoolPrinter, Spoolss};
use crate::print::{OutQueue, PrintShare, Printer};
use crate::signon::{SignOn, Verifier};

/// A file share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileShare {
    /// The share name (12 characters at most).
    pub name: String,
    /// The IFS directory it shares.
    pub path: String,
    /// Its description.
    pub text: String,
    /// Permission `*RDONLY` (else read/write).
    pub read_only: bool,
    /// Encryption required on it.
    pub encrypt: bool,
    /// Leases on its files (they let clients cache; see the module docs of
    /// [`crate`] for what other jobs' changes do to them).
    pub leases: bool,
    /// Its text conversion, when enabled.
    pub conversion: Option<TextConversion>,
}

impl FileShare {
    /// A read/write share of `path` with leases.
    #[must_use]
    pub fn new(name: &str, path: &str) -> Self {
        Self { name: name.into(), path: path.into(), text: String::new(), read_only: false, encrypt: false, leases: true, conversion: None }
    }
}

/// A print share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrintShareCfg {
    /// The share name.
    pub name: String,
    /// Its description.
    pub text: String,
    /// The output queue its files spool to.
    pub queue: OutQueue,
    /// The printer driver clients are told to use (the share's printer
    /// driver attribute).
    pub driver: String,
}

/// What the job serves with, from the server attributes and shares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetServerConfig {
    /// The server name (`NetBIOS` name, 15 characters).
    pub server_name: String,
    /// The domain (workgroup).
    pub domain: String,
    /// The server's description.
    pub text: String,
    /// Message signing required for every session.
    pub signing_required: bool,
    /// Encryption required for every session.
    pub encryption_required: bool,
    /// Sessions with no open file end after this.
    pub idle_timeout: Duration,
    /// The largest READ and WRITE (64 KiB to 8 MiB).
    pub max_read: u32,
    /// The largest WRITE.
    pub max_write: u32,
    /// The lowest dialect (0x0202, 0x0210, 0x0300, 0x0302, 0x0311).
    pub min_dialect: u16,
    /// How long a durable open waits for its client to reconnect.
    pub durable_timeout: Duration,
    /// The guest profile for unknown user names.
    pub guest: Option<String>,
    /// File shares.
    pub file_shares: Vec<FileShare>,
    /// Print shares.
    pub print_shares: Vec<PrintShareCfg>,
    /// The IFS directory the read-only `print$` share serves driver files
    /// from, with print shares; not served when it is not a directory.
    pub driver_store: Option<String>,
}

impl Default for NetServerConfig {
    fn default() -> Self {
        Self {
            server_name: "QS400".into(),
            domain: "WORKGROUP".into(),
            text: String::new(),
            signing_required: false,
            encryption_required: false,
            idle_timeout: Duration::from_mins(15),
            max_read: 1 << 20,
            max_write: 1 << 20,
            min_dialect: 0x0202,
            durable_timeout: smb_server::DEFAULT_DURABLE_HANDLE_TIMEOUT,
            guest: None,
            file_shares: Vec::new(),
            print_shares: Vec::new(),
            driver_store: Some(DRIVER_STORE.into()),
        }
    }
}

/// Why the server could not be built.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// The library refused the configuration.
    #[error(transparent)]
    Library(#[from] smb_server::BuildError),
    /// A file share's directory cannot be served.
    #[error("share {name}: {error}")]
    Share {
        /// The share.
        name: String,
        /// Why.
        error: SmbError,
    },
    /// The runtime or a thread could not be started.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The driver store at `root` and the read-only share of it, or `None`
/// when `root` is not a directory.
fn driver_store(handle: &Handle, io: &Arc<IfsWorker>, root: &str) -> Result<Option<(String, IfsShare)>, BuildError> {
    let is_dir = on_ifs(handle, io, root.to_owned(), (), |c, root, ()| {
        Ok(c.stat(&root, true).is_ok_and(|st| st.mode & ::ifs::ops::flags::S_IFMT == ::ifs::ops::flags::S_IFDIR))
    })?
    .unwrap_or(false);
    if !is_dir {
        return Ok(None);
    }
    let (tx, rx) = mpsc::channel();
    let (io2, path) = (io.clone(), root.to_owned());
    handle.spawn(async move {
        let _ = tx.send(IfsShare::new(io2, &path, true).await);
    });
    let share = rx
        .recv()
        .map_err(|_| io::Error::other("server thread gone"))?
        .map_err(|error| BuildError::Share { name: PRINT_DOLLAR.into(), error })?;
    Ok(Some((root.to_owned(), share)))
}

/// Runs `f` on the IFS thread from outside the runtime and waits for it;
/// `None` when the IFS call failed.
fn on_ifs<A, B, T>(
    handle: &Handle,
    io: &Arc<IfsWorker>,
    a: A,
    b: B,
    f: impl FnOnce(&dyn ::ifs::client::IfsClient, A, B) -> ::ifs::Result<T> + Send + 'static,
) -> Result<Option<T>, BuildError>
where
    A: Send + 'static,
    B: Send + 'static,
    T: Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let io2 = io.clone();
    handle.spawn(async move {
        let _ = tx.send(io2.call(move |c| f(c, a, b)).await);
    });
    Ok(rx.recv().map_err(|_| io::Error::other("server thread gone"))?.ok())
}

/// The library's admission over a shared one.
struct Admit(Arc<dyn TreeAdmission>);

impl TreeAdmission for Admit {
    fn admit(&self, req: &TreeRequest<'_>) -> bool {
        self.0.admit(req)
    }
}

/// One job's SMB server.
pub struct NetServer {
    smb: Arc<SmbServer>,
    handle: Handle,
    live: Arc<AtomicUsize>,
    changed: Arc<Notify>,
    profile: Arc<Mutex<Option<String>>>,
    stop: Option<oneshot::Sender<()>>,
    driver: Option<thread::JoinHandle<()>>,
    leases: Arc<LeaseLink>,
    /// Ends the thread polling for lease break orders.
    stopping: Arc<AtomicBool>,
    /// That thread, joined when the server ends.
    poller: Mutex<Option<thread::JoinHandle<()>>>,
}

impl NetServer {
    /// Builds the server: the IFS thread over `ifs`, the shares, sign-on
    /// through `signon` (the library's own logon throttle is off: the
    /// service counts `QMAXSIGN`), print files through `printer`.
    ///
    /// # Errors
    ///
    /// A share root that is not a directory, a configuration the library
    /// refuses, or no runtime.
    pub fn build(cfg: &NetServerConfig, ifs: MakeClient, signon: Arc<dyn SignOn>, printer: &Arc<dyn Printer>) -> Result<Self, BuildError> {
        Self::build_admitted(cfg, ifs, signon, printer, None)
    }

    /// [`NetServer::build`] with every tree connect admitted by
    /// `admission` first (the job counts a share's users across jobs).
    ///
    /// # Errors
    ///
    /// As [`NetServer::build`].
    pub fn build_admitted(
        cfg: &NetServerConfig,
        ifs: MakeClient,
        signon: Arc<dyn SignOn>,
        printer: &Arc<dyn Printer>,
        admission: Option<Arc<dyn TreeAdmission>>,
    ) -> Result<Self, BuildError> {
        let rt = Builder::new_current_thread().enable_all().build()?;
        let handle = rt.handle().clone();
        let (stop, stopped) = oneshot::channel::<()>();
        let driver = thread::Builder::new().name("netserver".into()).spawn(move || {
            let _ = rt.block_on(stopped);
        })?;
        let io = Arc::new(IfsWorker::start(ifs));
        let leases = LeaseLink::new(io.clone());
        let profile: Arc<Mutex<Option<String>>> = Arc::default();
        let mut b = SmbServer::builder()
            .listen("127.0.0.1:0".parse().expect("address"))
            .logon_verifier(Verifier::new(signon, cfg.guest.clone(), profile.clone()))
            .netbios_name(cfg.server_name.clone())
            .domain(cfg.domain.clone())
            .comment(cfg.text.clone())
            .require_signing(cfg.signing_required)
            .encrypt_data(cfg.encryption_required)
            .idle_timeout(cfg.idle_timeout)
            .max_read_size(cfg.max_read)
            .max_write_size(cfg.max_write)
            .min_dialect(cfg.min_dialect)
            .durable_handle_timeout(cfg.durable_timeout)
            .logon_throttle(0, Duration::from_secs(1))
            .lease_observer(Observer(leases.clone()));
        if let Some(a) = admission {
            b = b.tree_admission(Admit(a));
        }
        for s in &cfg.file_shares {
            let (tx, rx) = mpsc::channel();
            let (io2, path, ro, conv) = (io.clone(), s.path.clone(), s.read_only, s.conversion.clone());
            // Watches too go through the link; a share without leases asks
            // for none (the library's `Share::leases`).
            let link = leases.clone();
            handle.spawn(async move {
                let share = IfsShare::new(io2, &path, ro).await;
                let _ = tx.send(share.map(|sh| {
                    let sh = sh.with_leases(link);
                    match conv {
                        Some(t) => sh.with_conversion(t),
                        None => sh,
                    }
                }));
            });
            let backend = rx
                .recv()
                .map_err(|_| io::Error::other("server thread gone"))?
                .map_err(|error| BuildError::Share { name: s.name.clone(), error })?;
            let access = if s.read_only { Access::Read } else { Access::ReadWrite };
            let mut share = Share::new(s.name.clone(), backend)
                .any_user(access)
                .remark(s.text.clone())
                .named_streams(false)
                .extended_attributes(false)
                .leases(s.leases);
            if s.encrypt {
                share = share.encrypt_data();
            }
            b = b.share(share);
        }
        for p in &cfg.print_shares {
            let backend = PrintShare::new(printer.clone(), p.queue.clone(), profile.clone());
            b = b.share(Share::new(p.name.clone(), backend).print().any_user(Access::ReadWrite).remark(p.text.clone()));
        }
        if !cfg.print_shares.is_empty() {
            let store = match &cfg.driver_store {
                Some(root) => driver_store(&handle, &io, root)?,
                None => None,
            };
            let mut printers = Vec::new();
            for p in &cfg.print_shares {
                let driver_files = match &store {
                    Some((root, _)) => on_ifs(&handle, &io, root.clone(), p.driver.clone(), |c, root, driver| {
                        Ok(drivers::find(c, &root, &driver))
                    })?
                    .unwrap_or_default(),
                    None => Vec::new(),
                };
                printers.push(SpoolPrinter {
                    name: p.name.clone(),
                    text: p.text.clone(),
                    queue: p.queue.clone(),
                    driver: p.driver.clone(),
                    driver_files,
                });
            }
            if let Some((_, backend)) = store {
                b = b.share(Share::new(PRINT_DOLLAR, backend).any_user(Access::Read).named_streams(false).extended_attributes(false));
            }
            b = b.print_service(Spoolss::new(printer.clone(), printers));
        }
        let smb = Arc::new(b.build()?);
        Ok(Self {
            smb,
            handle,
            live: Arc::default(),
            changed: Arc::new(Notify::new()),
            profile,
            stop: Some(stop),
            driver: Some(driver),
            leases,
            stopping: Arc::default(),
            poller: Mutex::default(),
        })
    }

    /// Puts the job's leases in the IFS's lease table, so other jobs'
    /// changes break them (`docs/NETSERVER.md` section 6): from now on
    /// each lease is reserved there before it is granted, and a thread
    /// of its own polls for break orders over the client `orders` makes
    /// (a second link of the job). Call it once, before serving. Without
    /// it a job's leases are broken only by its own opens.
    ///
    /// # Errors
    ///
    /// The thread could not be started.
    pub fn start_leases(&self, orders: MakeClient) -> io::Result<()> {
        let (smb, handle, link, stop) = (self.smb.clone(), self.handle.clone(), self.leases.clone(), self.stopping.clone());
        link.start();
        let poller = thread::Builder::new().name("netserver-leases".into()).spawn(move || {
            let client = orders();
            link.serve_orders(&*client, &stop, |tag, keep, link| {
                let smb = smb.clone();
                handle.spawn(async move {
                    if !smb.break_leases(tag, keep).await {
                        link.none_open(tag);
                    }
                });
            });
        })?;
        *self.poller.lock().unwrap_or_else(PoisonError::into_inner) = Some(poller);
        Ok(())
    }

    /// Serves one connection until it closes. May be called from several
    /// threads at once (a reconnect routed here while another connection
    /// is served).
    ///
    /// # Errors
    ///
    /// The socket's, or the connection's end in error.
    pub fn serve_one(&self, stream: TcpStream) -> io::Result<()> {
        let peer = stream.peer_addr()?;
        stream.set_nonblocking(true)?;
        let (tx, rx) = mpsc::channel();
        let (smb, live, changed) = (self.smb.clone(), self.live.clone(), self.changed.clone());
        live.fetch_add(1, Ordering::SeqCst);
        self.handle.spawn(async move {
            let r = match tokio::net::TcpStream::from_std(stream) {
                Ok(s) => smb.serve_connection(s, peer).await,
                Err(e) => Err(e),
            };
            live.fetch_sub(1, Ordering::SeqCst);
            changed.notify_waiters();
            let _ = tx.send(r);
        });
        rx.recv().unwrap_or_else(|_| Err(io::Error::other("server stopped")))
    }

    /// Serves `stream` without waiting for it to close. It counts as served
    /// before this returns, so a [`NetServer::release_if_idle`] on the same
    /// thread afterwards cannot release the job under it.
    ///
    /// # Errors
    ///
    /// The socket's.
    pub fn serve_detached(&self, stream: TcpStream) -> io::Result<()> {
        let peer = stream.peer_addr()?;
        stream.set_nonblocking(true)?;
        let (smb, live, changed) = (self.smb.clone(), self.live.clone(), self.changed.clone());
        live.fetch_add(1, Ordering::SeqCst);
        self.handle.spawn(async move {
            let r = match tokio::net::TcpStream::from_std(stream) {
                Ok(s) => smb.serve_connection(s, peer).await,
                Err(e) => Err(e),
            };
            if let Err(e) = r {
                tracing::debug!(error = %e, "connection ended in error");
            }
            live.fetch_sub(1, Ordering::SeqCst);
            changed.notify_waiters();
        });
        Ok(())
    }

    /// When no connection is served and no durable open waits for a
    /// reconnect, closes what is left and answers true: the job may end
    /// (as [`NetServer::idle_until_released`], without waiting).
    #[must_use]
    pub fn release_if_idle(&self) -> bool {
        let (smb, live) = (self.smb.clone(), self.live.clone());
        let (tx, rx) = mpsc::channel();
        self.handle.spawn(async move {
            let idle = live.load(Ordering::SeqCst) == 0 && smb.disconnected_opens() == 0;
            if idle {
                smb.finish().await;
            }
            let _ = tx.send(idle);
        });
        let idle = rx.recv().unwrap_or(true);
        if idle {
            info!("NetServer job released");
        }
        idle
    }

    /// Blocks until no connection is served and no durable open waits for
    /// a reconnect, then closes what is left: the job may end. A
    /// [`NetServer::serve_one`] after this returns serves without durable
    /// opens being kept for it afterwards.
    pub fn idle_until_released(&self) {
        let (smb, live, changed) = (self.smb.clone(), self.live.clone(), self.changed.clone());
        let (tx, rx) = mpsc::channel();
        self.handle.spawn(async move {
            loop {
                let wake = changed.notified();
                if live.load(Ordering::SeqCst) == 0 && smb.disconnected_opens() == 0 {
                    break;
                }
                let _ = tokio::time::timeout(Duration::from_millis(200), wake).await;
            }
            smb.finish().await;
            let _ = tx.send(());
        });
        let _ = rx.recv();
        info!("NetServer job released");
    }

    /// The `ClientGuids` of the connections served and of the clients whose
    /// durable opens wait here, for `QZLSSERVER`'s routing table.
    #[must_use]
    pub fn client_guids(&self) -> Vec<[u8; 16]> {
        let smb = self.smb.clone();
        let (tx, rx) = mpsc::channel();
        self.handle.spawn(async move {
            let _ = tx.send(smb.client_guids().await);
        });
        rx.recv().unwrap_or_default()
    }

    /// The sessions served now, with their trees and open counts.
    #[must_use]
    pub fn sessions(&self) -> Vec<SessionView> {
        let smb = self.smb.clone();
        let (tx, rx) = mpsc::channel();
        self.handle.spawn(async move {
            let _ = tx.send(smb.sessions().await);
        });
        rx.recv().unwrap_or_default()
    }

    /// Ends session `id` (`QZLSENSS`): its trees and opens close. False
    /// when no connection holds it.
    #[must_use]
    pub fn end_session(&self, id: u64) -> bool {
        let smb = self.smb.clone();
        let (tx, rx) = mpsc::channel();
        self.handle.spawn(async move {
            let _ = tx.send(smb.end_session(id).await);
        });
        rx.recv().unwrap_or(false)
    }

    /// Durable opens waiting for their client to reconnect.
    #[must_use]
    pub fn disconnected_opens(&self) -> usize {
        self.smb.disconnected_opens()
    }

    /// Connections being served now.
    #[must_use]
    pub fn connections(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }

    /// The profile signed on in this job, once one has.
    #[must_use]
    pub fn profile(&self) -> Option<String> {
        self.profile.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Stops serving: every connection closes once its current request is
    /// answered (`ENDTCPSVR`).
    pub fn shutdown(&self) {
        self.smb.shutdown();
    }
}

impl Drop for NetServer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        // The job's leases and watches in the IFS go with this server, and
        // its poll returns, before a later server of the job polls: news
        // for that one never reaches this one.
        if let Some(poller) = self.poller.lock().unwrap_or_else(PoisonError::into_inner).take() {
            self.leases.reset();
            let _ = poller.join();
        }
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(d) = self.driver.take() {
            let _ = d.join();
        }
    }
}
