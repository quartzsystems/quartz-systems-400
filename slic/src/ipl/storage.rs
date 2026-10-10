// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The object store at IPL: locate the ASP 1 units (C1001070), open
//! them with WAL recovery or format a blank one (C1001080), bootstrap the
//! system libraries on first boot (C6001007), run the temporary ASP for
//! QTEMP, checkpoint on a timer, and close at power down (D1001010).

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use blockdev::{BlockDev, LinuxBlockDev, MemBlockDev};
use licpart::{Device, LicPart};
use qsname::Name;
use store::asp::io::IoEvent;
use store::asp::superblock::FLAG_RCLSTG_REQUIRED;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::{
    FormatParams, MemJournal, Recovery, Resume, ResumeJournal, ResumeState, Store, StoreConfig,
    StoreError, UnitEvent,
};

use crate::ipl::system::{System, now_unix};
use crate::src;

/// WAL ring: 64 MiB.
const WAL_PAGES: u64 = 16384;
/// The temporary ASP: 64 MiB of memory.
const TEMP_PAGES: u64 = 16384;
/// Libraries every system has, created on first boot after QSYS.
pub const SYSTEM_LIBRARIES: &[&str] = &[
    "QGPL",
    "QUSRSYS",
    "QSYS2",
    "QRECOVERY",
    "QSPL",
    "QRCL",
    "QHLPSYS",
];

/// ASP 1 as opened for this IPL.
pub struct Storage {
    /// The store.
    pub store: Store,
    /// QSYS, once the object layer is initialized.
    pub qsys: Option<ObjectId>,
    /// Formatted during this IPL: the libraries still need creating.
    pub fresh: bool,
    /// What recovery did.
    pub recovery: Recovery,
    /// The units' device nodes, in the order they were found.
    pub devices: Vec<PathBuf>,
    /// A mirrored half being copied in the background.
    pub resume: Option<Resume>,
}

/// Chunks (1 MiB each) copied per step of a background resume.
const RESUME_BUDGET: u64 = 16;
/// Chunks per step of the synchronization at IPL.
const SYNC_BUDGET: u64 = 64;

/// The resume journal in the LIC partition, or in memory without one.
pub enum Journal<'a> {
    /// On the LIC partition.
    Lic(&'a mut LicPart<Box<dyn Device>>),
    /// In memory: a crash starts the copy over.
    Mem(MemJournal),
}

impl ResumeJournal for Journal<'_> {
    fn load(&mut self, unit_uuid: [u8; 16]) -> Option<ResumeState> {
        match self {
            Journal::Lic(part) => part
                .read_resume()
                .and_then(|b| ResumeState::decode(&b))
                .filter(|s| s.unit_uuid == unit_uuid),
            Journal::Mem(m) => m.load(unit_uuid),
        }
    }
    fn save(&mut self, state: &ResumeState) -> store::Result<()> {
        match self {
            Journal::Lic(part) => {
                part.write_resume(&state.encode())
                    .map_err(|e| StoreError::Journal {
                        what: e.to_string(),
                    })
            }
            Journal::Mem(m) => m.save(state),
        }
    }
    fn clear(&mut self) -> store::Result<()> {
        match self {
            Journal::Lic(part) => part.clear_resume().map_err(|e| StoreError::Journal {
                what: e.to_string(),
            }),
            Journal::Mem(m) => m.clear(),
        }
    }
}

/// The journal to use: the LIC partition when attached.
fn journal(lic: &mut Option<(LicPart<Box<dyn Device>>, licpart::Record)>) -> Journal<'_> {
    match lic.as_mut() {
        Some((part, _)) => Journal::Lic(part),
        None => Journal::Mem(MemJournal::default()),
    }
}

/// The temporary ASP (ASP 0) and its root library QTEMP.
pub struct Temp {
    /// The in-memory store.
    pub store: Store,
    /// QTEMP.
    pub qtemp: ObjectId,
}

fn config() -> StoreConfig {
    StoreConfig::default()
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    if let Ok(mut f) = fs::File::open("/dev/urandom") {
        let _ = f.read_exact(&mut b);
    }
    b
}

