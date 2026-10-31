// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SAVSYS` (docs/RECOVERY.md): the Licensed Internal Code of both
//! slots, every installed product's load (the operating system first),
//! the security data and the configuration, written in the install
//! media layout (`instmedia::savsys`) to any tape or optical device the
//! device layer opens (a virtual device over its loaded image catalog,
//! a media library, a real drive), so the installer can reinstall the
//! system from it. And the recovery's restores: the security data and
//! the configuration, which the operating system install runs from such
//! media, then the user libraries and the IFS from the tape files after
//! the system save, once the storage service is up.

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};

use cmd::block::Parms;
use instmedia::Payload;
use instmedia::savsys::{Image, TapeWrite, write_optical, write_tape};
use medio::MediaError;
use saverst::{ObjSel, RestoreOptions, SaveOptions, SaveTarget};
use savfmt::{Reader, SaveKind, Writer};
use sec::profile::SpecialAuth;
use stg::proto::{CmdMsg, CmdReply};
use store::obj::JobHandle;
use tape::volume::VolumeKind;
use tape::{TapeTransport, VolumeProvider};
use wm::obj::devmed::MediaKind;

use super::direct::failed;
use super::savcmd::{CFG_TYPES, SvcHooks, header_with, values};
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::media::drive::{self, TapeJob};
use crate::stgsvc::media::{self, catalogs, operator, stream};

/// `SAVSYS` completed (Quartz's choice in IBM's range; see
/// `docs/SVC_OPEN_QUESTIONS.md`).
const SAVED_SYSTEM: &str = "CPC3703";
/// The device is not one `SAVSYS` can write.
const NO_DEVICE: &str = "CPF9814";
/// The save failed.
const NOT_SAVED: &str = "CPF3793";
/// An optical file is in the way.
const OPT_EXISTS: &str = "OPT1260";

/// Run `SAVSYS`, or `None` for another command.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    (name == "SAVSYS").then(|| savsys(system, parms))
}

fn read_slots() -> Result<(Vec<u8>, Vec<u8>), String> {
    let lic = licpart::locate().ok();
    let (node, _) = diskimg::locate::esp(lic.as_deref()).map_err(|e| format!("the ESP: {e}"))?;
    let read = |path: &str| -> Result<Vec<u8>, String> {
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&node)
            .map_err(|e| format!("{}: {e}", node.display()))?;
        diskimg::fat::read_file_at(f, path).map_err(|e| format!("{path} on the ESP: {e}"))
    };
    let a = read(crate::platform::efivar::PATH_A)?;
    // A system installed before slot B existed holds one image.
    let b = read(crate::platform::efivar::PATH_B).unwrap_or_else(|_| a.clone());
    Ok((a, b))
}

fn security_stream(system: &mut System) -> Result<Vec<u8>, String> {
    let head = header_of(system, SaveKind::Security);
    let storage = system.storage.as_mut().ok_or("CPF3700 object store not open")?;
    let qsys = storage.qsys.ok_or("CPF3700 object store not initialized")?;
    let hooks = SvcHooks::new(qsys);
    let mut w = Writer::new(Vec::new(), &head).map_err(|e| e.to_string())?;
    let store = &mut storage.store;
    let rep = sec::guard::as_system(|| {
        saverst::secdta::save_security(store, &hooks, qsys, now_unix(), &mut w)
    })
    .map_err(|e| e.to_string())?;
    w.finish(u32::try_from(rep.not_saved()).unwrap_or(0))
        .map(|(_, b)| b)
        .map_err(|e| e.to_string())
}

