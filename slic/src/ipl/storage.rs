// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The object store at IPL: locate the ASP 1 unit (C1001070), open it
//! with WAL recovery or format a blank one (C1001080), bootstrap the
//! system libraries on first boot (C6001007), run the temporary ASP for
//! QTEMP, checkpoint on a timer, and close at power down (D1001010).

use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use blockdev::{BlockDev, LinuxBlockDev, MemBlockDev};
use qsname::Name;
use store::asp::superblock::FLAG_RCLSTG_REQUIRED;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::{FormatParams, Recovery, Store, StoreConfig, StoreError};

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
    /// The unit's device node.
    pub device: PathBuf,
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

/// C1001070 and C1001080: find the ASP 1 partition, open it or format
/// it when blank. Without a unit, a D IPL continues without a store;
/// any other IPL halts.
pub fn locate_and_open(system: &mut System) -> Result<(), &'static src::Src> {
    let units = diskimg::locate::partitions_of_type(diskimg::ASP1_TYPE_GUID).unwrap_or_default();
    let Some(path) = units.into_iter().next() else {
        if system.boot.ipl_d {
            system
                .out
                .serial("slic: no ASP 1 unit; IPL type D continues without an object store\n");
            return Ok(());
        }
        system.out.serial("slic: no ASP 1 unit found\n");
        return Err(&src::ASP_NOT_FOUND);
    };
    let dev = match LinuxBlockDev::open(&path) {
        Ok(d) => d,
        Err(e) => {
            system
                .out
                .serial(&format!("slic: open {}: {e}\n", path.display()));
            return Err(&src::ASP_NOT_FOUND);
        }
    };
    system.out.serial(&format!(
        "slic: ASP 1 unit {} ({} MiB{})\n",
        path.display(),
        dev.blocks() * 4096 / (1024 * 1024),
        if dev.is_direct() { "" } else { ", buffered" }
    ));
    system.emit(&src::ASP_LOCATED);

    let blank = {
        let mut page = blockdev::AlignedBuf::new(1);
        dev.read(0, &mut page).is_ok() && page.iter().all(|&b| b == 0)
    };
    let devs: Vec<Box<dyn BlockDev>> = vec![Box::new(dev)];
    let (store, recovery, fresh) = if blank {
        system.out.serial("slic: ASP 1 unit is blank; formatting\n");
        let params = FormatParams {
            asp_number: 1,
            asp_uuid: random_bytes(),
            unit_uuids: vec![random_bytes()],
            wal_pages: WAL_PAGES,
            now: now_unix(),
        };
        match Store::format(devs, &params, config()) {
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
    let sb = store.asp().superblock();
    let qsys = (!fresh && !sb.qsys.is_none()).then_some(sb.qsys);
    system.storage = Some(Storage {
        store,
        qsys,
        fresh,
        recovery,
        device: path,
    });
    Ok(())
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

/// QSYS plus the system libraries, then a checkpoint.
fn bootstrap(store: &mut Store, now: u64, system_name: &str) -> store::Result<ObjectId> {
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

/// Time-based checkpoints, from the run loop.
pub fn tick(system: &mut System) {
    let now = now_unix();
    if let Some(s) = system.storage.as_mut()
        && let Err(e) = s.store.tick(now)
    {
        system.out.serial(&format!("slic: checkpoint: {e}\n"));
    }
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