/// C1001070 and C1001080: find the ASP 1 partitions, open them, or
/// format a single blank one. Without a unit, a D IPL continues without
/// a store; any other IPL halts.
pub fn locate_and_open(system: &mut System) -> Result<(), &'static src::Src> {
    let paths = unit_candidates();
    if paths.is_empty() {
        if system.boot.ipl_d {
            system
                .out
                .serial("slic: no ASP 1 unit; IPL type D continues without an object store\n");
            return Ok(());
        }
        system.out.serial("slic: no ASP 1 unit found\n");
        return Err(&src::ASP_NOT_FOUND);
    }
    let mut devs: Vec<Box<dyn BlockDev>> = Vec::new();
    let mut opened: Vec<PathBuf> = Vec::new();
    for path in &paths {
        match LinuxBlockDev::open(path) {
            Ok(d) => {
                system.out.serial(&format!(
                    "slic: ASP 1 unit {} ({} MiB{})\n",
                    path.display(),
                    d.blocks() * 4096 / (1024 * 1024),
                    if d.is_direct() { "" } else { ", buffered" }
                ));
                devs.push(Box::new(d));
                opened.push(path.clone());
            }
            Err(e) => system
                .out
                .serial(&format!("slic: open {}: {e}\n", path.display())),
        }
    }
    if devs.is_empty() {
        return Err(&src::ASP_NOT_FOUND);
    }
    system.emit(&src::ASP_LOCATED);

    let blank = devs.len() == 1 && {
        let mut page = blockdev::AlignedBuf::new(1);
        devs[0].read(0, &mut page).is_ok() && page.iter().all(|&b| b == 0)
    };
    let (store, recovery, fresh) = if blank {
        system.out.serial("slic: ASP 1 unit is blank; formatting\n");
        let (model, serial) = diskimg::locate::identity(&opened[0]);
        system
            .out
            .serial(&format!("slic: unit 0 is {model} serial {serial}\n"));
        match Store::format(devs, &format_params(now_unix(), &serial), config()) {
            Ok(s) => (s, Recovery::default(), true),
            Err(e) => {
                system.out.serial(&format!("slic: format ASP 1: {e}\n"));
                system.emit(&src::RECOVERY_FAILED);
                return Err(&src::ASP_UNUSABLE);
            }
        }
    } else {
        match Store::open(devs, config()) {
            Ok((s, r)) => (s, r, false),
            Err(StoreError::ReleaseMismatch { store, running }) => {
                system.out.serial(&format!(
                    "slic: ASP 1 was created by {store}; this is {running}\n"
                ));
                system.emit(&src::RELEASE_MISMATCH);
                return Err(&src::ASP_WRONG_RELEASE);
            }
            Err(StoreError::UnitMissing { unit, what }) => {
                system.out.serial(&format!(
                    "slic: ASP 1 disk unit {unit} is missing: {what}\n"
                ));
                return Err(&src::ASP_NOT_FOUND);
            }
            Err(e) => {
                system.out.serial(&format!("slic: ASP 1 recovery: {e}\n"));
                system.emit(&src::RECOVERY_FAILED);
                return Err(&src::ASP_UNUSABLE);
            }
        }
    };
    system.out.serial(&format!(
        "slic: storage management recovery: {} transaction(s) replayed, generation {}\n",
        recovery.replayed,
        store.asp().superblock().generation
    ));
    system.emit(&src::RECOVERY);
    let mut store = store;
    report_events(system, &mut store, &recovery.events)?;
    let sb = store.asp().superblock();
    let qsys = (!fresh && !sb.qsys.is_none()).then_some(sb.qsys);
    system.storage = Some(Storage {
        store,
        qsys,
        fresh,
        recovery,
        devices: opened,
        resume: None,
    });
    Ok(())
}

/// What `open` found out about the units: a missing half is an
/// attention code, a stale half a note, a resuming half a
/// synchronization before the store is used.
fn report_events(
    system: &mut System,
    store: &mut Store,
    events: &[UnitEvent],
) -> Result<(), &'static src::Src> {
    for e in events {
        match *e {
            UnitEvent::Missing { unit } => {
                system.out.serial(&format!(
                    "slic: disk unit {unit} missing; its pair runs on one half
"
                ));
                system.emit(&src::MIRROR_MISSING);
            }
            UnitEvent::Stale { unit } => system.out.serial(&format!(
                "slic: disk unit {unit} is back with stale data; suspended until resumed
"
            )),
            UnitEvent::Resuming { unit } => {
                if let Err(e) = synchronize(system, store, unit) {
                    system.out.serial(&format!(
                        "slic: synchronizing unit {unit}: {e}
"
                    ));
                    system.emit(&src::RECOVERY_FAILED);
                    return Err(&src::ASP_UNUSABLE);
                }
            }
        }
    }
    Ok(())
}