fn config_stream(system: &mut System) -> Result<Vec<u8>, String> {
    let head = header_of(system, SaveKind::Config);
    let jobh = JobHandle(u64::from(system.current_job));
    let storage = system.storage.as_mut().ok_or("CPF3700 object store not open")?;
    let qsys = storage.qsys.ok_or("CPF3700 object store not initialized")?;
    let hooks = SvcHooks::new(qsys);
    let mut w = Writer::new(Vec::new(), &head).map_err(|e| e.to_string())?;
    let opts = SaveOptions {
        select: CFG_TYPES
            .iter()
            .map(|t| ObjSel {
                name: "*ALL".into(),
                type_code: Some(*t),
            })
            .collect(),
        omit: Vec::new(),
        library_object: false,
        skip_journaled: false,
        update_history: false,
        save_file_data: false,
        spool: false,
        target_savf: None,
        private_auth: true,
        changed_since: None,
        now: now_unix(),
        target: SaveTarget {
            command: "SAVSYS".into(),
            device: "*SAVSYS".into(),
            ..SaveTarget::default()
        },
    };
    let rep = saverst::save_library(&mut storage.store, jobh, &hooks, qsys, "QSYS", &opts, &mut w)
        .map_err(|e| e.to_string())?;
    w.finish(u32::try_from(rep.not_saved()).unwrap_or(0))
        .map(|(_, b)| b)
        .map_err(|e| e.to_string())
}

fn header_of(system: &System, kind: SaveKind) -> savfmt::MediaHeader {
    header_with(system, "SAVSYS", savfmt::Compression::None, kind)
}

/// The payload of a system save: what `SAVSYS` writes. `omit` holds
/// `*CFG` and `*SECDTA` when those are left out.
pub fn image(system: &mut System, volume: &str, omit: &[String]) -> Result<Image, String> {
    let (a, b) = read_slots()?;
    let mut files = vec![(Payload::LicA, a), (Payload::LicB, b)];
    let mut products = super::lpcmd::installed_products(system);
    products.sort_by_key(|p| (p != instmedia::layout::OS_PRODUCT, p.clone()));
    if !products.iter().any(|p| p == instmedia::layout::OS_PRODUCT) {
        return Err("the operating system's product QS00SS1 is not installed".into());
    }
    for p in products {
        let s = super::lpcmd::product_stream(system, &p)?;
        files.push((Payload::Product(p), s));
    }
    if !omit.iter().any(|o| o == "*SECDTA") {
        files.push((Payload::SecData, security_stream(system)?));
    }
    if !omit.iter().any(|o| o == "*CFG") {
        files.push((Payload::Config, config_stream(system)?));
    }
    Ok(Image::new(release::RELEASE, volume, true, files, None))
}

/// The install media error as the device layer reports it.
fn media_error(e: instmedia::Error) -> MediaError {
    match e {
        instmedia::Error::Tape(t) => MediaError::Tape(t),
        instmedia::Error::Optical(o) => MediaError::Optical(o),
        e => MediaError::State(e.to_string()),
    }
}

/// The mounted tape's volume identifier; a blank or unlabeled volume
/// is refused before anything is saved (`INZTAP` first, as IBM i asks).
struct Ident;

impl TapeJob for Ident {
    type Out = String;
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, _: &mut dyn VolumeProvider<D>) -> Result<String, MediaError> {
        match tape::volume::identify(d)? {
            VolumeKind::Labeled { vol1, .. } => Ok(vol1.volume_id),
            _ => Err(tape::TapeError::NotLabeled.into()),
        }
    }
}

/// The system save onto a tape.
struct SysJob<'a> {
    img: &'a Image,
    w: TapeWrite,
}

impl TapeJob for SysJob<'_> {
    type Out = u32;
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, _: &mut dyn VolumeProvider<D>) -> Result<u32, MediaError> {
        write_tape(d, self.img, &self.w).map_err(media_error)
    }
}

/// Write `img` on the tape in `m.dev`. Files that have not expired are
/// asked about (`CPA4036`) unless `CLEAR` says otherwise.
fn to_tape(system: &mut System, m: &stream::Media, img: &Image, vol: &str) -> Result<String, String> {
    let mut w = TapeWrite {
        today: stream::today(),
        expires: m.expires,
        allow_active: m.clear,
        end_opt: m.end_opt,
    };
    loop {
        let mut job = SysJob { img, w };
        match drive::with_tape(system, &m.dev, &m.vols, true, &mut job) {
            Err(e) if e.starts_with(operator::ACTIVE_FILE) && !w.allow_active => {
                let a = operator::inquiry(system, operator::ACTIVE_FILE, &[(&m.dev, 10)], "C");
                if a.is("I") {
                    w.allow_active = true;
                    continue;
                }
                return Err(format!("{} {e}; {}", stream::SAVE_ENDED, a.shown()));
            }
            Err(e) => return Err(e),
            Ok(n) => return Ok(format!("{n} files on tape volume {vol}")),
        }
    }
}

/// Write `img` on the optical volume `vol` of `m.dev`. A system save
/// already there is asked about (`OPT1260`) unless `CLEAR` says
/// otherwise.
fn to_optical(system: &mut System, m: &stream::Media, img: &Image, vol: &str) -> Result<String, String> {
    let set = media::volume_set();
    let opt = |e: optical::Error| format!("{} {e}", e.msgid());
    let there = set.with(vol, |v| Ok(v.stat(optical::manifest::PATH).is_ok())).map_err(opt)?;
    if there && !m.clear {
        let a = operator::inquiry(system, OPT_EXISTS, &[(optical::manifest::PATH, 64), (vol, 32)], "C");
        if !a.is("G") {
            return Err(format!(
                "{} {} is already on volume {vol}; {}",
                stream::SAVE_ENDED,
                optical::manifest::PATH,
                a.shown()
            ));
        }
    }
    set.with(vol, |v| Ok(write_optical(v, img, now_unix())))
        .map_err(opt)?
        .map_err(|e| match media_error(e) {
            MediaError::Optical(o) => opt(o),
            e => format!("{} {e}", e.msgid()),
        })?;
    Ok(format!("{} files on optical volume {vol}", img.files.len() + 1))
}

fn savsys(system: &mut System, parms: &Parms) -> CmdReply {
    const NAME: &str = "SAVSYS";
    if let Err(e) = crate::stgsvc::security::require(SpecialAuth::SAVSYS, "save the system") {
        return failed(NAME, &format!("CPF2189 {e}"));
    }
    let dev = values(parms, "DEV").first().cloned().unwrap_or_default();
    if !stream::is_device(&dev) {
        return failed(NAME, &format!("{NO_DEVICE} device {dev} not found"));
    }
    let m = stream::of(parms, &dev);
    let t = match drive::target(system, &m.dev) {
        Ok(t) => t,
        Err(e) => return failed(NAME, &e),
    };
    let optical = t.kind == MediaKind::Optical;
    // The volume first: nothing is built for a device that cannot take it.
    let vol = if optical {
        drive::optical(system, &m.dev, m.vols.first().map(String::as_str)).map(|(_, v)| v)
    } else {
        drive::with_tape(system, &m.dev, &m.vols, true, &mut Ident)
    };
    let vol = match vol {
        Ok(v) => v,
        Err(e) => return failed(NAME, &e),
    };
    let omit = values(parms, "OMIT");
    let img = match image(system, &vol, &omit) {
        Ok(i) => i,
        Err(e) => return failed(NAME, &format!("{NOT_SAVED} {e}")),
    };
    let wrote = if optical {
        to_optical(system, &m, &img, &vol)
    } else {
        to_tape(system, &m, &img, &vol)
    };
    match wrote {
        Ok(what) => {
            let mut text = String::new();
            for (p, d) in &img.files {
                let _ = writeln!(text, "{} {} bytes", p.path(), d.len());
            }
            let _ = writeln!(text, "{SAVED_SYSTEM} system saved to {}: {what}", m.dev);
            CmdReply {
                text,
                messages: vec![CmdMsg {
                    msgid: SAVED_SYSTEM.into(),
                    kind: 4,
                    data: cmd::error::data(&[(&m.dev, 10)]),
                }],
                ..CmdReply::default()
            }
        }
        Err(e) => failed(NAME, &e),
    }
}

/// Copy the volume mounted on virtual device `dev` onto `path` (a disk
/// or a file): `svc.vrtout`, dev builds. A recovery boots with its
/// system save on a disk of its own, and the catalog's image file is on
/// the system disk the recovery replaces; on a real system the file is
/// sent off the system instead (FTP, SFTP).
pub fn export(dev: &str, path: &str) -> Result<String, String> {
    let dev = dev.to_ascii_uppercase();
    let c = media::catalog_on(&dev).ok_or_else(|| format!("{} device {dev}: no image catalog is loaded", drive::NOT_READY))?;
    let e = c
        .mounted()
        .ok_or_else(|| format!("{} device {dev}: no volume is mounted", drive::NOT_READY))?;
    let from = catalogs::host_path(&c.path_of(e));
    let mut src = File::open(&from).map_err(|x| format!("{}: {x}", from.display()))?;
    let mut dst = OpenOptions::new().write(true).open(path).map_err(|x| format!("{path}: {x}"))?;
    let n = std::io::copy(&mut src, &mut dst).map_err(|x| format!("{path}: {x}"))?;
    // Whatever followed on the disk is no longer part of the volume.
    std::io::copy(&mut std::io::repeat(0).take(64 * 1024), &mut dst).map_err(|x| format!("{path}: {x}"))?;
    dst.sync_all().map_err(|x| format!("{path}: {x}"))?;
    Ok(format!("volume {} of {dev}: {n} bytes copied to {path}", e.volume))
}