/// C1001075: copy a resuming half from its partner before the store is
/// used, continuing from the journal after a crash.
fn synchronize(system: &mut System, store: &mut Store, unit: u16) -> store::Result<()> {
    system.emit(&src::MIRROR_SYNC);
    let mut journal = journal(&mut system.lic);
    let mut resume = store.resume_unit(unit, &mut journal)?;
    let start = resume.progress();
    system.out.serial(&format!(
        "slic: synchronizing unit {unit}: {} of {} chunks already copied\n",
        start.chunks_done, start.chunks
    ));
    let mut shown = start.percent / 10;
    loop {
        let p = store.resume_step(&mut resume, &mut journal, SYNC_BUDGET)?;
        if p.percent / 10 > shown || p.complete {
            shown = p.percent / 10;
            system.out.serial(&format!(
                "slic: synchronizing unit {unit}: {}%\n",
                p.percent
            ));
        }
        if p.complete {
            return Ok(());
        }
    }
}

/// Start copying a half in the background; `tick` carries on.
pub fn start_resume(system: &mut System, unit: u16) -> Result<String, String> {
    let System { storage, lic, .. } = system;
    let Some(s) = storage.as_mut() else {
        return Err("object store not open".into());
    };
    if s.resume.is_some() {
        return Err("a resume is already running".into());
    }
    let mut journal = journal(lic);
    let resume = s
        .store
        .resume_unit(unit, &mut journal)
        .map_err(|e| e.to_string())?;
    let p = resume.progress();
    s.resume = Some(resume);
    Ok(format!(
        "unit {unit} resuming in the background, {} of {} chunks done\n",
        p.chunks_done, p.chunks
    ))
}

/// Progress of the background resume, if one runs.
pub fn resume_progress(system: &System) -> Option<store::ResumeProgress> {
    system
        .storage
        .as_ref()?
        .resume
        .as_ref()
        .map(Resume::progress)
}

/// One step of the background resume. Returns whether one is running.
fn resume_step(system: &mut System) -> bool {
    let System {
        storage, lic, out, ..
    } = system;
    let Some(s) = storage.as_mut() else {
        return false;
    };
    let Some(resume) = s.resume.as_mut() else {
        return false;
    };
    let mut journal = journal(lic);
    match s.store.resume_step(resume, &mut journal, RESUME_BUDGET) {
        Ok(p) if p.complete => {
            out.serial(&format!("slic: unit {} resumed\n", p.unit));
            s.resume = None;
            false
        }
        Ok(_) => true,
        Err(e) => {
            out.serial(&format!("slic: resume: {e}\n"));
            s.resume = None;
            false
        }
    }
}

/// Report what the I/O layer did on its own.
fn report_io_events(system: &mut System) {
    let events = match system.storage.as_mut() {
        Some(s) => s.store.drain_io_events(),
        None => return,
    };
    for e in events {
        match e {
            IoEvent::Healed { page, unit } => system.out.serial(&format!(
                "slic: page {page} rewritten on unit {unit} from its mirror\n"
            )),
            IoEvent::UnitFailed { unit, what } => {
                system.out.serial(&format!(
                    "slic: disk unit {unit} taken out of service: {what}\n"
                ));
                system.emit(&src::MIRROR_FAILED);
            }
        }
    }
}

/// Whether the run loop should come back soon: a resume is running.
pub fn busy(system: &System) -> bool {
    system.storage.as_ref().is_some_and(|s| s.resume.is_some())
}

/// C6001005, C6001007, C6001008: the object layer. On a fresh store,
/// create QSYS and the system libraries.
pub fn open_objects(system: &mut System) -> Result<(), &'static src::Src> {
    let now = now_unix();
    let system_name = "QS400".to_string();
    // Work on the store first; the codes go out once the borrow ends.
    let outcome = {
        let Some(storage) = system.storage.as_mut() else {
            return Ok(());
        };
        if storage.fresh {
            match bootstrap(&mut storage.store, now, &system_name) {
                Ok(qsys) => {
                    storage.qsys = Some(qsys);
                    storage.fresh = false;
                    Ok(true)
                }
                Err(e) => Err(format!("initializing ASP 1: {e}")),
            }
        } else if storage.qsys.is_none() {
            Err("ASP 1 has no QSYS; the store is unusable".to_string())
        } else {
            Ok(false)
        }
    };
    let initialized = match outcome {
        Ok(i) => i,
        Err(msg) => {
            system.out.serial(&format!(
                "slic: {msg}
"
            ));
            return Err(&src::ASP_UNUSABLE);
        }
    };
    system.emit(&src::STORE_OPENED);
    if initialized {
        system.emit(&src::ASP_INITIALIZED);
    }
    let (rclstg, line) = {
        let storage = system.storage.as_ref().expect("checked above");
        let asp = storage.store.asp();
        (
            asp.superblock().flag(FLAG_RCLSTG_REQUIRED),
            format!(
                "slic: object store: {} free pages of {}, next serial {}
",
                asp.total_free(),
                asp.layout().total_pages(),
                asp.superblock().next_serial
            ),
        )
    };
    if rclstg {
        system.emit(&src::RCLSTG_REQUIRED);
    }
    system.out.serial(&line);
    Ok(())
}

/// Device nodes that may be units: every ASP 1 partition, and every
/// whole disk that starts with a unit header (a unit added as a whole
/// disk through the service tools).
pub fn unit_candidates() -> Vec<PathBuf> {
    let mut paths =
        diskimg::locate::partitions_of_type(diskimg::ASP1_TYPE_GUID).unwrap_or_default();
    for disk in diskimg::locate::whole_disks().unwrap_or_default() {
        if disk.has_qs400 {
            continue; // its units are partitions, listed above
        }
        if has_unit_header(&disk.dev) {
            paths.push(disk.dev);
        }
    }
    paths
}

/// Whether a device's first page is a verifiable unit header.
pub fn has_unit_header(path: &std::path::Path) -> bool {
    let Ok(dev) = LinuxBlockDev::open(path) else {
        return false;
    };
    let mut page = store::Page::zeroed();
    dev.read(0, page.bytes_mut()).is_ok()
        && page.verify_type(0, store::PageType::UnitHeader).is_ok()
        && store::asp::unit::UnitHeader::decode(page.body(), 0).is_ok()
}

/// Format parameters for a new ASP 1 with one unit.
pub fn format_params(now: u64, serial: &str) -> FormatParams {
    FormatParams {
        asp_number: 1,
        asp_uuid: random_bytes(),
        unit_uuids: vec![random_bytes()],
        unit_serials: vec![serial.to_string()],
        wal_pages: WAL_PAGES,
        now,
    }
}

/// QSYS plus the system libraries, then a checkpoint.
pub fn bootstrap(store: &mut Store, now: u64, system_name: &str) -> store::Result<ObjectId> {
    let qsys = {
        let mut t = store.begin()?;
        let qsys = ops::bootstrap(&mut t, now, system_name)?;
        for lib in SYSTEM_LIBRARIES {
            ops::create(
                &mut t,
                &CreateParams {
                    library: qsys,
                    name: Name::parse(lib).expect("valid"),
                    type_code: types::LIB,
                    attribute: String::new(),
                    text: format!("System library {lib}"),
                    owner: "QSYS".into(),
                    system: system_name.into(),
                    now,
                    by_system: true,
                },
            )?;
        }
        t.commit()?;
        qsys
    };
    store.asp_superblock_set_qsys(qsys);
    store.checkpoint()?;
    Ok(qsys)
}

/// The temporary ASP with QTEMP, fresh every IPL. `MIGRATE(phase5)`:
/// one QTEMP per job.
pub fn open_temp(system: &mut System) {
    let dev: Box<dyn BlockDev> = Box::new(Arc::new(MemBlockDev::new(TEMP_PAGES)));
    let params = FormatParams {
        asp_number: 0,
        asp_uuid: random_bytes(),
        unit_uuids: vec![random_bytes()],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: now_unix(),
    };
    let result = Store::format(vec![dev], &params, config()).and_then(|mut store| {
        let qtemp = {
            let mut t = store.begin()?;
            let q = ops::bootstrap_root(
                &mut t,
                now_unix(),
                "QS400",
                Name::parse("QTEMP").expect("valid"),
            )?;
            t.commit()?;
            q
        };
        Ok(Temp { store, qtemp })
    });
    match result {
        Ok(temp) => {
            system.out.serial(&format!(
                "slic: QTEMP in ASP 0 ({} MiB)\n",
                (TEMP_PAGES * 4096) >> 20
            ));
            system.temp = Some(temp);
        }
        Err(e) => system.out.serial(&format!("slic: temporary ASP: {e}\n")),
    }
}

/// From the run loop: a step of any background resume, the I/O layer's
/// events, and time-based checkpoints.
pub fn tick(system: &mut System) {
    resume_step(system);
    report_io_events(system);
    let now = now_unix();
    if let Some(s) = system.storage.as_mut()
        && let Err(e) = s.store.tick(now)
    {
        system.out.serial(&format!("slic: checkpoint: {e}\n"));
    }
}

/// Random bytes from the kernel, for new unit identities.
pub fn fresh_uuid() -> [u8; 16] {
    random_bytes()
}

/// D1001010: checkpoint and close the store, release the unit.
pub fn close(system: &mut System) {
    if let Some(storage) = system.storage.take() {
        match storage.store.close() {
            Ok(()) => system.emit(&src::STORE_CLOSED),
            Err(e) => system.out.serial(&format!("slic: close store: {e}\n")),
        }
    }
    system.temp = None;
}