// ------------------------------------------------------------- recovery

/// Where the operating system install leaves word of the user data on
/// its system save, for the storage service to restore.
pub const PENDING: &str = "/run/qs400/recovery";
/// The first line of [`PENDING`].
const PENDING_MAGIC: &str = "QS400-RECOVERY 1";

/// What holds the files after a system save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Medium {
    /// A disk that is a virtual tape volume (`TAPVRTnn` at install).
    Disk(PathBuf),
    /// A tape drive, by its resource name (`TAPnn`).
    Drive(String),
}

/// User data waiting on the install device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    /// The install device's name, for the messages.
    pub device: String,
    /// Where the files are.
    pub medium: Medium,
    /// How many files follow the system save.
    pub files: usize,
}

impl Pending {
    /// As [`PENDING`] holds it.
    #[must_use]
    pub fn to_text(&self) -> String {
        let (kind, at) = match &self.medium {
            Medium::Disk(p) => ("disk", p.display().to_string()),
            Medium::Drive(r) => ("drive", r.clone()),
        };
        format!("{PENDING_MAGIC}\n{kind} {} {} {at}\n", self.device, self.files)
    }

    /// From [`PENDING`]'s text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut l = text.lines();
        if l.next()? != PENDING_MAGIC {
            return None;
        }
        let mut f = l.next()?.splitn(4, ' ');
        let (kind, device, files, at) = (f.next()?, f.next()?, f.next()?, f.next()?);
        let medium = match kind {
            "disk" => Medium::Disk(PathBuf::from(at)),
            "drive" => Medium::Drive(at.to_string()),
            _ => return None,
        };
        Some(Self {
            device: device.to_string(),
            medium,
            files: files.parse().ok()?,
        })
    }
}

/// The recovery's restores from `SAVSYS` media, in IBM's order:
/// `RSTUSRPRF USRPRF(*ALL)`, `RSTCFG OBJ(*ALL)`, then `RSTAUT` once the
/// objects are back. User data after the system save (`user`) is left
/// for the storage service, which has the IFS ([`restore_user_data`]).
/// One line per step for the log.
pub fn recover(system: &mut System, secdta: Option<&[u8]>, cfg: Option<&[u8]>, user: Option<&Pending>) -> Vec<String> {
    let mut out = Vec::new();
    let Some(storage) = system.storage.as_mut() else {
        return vec!["recovery: object store not open".into()];
    };
    let Some(qsys) = storage.qsys else {
        return vec!["recovery: object store not initialized".into()];
    };
    let hooks = SvcHooks::new(qsys);
    let store = &mut storage.store;
    let now = now_unix();
    if let Some(s) = secdta {
        let r = Reader::new(Cursor::new(s)).map_err(|e| e.to_string()).and_then(|mut r| {
            let opts = saverst::secdta::ProfileOptions {
                new_only: false,
                users: Vec::new(),
                omit: Vec::new(),
                now,
            };
            sec::guard::as_system(|| {
                saverst::secdta::restore_profiles(store, JobHandle(0), &hooks, qsys, &mut r, &opts)
            })
            .map_err(|e| e.to_string())
        });
        out.push(match r {
            Ok(rep) => format!(
                "RSTUSRPRF: {} user profiles restored, {} not; {} authorization lists",
                rep.profiles(),
                rep.profiles_not(),
                rep.autls()
            ),
            Err(e) => format!("RSTUSRPRF: {e}"),
        });
    }
    if let Some(c) = cfg {
        let r = Reader::new(Cursor::new(c)).map_err(|e| e.to_string()).and_then(|mut r| {
            let opts = RestoreOptions {
                libraries: vec!["QSYS".into()],
                select: CFG_TYPES
                    .iter()
                    .map(|t| ObjSel {
                        name: "*ALL".into(),
                        type_code: Some(*t),
                    })
                    .collect(),
                system: "QS400".into(),
                now,
                ..RestoreOptions::default()
            };
            sec::guard::as_system(|| saverst::restore(store, JobHandle(0), &hooks, qsys, &mut r, &opts))
                .map_err(|e| e.to_string())
        });
        out.push(match r {
            Ok(rep) => format!(
                "RSTCFG: {} configuration objects restored",
                rep.objects.iter().filter(|o| o.msgid.is_none()).count()
            ),
            Err(e) => format!("RSTCFG: {e}"),
        });
    }
    if secdta.is_some() {
        let r = sec::guard::as_system(|| saverst::secdta::rstaut(store, &hooks, qsys, &[]));
        out.push(match r {
            Ok(done) => format!(
                "RSTAUT: authorities restored for {} profiles",
                done.len()
            ),
            Err(e) => format!("RSTAUT: {e}"),
        });
    }
    if let Some(u) = user {
        let wrote = Path::new(PENDING)
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(PENDING, u.to_text()));
        out.push(match wrote {
            Ok(()) => format!(
                "user data: {} files after the system save on {}; RSTLIB and RST run when the storage service starts",
                u.files, u.device
            ),
            Err(e) => format!("user data on {}: {PENDING}: {e}", u.device),
        });
    }
    out
}

/// The device, image catalog and catalog directory the recovery opens a
/// disk's volume through.
const RCY_DEV: &str = "QRCYTAP";
const RCY_CLG: &str = "QRCYCLG";
const RCY_DIR: &str = "/QRCYCLG";

/// A command's outcome in one line: the escape, or the last line.
fn outcome(r: &CmdReply) -> String {
    let last = r.text.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("").to_string();
    match &r.escape {
        Some(e) if !last.starts_with(&e.msgid) => format!("{} {last}", e.msgid),
        _ => last,
    }
}

/// Run `cl`, logging its outcome.
fn step(system: &mut System, out: &mut Vec<String>, cl: &str) -> bool {
    let r = super::opacmd::run_cl(system, cl);
    let word = cl.split([' ', '(']).next().unwrap_or(cl);
    out.push(format!("{word}: {}", outcome(&r)));
    // What was not restored, for the operator.
    for l in r.text.lines().map(str::trim).filter(|l| l.starts_with("CPF") || l.contains("not restored:")).take(20) {
        out.push(format!("{word}:   {l}"));
    }
    r.escape.is_none()
}

/// Run quietly: setup and cleanup.
fn quiet(system: &mut System, cl: &str) -> Result<(), String> {
    let r = super::opacmd::run_cl(system, cl);
    match r.escape {
        None => Ok(()),
        Some(_) => Err(format!("{cl}: {}", outcome(&r))),
    }
}

/// A disk holding a virtual tape volume, as a tape device: a catalog
/// whose one entry is the disk, loaded on a virtual device.
fn attach_disk(system: &mut System, disk: &Path) -> Result<(), String> {
    detach_disk(system);
    quiet(system, &format!("CRTDEVTAP DEVD({RCY_DEV}) RSRCNAME(*VRT) TEXT('Recovery')"))?;
    quiet(system, &format!("CRTIMGCLG IMGCLG({RCY_CLG}) DIR('{RCY_DIR}') TYPE(*TAP) CRTDIR(*YES) TEXT('Recovery')"))?;
    let link = catalogs::host_path(&format!("{RCY_DIR}/{RCY_DEV}"));
    std::os::unix::fs::symlink(disk, &link).map_err(|e| format!("{}: {e}", link.display()))?;
    quiet(system, &format!("ADDIMGCLGE IMGCLG({RCY_CLG}) FROMFILE({RCY_DEV}) TOFILE(*FROMFILE)"))?;
    quiet(system, &format!("LODIMGCLG IMGCLG({RCY_CLG}) DEV({RCY_DEV})"))?;
    quiet(system, &format!("VRYCFG CFGOBJ({RCY_DEV}) CFGTYPE(*DEV) STATUS(*ON)"))
}

/// Take the recovery's device and catalog away; the disk is left as it is.
fn detach_disk(system: &mut System) {
    let _ = quiet(system, &format!("VRYCFG CFGOBJ({RCY_DEV}) CFGTYPE(*DEV) STATUS(*OFF)"));
    let _ = quiet(system, &format!("LODIMGCLG IMGCLG({RCY_CLG}) OPTION(*UNLOAD)"));
    let _ = std::fs::remove_file(catalogs::host_path(&format!("{RCY_DIR}/{RCY_DEV}")));
    let _ = quiet(system, &format!("DLTIMGCLG IMGCLG({RCY_CLG}) KEEP(*YES)"));
    let _ = std::fs::remove_dir_all(catalogs::host_path(RCY_DIR));
    let _ = quiet(system, &format!("DLTDEVD DEVD({RCY_DEV})"));
}

/// The commands of the user data's recovery on device `dev`, in IBM's
/// order: the libraries, the IFS, then the authorities again.
#[must_use]
pub fn user_data_commands(dev: &str) -> [String; 3] {
    [
        format!("RSTLIB SAVLIB(*NONSYS) DEV({dev}) MBROPT(*ALL) ALWOBJDIF(*ALL)"),
        format!("RST DEV('/QSYS.LIB/{dev}.DEVD') OBJ(('/*') ('/QSYS.LIB' *OMIT))"),
        "RSTAUT USRPRF(*ALL)".into(),
    ]
}

/// In the storage service, once it is up: the user libraries and the
/// IFS from the files after the system save the install came from
/// (`RSTLIB SAVLIB(*NONSYS)`, `RST`, then `RSTAUT`), through the device
/// layer. Nothing when the install left none. One line per step.
pub fn restore_user_data(system: &mut System) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(PENDING) else {
        return Vec::new();
    };
    let _ = std::fs::remove_file(PENDING);
    let Some(p) = Pending::parse(&text) else {
        return vec![format!("recovery: {PENDING} is not understood")];
    };
    let mut out = vec![format!("recovery: user data from {} ({} files)", p.device, p.files)];
    let dev = match &p.medium {
        Medium::Disk(disk) => {
            if let Err(e) = attach_disk(system, disk) {
                out.push(format!("recovery: {e}"));
                detach_disk(system);
                return out;
            }
            RCY_DEV.to_string()
        }
        Medium::Drive(rsrc) => {
            let _ = quiet(system, &format!("CRTDEVTAP DEVD({rsrc}) RSRCNAME({rsrc})"));
            let _ = quiet(system, &format!("VRYCFG CFGOBJ({rsrc}) CFGTYPE(*DEV) STATUS(*ON)"));
            rsrc.clone()
        }
    };
    for cl in user_data_commands(&dev) {
        step(system, &mut out, &cl);
    }
    if matches!(p.medium, Medium::Disk(_)) {
        detach_disk(system);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_round_trips() {
        for p in [
            Pending {
                device: "TAPVRT01".into(),
                medium: Medium::Disk("/dev/vdb".into()),
                files: 7,
            },
            Pending {
                device: "TAP01".into(),
                medium: Medium::Drive("TAP01".into()),
                files: 2,
            },
        ] {
            assert_eq!(Pending::parse(&p.to_text()), Some(p));
        }
        assert_eq!(Pending::parse("QS400-RECOVERY 2\ndisk A 1 /x\n"), None);
        assert_eq!(Pending::parse("QS400-RECOVERY 1\nfloppy A 1 /x\n"), None);
    }

    #[test]
    fn user_data_follows_ibm_order() {
        let c = user_data_commands("TAP01");
        assert!(c[0].starts_with("RSTLIB SAVLIB(*NONSYS) DEV(TAP01)"));
        assert!(c[1].starts_with("RST DEV('/QSYS.LIB/TAP01.DEVD')"));
        assert_eq!(c[2], "RSTAUT USRPRF(*ALL)");
    }

    #[test]
    fn outcome_prefers_the_escape() {
        let r = super::super::escape("CPF3780", "RSTLIB", "nothing to restore");
        assert_eq!(outcome(&r), "CPF3780 nothing to restore");
        let ok = super::super::done("12 objects restored\n".into(), "CPC3703", &[]);
        assert_eq!(outcome(&ok), "12 objects restored");
    }
}
