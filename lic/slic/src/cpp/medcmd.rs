// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media commands in the service: the tape, optical and media
//! library device descriptions (`CRTDEVTAP`, `CRTDEVOPT`, `CRTDEVMLB`,
//! `CHGDEVxxx`), the image catalog commands, and the tape and optical
//! commands (`INZTAP`, `DSPTAP`, `CHKTAP`, `DUPTAP`, `INZOPT`, `DSPOPT`,
//! `CHKOPT`, `DUPOPT`, `WRKOPTVOL`). The work is `stgsvc::media`'s and
//! the `tape`, `optical` and `imgclg` crates'; here are the parameters,
//! the text forms and the messages: each its catalogue entry with IBM's
//! identifier and data layout, and no words of the command's own
//! (docs/MEDIA.md, "Messages of the media commands").

use std::fmt::Write as _;

use cmd::block::Parms;
use imgclg::ImgType;
use imgclg::ops::{
    AddImgClgE, AddVrtVol, ChgImgClgE, CrtImgClg, EntryOption, FromFile, LoadOption, ToFile,
    VfyImgClg, VfyType,
};
use medio::MediaError;
use medio::msg::{Field, Msg};
use stg::proto::CmdReply;
use tape::device::Category;
use tape::volume::{Check, ChkOptions, DupOptions, DupSeq, InzOptions, ToSeq};
use tape::{SeqNbr, TapeTransport, VolumeProvider};
use wm::obj::devd::Devd;
use wm::obj::devmed::{MediaDevice, MediaKind};

use super::meddsp;
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::media::drive::{self, TapeJob};
use crate::stgsvc::media::fail::{Fail, cause, done};
use crate::stgsvc::media::{self, catalogs, devices, stream};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CRTDEVTAP" | "CRTDEVOPT" | "CRTDEVMLB" => crtdev(system, name, parms),
        "CHGDEVTAP" | "CHGDEVOPT" | "CHGDEVMLB" => chgdev(system, name, parms),
        "CRTIMGCLG" => crtimgclg(system, parms),
        "DLTIMGCLG" => dltimgclg(system, parms),
        "ADDIMGCLGE" => addimgclge(system, parms),
        "RMVIMGCLGE" => rmvimgclge(parms),
        "CHGIMGCLGE" => chgimgclge(parms),
        "LODIMGCLG" => lodimgclg(system, parms),
        "LODIMGCLGE" => lodimgclge(parms),
        "VFYIMGCLG" => vfyimgclg(parms),
        "WRKIMGCLG" => Ok(plain(wrkimgclg_text(
            &parms.name("IMGCLG"),
            &parms.name("TYPE"),
        ))),
        "WRKIMGCLGE" => wrkimgclge_text(&parms.name("IMGCLG"))
            .map(plain)
            .map_err(Fail::from),
        "INZTAP" => inztap(system, parms),
        "DSPTAP" => dsptap(system, parms),
        "CHKTAP" => chktap(system, parms),
        "DUPTAP" => duptap(system, parms),
        "INZOPT" => inzopt(system, parms),
        "DSPOPT" => dspopt(system, parms),
        "CHKOPT" => chkopt(system, parms),
        "DUPOPT" => dupopt(system, parms),
        "WRKOPTVOL" => wrkoptvol(system, parms).map(plain),
        "ADDOPTCTG" => Ok(addoptctg(system, parms)),
        "WRKMLBSTS" => Ok(plain(wrkmlbsts_text(system, &parms.name("MLB")))),
        "DSPTAPCTG" => dsptapctg(system, parms),
        "WRKTAPCTG" | "ADDTAPCTG" | "RMVTAPCTG" | "CHGTAPCTG" => mlb_ctg(system, name, parms),
        _ => return None,
    };
    Some(match r {
        Ok(reply) => reply,
        Err(e) => e.reply(),
    })
}

/// Device description created, changed; not created, not changed;
/// not found.
const DEV_CREATED: &str = "CPC2622";
const DEV_CHANGED: &str = "CPC2613";
const DEV_NOT_CREATED: &str = "CPF261A";
const DEV_NOT_CHANGED: &str = "CPF2618";
const DEV_NOT_FOUND: &str = "CPF9814";
/// Image catalog created in its library, deleted, entry added, removed,
/// changed, catalog loaded, unloaded, verified; entry loaded, unloaded,
/// mounted.
const CLG_CREATED: &str = "CPCBC00";
const CLG_DELETED: &str = "CPCBC18";
const CLGE_ADDED: &str = "CPCBC28";
const CLGE_REMOVED: &str = "CPCBC30";
const CLGE_CHANGED: &str = "CPCBC0C";
const CLG_LOADED: &str = "CPCBC10";
const CLG_UNLOADED: &str = "CPCBC11";
const CLG_VERIFIED: &str = "CPCBC20";
const CLGE_LOADED: &str = "CPCBC40";
const CLGE_UNLOADED: &str = "CPCBC41";
const CLGE_MOUNTED: &str = "CPCBC42";
/// The library image catalogs live in.
const CLG_LIB: &str = "QUSRSYS";
/// Tape volume found, file found, files duplicated.
const VOL_FOUND: &str = "CPC6778";
const FILE_FOUND: &str = "CPC6779";
const TAPE_DUPLICATED: &str = "CPC6775";
/// QS/400's own: optical volume initialized, checked, duplicated.
const OPT_INITIALIZED: &str = "CPC6774";
const OPT_CHECKED: &str = "CPC6770";
const OPT_DUPLICATED: &str = "CPC6772";

/// A parameter value that cannot be used: `CPD0084` naming it.
fn bad_parm(kwd: &str) -> Fail {
    Fail::new("CPD0084", &[Field::Char(kwd, 10)])
}

/// A catalog's refusal: its own escape when the catalog is missing or
/// already there, else the command's `id` over the cause.
fn clg_fail(e: &imgclg::Error, name: &str, id: &str, fields: &[Field<'_>]) -> Fail {
    match e {
        imgclg::Error::NotFound(_) => Fail::new("CPFBC45", &[Field::Char(name, 10)]),
        imgclg::Error::Exists(_) => Fail::new(
            "CPFBC01",
            &[Field::Char(name, 10), Field::Char(CLG_LIB, 10)],
        ),
        _ => Fail::new(id, fields).detail(e),
    }
}

/// A device description read for a command: `CPF9814` when there is none.
fn read_dev(system: &mut System, dev: &str) -> Result<Devd, Fail> {
    devices::read(system, dev).map_err(|e| {
        if e.starts_with(devices::NOT_FOUND) {
            Fail::new(DEV_NOT_FOUND, &[Field::Char(dev, 10)])
        } else {
            Fail::from(e)
        }
    })
}

/// Not an optical device; an optical device that is not a library.
const OPT_NOT_DEVICE: &str = "OPT1530";
const OPT_NOT_LIBRARY: &str = "OPT1652";

/// `ADDOPTCTG`: IBM moves a cartridge from an optical media library's
/// I/O station or magazine into the library. QS/400 has stand-alone and
/// virtual optical devices only (`CRTDEVMLB DEVCLS(*OPT)` is `CPF9B70`),
/// so an optical device is refused as no library (`OPT1652`, what IBM
/// sends for a stand-alone drive) and any other name as no optical
/// device (`OPT1530`). Nothing is changed.
fn addoptctg(system: &mut System, parms: &Parms) -> CmdReply {
    let dev = parms.name("MLB").to_ascii_uppercase();
    let id = match devices::read(system, &dev) {
        Ok(d)
            if d.media
                .as_ref()
                .is_some_and(|m| m.kind == MediaKind::Optical) =>
        {
            OPT_NOT_LIBRARY
        }
        Ok(_) => OPT_NOT_DEVICE,
        Err(e) if e.starts_with(devices::NOT_FOUND) => OPT_NOT_DEVICE,
        Err(e) => return Fail::from(e).reply(),
    };
    Fail::new(id, &[Field::Char(&dev, 10)]).reply()
}

fn plain(text: String) -> CmdReply {
    CmdReply {
        text,
        ..CmdReply::default()
    }
}

fn owner(system: &System) -> String {
    crate::stgsvc::security::actor()
        .map(|p| p.user.clone())
        .or_else(|| system.svc_user.clone())
        .unwrap_or_else(|| "QSYS".into())
}

/// `TEXT`, `*BLANK` as empty; `None` for `*SAME`.
fn text(parms: &Parms) -> Option<String> {
    let t = parms.text("TEXT");
    match t.trim() {
        "*SAME" => None,
        "*BLANK" => Some(String::new()),
        _ => Some(t.trim().to_string()),
    }
}

/// `MSGQ` as `lib/name`; `None` for `*SAME`.
fn msgq(parms: &Parms) -> Option<String> {
    let (n, l) = parms.qual("MSGQ");
    match n.to_ascii_uppercase().as_str() {
        "*SAME" => None,
        "" | "*SYSOPR" => Some("QSYS/QSYSOPR".into()),
        n => Some(format!(
            "{}/{n}",
            if l.is_empty() || l.starts_with('*') {
                "QSYS".into()
            } else {
                l.to_ascii_uppercase()
            }
        )),
    }
}

fn kind_of(cmd: &str) -> MediaKind {
    match &cmd[cmd.len() - 3..] {
        "TAP" => MediaKind::Tape,
        "OPT" => MediaKind::Optical,
        _ => MediaKind::TapeLibrary,
    }
}

fn crtdev(system: &mut System, cmd: &str, parms: &Parms) -> Result<CmdReply, Fail> {
    let kind = kind_of(cmd);
    if kind == MediaKind::TapeLibrary && parms.name("DEVCLS") == "*OPT" {
        return Err(Fail::new("CPF9B70", &[Field::Char(cmd, 10)])
            .detail("DEVCLS(*OPT): optical media libraries are not available in this release"));
    }
    let name = parms.name("DEVD");
    let not_made = |e: String| {
        Fail::new(DEV_NOT_CREATED, &[Field::Char(&name, 10)]).detail(cause(&e))
    };
    let rsrc_in = parms.name("RSRCNAME");
    let virt = rsrc_in == "*VRT" || parms.name("TYPE") == "*VRT";
    let rsrc = if virt {
        devices::next_virtual(system, kind).map_err(not_made)?
    } else if rsrc_in == "*NONE" {
        String::new()
    } else {
        rsrc_in
    };
    let (dt, dm) = if virt {
        kind.virtual_type()
    } else {
        devices::real_type(kind)
    };
    let pick = |kwd: &str, dft: &str| {
        let v = parms.name(kwd);
        if v.is_empty() || v.starts_with('*') {
            dft.to_string()
        } else {
            v
        }
    };
    let positive = |kwd: &str| u32::try_from(parms.int(kwd).unwrap_or(0)).unwrap_or(0);
    let med = MediaDevice {
        kind,
        assign: parms.name("ASSIGN") != "*NO",
        unload: parms.name("UNLOAD") != "*NO",
        gen_ctg_sysgen: parms.name("GENCTGID") == "*SYSGEN",
        rsrcname: rsrc,
        unload_wait: positive("UNLOADWAIT"),
        max_dev_time: positive("MAXDEVTIME"),
        last_used: 0,
    };
    let mut d = Devd::media_device(&pick("TYPE", dt), &pick("MODEL", dm), med);
    d.online = parms.name("ONLINE") != "*NO";
    d.text = text(parms).unwrap_or_default();
    if let Some(q) = msgq(parms) {
        d.msgq = q;
    }
    let who = owner(system);
    devices::create(system, &name, &d, &who).map_err(not_made)?;
    Ok(done(&Msg::new(DEV_CREATED, &[Field::Char(&name, 10)])))
}

fn chgdev(system: &mut System, cmd: &str, parms: &Parms) -> Result<CmdReply, Fail> {
    let name = parms.name("DEVD");
    let not_changed = |why: &str| {
        Fail::new(DEV_NOT_CHANGED, &[Field::Char(&name, 10)]).detail(cause(why))
    };
    let mut d = devices::read(system, &name).map_err(|e| {
        if e.starts_with(devices::NOT_FOUND) {
            Fail::new(devices::NOT_FOUND, &[Field::Char(&name, 10)])
        } else {
            not_changed(&e)
        }
    })?;
    let kind = kind_of(cmd);
    let Some(m) = d.media.as_mut() else {
        unreachable!("read checks")
    };
    if m.kind != kind {
        let why = format!("device {name} is a {} device", m.kind.devcls());
        return Err(not_changed(&why));
    }
    let rsrc = parms.name("RSRCNAME");
    if !rsrc.is_empty() && rsrc != "*SAME" {
        if devices::usable(&d) {
            return Err(not_changed(&format!(
                "device {name} must be varied off to change RSRCNAME"
            )));
        }
        let new = if rsrc == "*VRT" {
            devices::next_virtual(system, kind).map_err(|e| not_changed(&e))?
        } else {
            rsrc
        };
        if let Some(m) = d.media.as_mut() {
            m.rsrcname = new;
        }
    }
    let m = d.media.as_mut().expect("media");
    let yes = |kwd: &str| match parms.name(kwd).as_str() {
        "*YES" => Some(true),
        "*NO" => Some(false),
        _ => None,
    };
    if let Some(v) = yes("ASSIGN") {
        m.assign = v;
    }
    if let Some(v) = yes("UNLOAD") {
        m.unload = v;
    }
    for (kwd, slot) in [
        ("UNLOADWAIT", &mut m.unload_wait),
        ("MAXDEVTIME", &mut m.max_dev_time),
    ] {
        if let Some(n) = parms.int(kwd).filter(|n| *n != -3) {
            *slot = u32::try_from(n).unwrap_or(0);
        }
    }
    match parms.name("GENCTGID").as_str() {
        "*SYSGEN" => m.gen_ctg_sysgen = true,
        "*VOLID" => m.gen_ctg_sysgen = false,
        _ => {}
    }
    if let Some(v) = yes("ONLINE") {
        d.online = v;
    }
    if let Some(t) = text(parms) {
        d.text = t;
    }
    if let Some(q) = msgq(parms) {
        d.msgq = q;
    }
    devices::write(system, &name, &d).map_err(|e| not_changed(&e))?;
    Ok(done(&Msg::new(DEV_CHANGED, &[Field::Char(&name, 10)])))
}

// ----------------------------------------------------------------- catalogs

fn crtimgclg(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let name = parms.name("IMGCLG");
    let kind = if parms.name("TYPE") == "*TAP" {
        ImgType::Tap
    } else {
        ImgType::Opt
    };
    let add = match parms.int("ADDVRTVOL").unwrap_or(0) {
        0 => AddVrtVol::None,
        -1 => AddVrtVol::Dir,
        n => AddVrtVol::New {
            count: u16::try_from(n).unwrap_or(1),
            size_mb: u64::try_from(parms.int("IMGSIZ").unwrap_or(4700)).unwrap_or(4700),
        },
    };
    let text = text(parms).unwrap_or_default();
    let p = CrtImgClg {
        name: name.clone(),
        dir: parms.text("DIR").trim().trim_matches('\'').to_string(),
        kind,
        create_dir: parms.name("CRTDIR") != "*NO",
        add,
        text: text.clone(),
        epoch: now_unix(),
        import: parms.yes("IMPORT"),
    };
    let fields = [Field::Char(&name, 10), Field::Char(CLG_LIB, 10)];
    media::catalog_op(|r| r.crtimgclg(&p)).map_err(|e| clg_fail(&e, &name, "CPFBC02", &fields))?;
    let who = owner(system);
    if let Err(e) = catalogs::create_object(
        system,
        &name,
        &format!("/{}", p.dir.trim_matches('/')),
        &text,
        &who,
    ) {
        let _ = media::with_catalogs(|r| r.dltimgclg(&name, true));
        return Err(Fail::new("CPFBC02", &fields).detail(cause(&e)));
    }
    Ok(done(&Msg::new(CLG_CREATED, &fields)))
}

fn dltimgclg(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let want = parms.name("IMGCLG");
    let keep = parms.name("KEEP") != "*NO";
    let names: Vec<String> = media::catalog_op(|r| Ok(r.names()))
        .unwrap_or_default()
        .into_iter()
        .filter(|n| *n == want || want.strip_suffix('*').is_some_and(|p| n.starts_with(p)))
        .collect();
    if names.is_empty() {
        return Err(Fail::new("CPFBC45", &[Field::Char(&want, 10)]));
    }
    let mut reply = CmdReply::default();
    for n in &names {
        let fields = [Field::Char(n, 10)];
        media::catalog_op(|r| r.dltimgclg(n, keep))
            .map_err(|e| clg_fail(&e, n, "CPFBC18", &fields))?;
        catalogs::delete_object(system, n)
            .map_err(|e| Fail::new("CPFBC18", &fields).detail(cause(&e)))?;
        reply
            .messages
            .extend(done(&Msg::new(CLG_DELETED, &fields)).messages);
    }
    Ok(reply)
}

fn index_of(parms: &Parms) -> Option<u16> {
    parms
        .int("IMGCLGIDX")
        .and_then(|i| u16::try_from(i).ok())
        .filter(|i| *i > 0)
}

/// `ADDIMGCLGE FROMDEV(dev)`: the volume in an optical device. A virtual
/// device's is its catalog's mounted image, copied; a real drive's
/// medium is read whole into a new image.
enum FromDev {
    Image(String),
    Drive(String),
}

fn from_dev(system: &mut System, dev: &str) -> Result<FromDev, Fail> {
    let t = drive::device(system, dev)?;
    if t.kind != MediaKind::Optical {
        return Err(Fail::new(OPT_NOT_DEVICE, &[Field::Char(&t.dev, 10)]));
    }
    let empty = || Fail::new(drive::OPT_EMPTY, &[Field::Char(&t.dev, 10)]);
    if t.virtual_ {
        let c = media::catalog_on(&t.dev).ok_or_else(empty)?;
        let e = c.mounted().ok_or_else(empty)?;
        return Ok(FromDev::Image(c.path_of(e)));
    }
    let node = devices::node_of(system, &t.rsrc).ok_or_else(|| {
        drive::dev_fail(
            drive::NOT_READY,
            &t.dev,
            format_args!("resource {} not found", t.rsrc),
        )
    })?;
    Ok(FromDev::Drive(format!("/dev/{node}")))
}

fn addimgclge(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let name = parms.name("IMGCLG");
    let dev = if parms.specified("FROMDEV") {
        let d = parms.name("FROMDEV");
        (!d.is_empty() && !d.starts_with('*')).then_some(d)
    } else {
        None
    };
    let from_dev = match &dev {
        Some(d) => Some(from_dev(system, d)?),
        None => None,
    };
    let from = match &from_dev {
        Some(FromDev::Image(p)) => p.clone(),
        Some(FromDev::Drive(_)) => "*NEW".to_string(),
        None => parms.text("FROMFILE"),
    };
    let from = from.trim().trim_matches('\'');
    if from.is_empty() {
        return Err(bad_parm("FROMFILE"));
    }
    let to = parms.text("TOFILE");
    let to = to.trim().trim_matches('\'');
    let vol = parms.text("VOLNAM");
    let vol = vol.trim().trim_matches('\'');
    let p = AddImgClgE {
        from: if from.eq_ignore_ascii_case("*NEW") {
            FromFile::New
        } else {
            FromFile::Path(from.to_string())
        },
        to_file: match to.to_ascii_uppercase().as_str() {
            "" | "*GEN" => ToFile::Gen,
            "*FROMFILE" => ToFile::FromFile,
            _ => ToFile::Name(to.to_string()),
        },
        index: index_of(parms),
        size_mb: parms
            .int("IMGSIZ")
            .and_then(|n| u64::try_from(n).ok())
            .or(Some(4700)),
        volume: (!vol.is_empty() && !vol.starts_with('*')).then(|| vol.to_ascii_uppercase()),
        write_protect: parms.specified("WRTPTC").then(|| parms.yes("WRTPTC")),
        text: text(parms).unwrap_or_default(),
        epoch: now_unix(),
    };
    let replace = parms.yes("REPLACE");
    let mut drive = match &from_dev {
        Some(FromDev::Drive(node)) => Some(std::fs::File::open(node).map_err(|e| {
            drive::dev_fail(drive::NOT_READY, dev.as_deref().unwrap_or_default(), e)
        })?),
        _ => None,
    };
    let fields = [Field::Char(&name, 10)];
    media::catalog_op(|r| {
        if replace && let Some(i) = p.index {
            let _ = r.rmvimgclge(&name, i, true);
        }
        match drive.as_mut() {
            Some(f) => r.addimgclge_from(&name, &p, f),
            None => r.addimgclge(&name, &p),
        }
    })
    .map_err(|e| clg_fail(&e, &name, "CPFBC28", &fields))?;
    Ok(done(&Msg::new(CLGE_ADDED, &fields)))
}

/// An entry index as `*BIN 2`.
fn bin2(i: u16) -> Field<'static> {
    Field::Bin2(i16::try_from(i).unwrap_or(i16::MAX))
}

fn rmvimgclge(parms: &Parms) -> Result<CmdReply, Fail> {
    let name = parms.name("IMGCLG");
    let keep = parms.name("KEEP") != "*NO";
    let vol = parms.text("VOL").trim().to_ascii_uppercase();
    let at = index_of(parms);
    let i = media::catalog_op(|r| {
        let i = match at {
            Some(i) => i,
            None => r
                .catalog(&name)?
                .entries
                .iter()
                .find(|e| e.volume.eq_ignore_ascii_case(&vol))
                .map(|e| e.index)
                .ok_or_else(|| {
                    imgclg::Error::Invalid(format!("volume {vol} is not in image catalog {name}"))
                })?,
        };
        r.rmvimgclge(&name, i, keep).map(|()| i)
    })
    .map_err(|e| match at {
        Some(i) => clg_fail(&e, &name, "CPFBC30", &[Field::Char(&name, 10), bin2(i)]),
        None => clg_fail(
            &e,
            &name,
            "CPFBC31",
            &[Field::Char(&name, 10), Field::Char(&vol, 6)],
        ),
    })?;
    Ok(done(&Msg::new(
        CLGE_REMOVED,
        &[Field::Char(&name, 10), bin2(i)],
    )))
}

fn chgimgclge(parms: &Parms) -> Result<CmdReply, Fail> {
    let name = parms.name("IMGCLG");
    let i = index_of(parms).unwrap_or(0);
    let p = ChgImgClgE {
        write_protect: match parms.name("WRTPTC").as_str() {
            "*YES" => Some(true),
            "*NO" => Some(false),
            _ => None,
        },
        text: text(parms),
    };
    media::catalog_op(|r| r.chgimgclge(&name, i, &p))
        .map_err(|e| clg_fail(&e, &name, "CPFBC0C", &[Field::Char(&name, 10), bin2(i)]))?;
    Ok(done(&Msg::new(CLGE_CHANGED, &[bin2(i)])))
}

fn lodimgclg(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let name = parms.name("IMGCLG");
    let unload = parms.name("OPTION") == "*UNLOAD";
    let dev = parms.name("DEV");
    if unload {
        // The device it was on, for the messages.
        let on = media::catalog_op(|r| Ok(r.catalog(&name)?.device.clone()))
            .ok()
            .flatten()
            .unwrap_or_else(|| dev.clone());
        let fields = [Field::Char(&name, 10), Field::Char(&on, 10)];
        media::catalog_op(|r| r.lodimgclg(&name, LoadOption::Unload, ""))
            .map_err(|e| clg_fail(&e, &name, "CPFBC11", &fields))?;
        return Ok(done(&Msg::new(CLG_UNLOADED, &fields)));
    }
    if dev.is_empty() {
        return Err(bad_parm("DEV"));
    }
    let fields = [Field::Char(&name, 10), Field::Char(&dev, 10)];
    let d = read_dev(system, &dev)?;
    let m = d.media.unwrap_or_default();
    let kind = media::catalog_op(|r| Ok(r.catalog(&name)?.kind))
        .map_err(|e| clg_fail(&e, &name, "CPFBC10", &fields))?;
    let fits = match kind {
        ImgType::Opt => m.kind == MediaKind::Optical,
        ImgType::Tap => m.kind != MediaKind::Optical,
    };
    if !m.is_virtual() || !fits {
        let what = if kind == ImgType::Opt {
            "optical"
        } else {
            "tape"
        };
        return Err(Fail::new("CPFBC10", &fields)
            .detail(format_args!("device {dev} is not a virtual {what} device")));
    }
    media::catalog_op(|r| r.lodimgclg(&name, LoadOption::Load, &dev))
        .map_err(|e| clg_fail(&e, &name, "CPFBC10", &fields))?;
    Ok(done(&Msg::new(CLG_LOADED, &fields)))
}

fn lodimgclge(parms: &Parms) -> Result<CmdReply, Fail> {
    let name = parms.name("IMGCLG");
    let i = index_of(parms).unwrap_or(0);
    let (opt, ok, not) = match parms.name("OPTION").as_str() {
        "*UNLOAD" => (EntryOption::Unload, CLGE_UNLOADED, "CPFBC14"),
        "*MOUNT" => (EntryOption::Mount, CLGE_MOUNTED, "CPFBC15"),
        _ => (EntryOption::Load, CLGE_LOADED, "CPFBC13"),
    };
    media::catalog_op(|r| r.lodimgclge(&name, i, opt))
        .map_err(|e| clg_fail(&e, &name, not, &[bin2(i), Field::Char(&name, 10)]))?;
    Ok(done(&Msg::new(ok, &[bin2(i)])))
}

fn vfyimgclg(parms: &Parms) -> Result<CmdReply, Fail> {
    let name = parms.name("IMGCLG");
    let ty = match parms.name("TYPE") {
        t if t.starts_with('*') => t,
        _ => "*UPGRADE".to_string(),
    };
    let p = VfyImgClg {
        kind: match parms.name("TYPE").as_str() {
            "*PTF" => VfyType::Ptf,
            "*LIC" => VfyType::Lic,
            "*OTHER" => VfyType::Other,
            _ => VfyType::Upgrade,
        },
        sort: parms.yes("SORT"),
    };
    let fields = [Field::Char(&name, 10)];
    let r = media::catalog_op(|r| r.vfyimgclg(&name, &p))
        .map_err(|e| clg_fail(&e, &name, "CPFBC42", &fields))?;
    if !r.ok() {
        let probs: Vec<String> = r.problems.iter().map(|p| format!("{p:?}")).collect();
        return Err(Fail::new("CPFBC42", &fields).detail(probs.join("; ")));
    }
    Ok(done(&Msg::new(
        CLG_VERIFIED,
        &[Field::Char(&name, 10), Field::Char(&ty, 8)],
    )))
}

/// `WRKIMGCLG`'s text form.
pub(crate) fn wrkimgclg_text(which: &str, ty: &str) -> String {
    let rows = media::with_catalogs(|r| Ok(r.wrkimgclg())).unwrap_or_default();
    let mut s = String::from("Catalog     Status      Type  Text\n");
    for c in rows {
        let pass = (which.is_empty()
            || which == "*ALL"
            || c.name == which
            || which
                .strip_suffix('*')
                .is_some_and(|p| c.name.starts_with(p)))
            && (ty.is_empty() || ty == "*ALL" || ty == c.kind);
        if pass {
            let _ = writeln!(
                s,
                "{:<10}  {:<10}  {:<4}  {}",
                c.name, c.status, c.kind, c.text
            );
        }
    }
    s
}

/// `WRKIMGCLGE`'s text form.
pub(crate) fn wrkimgclge_text(name: &str) -> Result<String, String> {
    let d = media::with_catalogs(|r| r.wrkimgclge(name))?;
    let mut s = String::new();
    let _ = writeln!(s, "Image catalog . . . : {}", d.name);
    let _ = writeln!(s, "Status  . . . . . . : {}", d.status);
    let _ = writeln!(s, "Type  . . . . . . . : {}", d.kind);
    let _ = writeln!(s, "Device  . . . . . . : {}", d.device);
    let _ = writeln!(s, "Directory . . . . . : {}", d.dir);
    let _ = writeln!(
        s,
        "Index  Status      WrtPtc  Volume                            File"
    );
    for e in &d.entries {
        let _ = writeln!(
            s,
            "{:>5}  {:<10}  {:<6}  {:<32}  {}",
            e.index, e.status, e.write_protect, e.volume, e.file
        );
    }
    Ok(s)
}

// --------------------------------------------------------------------- tape

/// A `VOL` value, `None` for `*MOUNTED`.
fn vol(parms: &Parms, kwd: &str) -> Option<String> {
    let v = parms
        .text(kwd)
        .trim()
        .trim_matches('\'')
        .to_ascii_uppercase();
    (!v.is_empty() && !v.starts_with('*')).then_some(v)
}

/// A `VOL` list, empty for `*MOUNTED`.
fn vols(parms: &Parms, kwd: &str) -> Vec<String> {
    parms
        .list(kwd)
        .iter()
        .map(|v| v.source().trim().trim_matches('\'').to_ascii_uppercase())
        .filter(|v| !v.is_empty() && !v.starts_with('*'))
        .collect()
}

struct Inz(InzOptions);

impl TapeJob for Inz {
    type Out = ();
    fn run<D: TapeTransport + ?Sized>(
        &mut self,
        d: &mut D,
        _: &mut dyn VolumeProvider<D>,
    ) -> Result<(), MediaError> {
        Ok(tape::volume::inztap(d, &self.0)?)
    }
}

fn inztap(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let dev = parms.name("DEV");
    let new = parms.text("NEWVOL").trim().to_ascii_uppercase();
    let v = vol(parms, "VOL");
    let new_vol = match new.as_str() {
        "" | "*NONE" => None,
        "*CTGID" => v.clone(),
        n => Some(n.to_string()),
    };
    let owner = parms.text("NEWOWNID");
    let opts = InzOptions {
        new_vol: new_vol.clone(),
        new_owner: if owner.trim() == "*BLANK" {
            String::new()
        } else {
            owner.trim().to_string()
        },
        vol: v.clone(),
        check: match parms.name("CHECK").as_str() {
            "*NO" => Check::No,
            "*FIRST" => Check::First,
            _ => Check::Yes,
        },
        density: Some(parms.name("DENSITY")).filter(|d| !d.is_empty() && !d.starts_with('*')),
        code: if parms.name("CODE") == "*ASCII" {
            tape::label::Code::Ascii
        } else {
            tape::label::Code::Ebcdic
        },
        end_opt: stream::end_opt(&parms.name("ENDOPT")),
        clear: parms.yes("CLEAR"),
        today: Some(stream::today()),
    };
    let density = match parms.name("DENSITY") {
        d if d.is_empty() => "*DEVTYPE".to_string(),
        d => d,
    };
    let vl: Vec<String> = v.into_iter().collect();
    drive::tape_job(system, &dev, &vl, true, &mut Inz(opts))?;
    let dev = dev.to_ascii_uppercase();
    Ok(done(&medio::msg::tape_initialized(
        &dev,
        new_vol.as_deref(),
        &density,
    )))
}

/// The sequence range `SEQNBR` names.
fn seq_range(parms: &Parms) -> (u32, u32) {
    let e = parms.elems("SEQNBR");
    let num = |i: usize, dft: i64| {
        e.get(i)
            .copied()
            .flatten()
            .and_then(|v| v.source().trim().parse::<i64>().ok())
            .unwrap_or(dft)
    };
    let first = u32::try_from(num(0, 1).max(1)).unwrap_or(1);
    let last = match num(1, 16_777_215) {
        0 => first,
        n => u32::try_from(n).unwrap_or(u32::MAX),
    };
    (first, last)
}

/// `DSPTAP`'s view of the volume its parameters name.
pub(crate) fn tape_view_of(system: &mut System, parms: &Parms) -> Result<meddsp::TapView, Fail> {
    let (dev, vol, label, data, endopt) = (
        parms.name("DEV"),
        parms.text("VOL"),
        parms.text("LABEL"),
        parms.name("DATA"),
        parms.name("ENDOPT"),
    );
    meddsp::tape_view(
        system,
        &meddsp::TapAsk {
            dev: &dev,
            vol: &vol,
            label: &label,
            range: seq_range(parms),
            data: &data,
            end_opt: &endopt,
        },
    )
}

/// The text form, and the printed report, of `DSPTAP`.
fn tape_lines(v: &meddsp::TapView, data: &str) -> Vec<String> {
    let mut s = vec![
        format!("Volume identifier . . . . . : {}", v.vol),
        format!("Owner identifier  . . . . . : {}", v.owner),
        format!("Volume type . . . . . . . . : {}", v.vol_type),
        format!("Code  . . . . . . . . . . . : {}", v.code),
        format!("Device  . . . . . . . . . . : {}", v.dev),
    ];
    if data == "*VOLLBL" {
        return s;
    }
    if data == "*SAVRST" {
        // Each save's header, then the objects of every save.
        for h in &v.saves {
            s.push(String::new());
            s.push(format!("File sequence number  . . . : {}", h.seq));
            s.push(format!("Save command  . . . . . . . : {}", h.command));
            s.push(format!("Library saved . . . . . . . : {}", h.library));
            s.push(format!("Release . . . . . . . . . . : {}", h.release));
        }
        s.push(String::new());
        s.push(format!(
            "{:>4}  {:<10} {:<10} {:<10} {:<10} {:>12}  Text",
            "Seq", "Object", "Type", "Library", "Attribute", "Size"
        ));
        for o in &v.objects {
            let r = &o.row;
            let c = |i: usize| r.get(i).map_or("", String::as_str);
            s.push(format!(
                "{:>4}  {:<10} {:<10} {:<10} {:<10} {:>12}  {}",
                o.seq,
                c(0),
                c(1),
                c(2),
                c(3),
                c(4),
                c(5)
            ));
        }
        return s;
    }
    s.push(String::new());
    s.push("Seq   Label              VolSeq  Created   Expires   Blocks  End".into());
    for f in &v.files {
        s.push(format!(
            "{:>4}  {:<17}  {:>6}  {:<8}  {:<8} {:>6}  {}",
            f.seq, f.label, f.volseq, f.created, f.expires, f.blocks, f.end
        ));
    }
    s
}

fn dsptap(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    // Refused before the tape is read, as the 7.1 oracle does
    // (`audit/displays/dsptap71-outf-sav-err`).
    if parms.name("OUTPUT") == "*OUTFILE" && parms.name("DATA") != "*LABELS" {
        return Err(bad_parm("OUTPUT").detail("OUTPUT(*OUTFILE) is valid with DATA(*LABELS) only"));
    }
    let v = tape_view_of(system, parms)?;
    match parms.name("OUTPUT").as_str() {
        "*OUTFILE" => super::medout::tape(system, parms, &v)
            .map(plain)
            .map_err(Fail::from),
        "*PRINT" => crate::svc::security::print_as(
            system,
            "QSYSPRT",
            "DSPTAP",
            &tape_lines(&v, &parms.name("DATA")),
            false,
        )
        .map(plain)
        .map_err(Fail::from),
        _ => Ok(plain(tape_lines(&v, &parms.name("DATA")).join("\n") + "\n")),
    }
}

struct Chk(ChkOptions);

impl TapeJob for Chk {
    type Out = (tape::volume::ChkResult, Option<instmedia::layout::Contents>);
    fn run<D: TapeTransport + ?Sized>(
        &mut self,
        d: &mut D,
        _: &mut dyn VolumeProvider<D>,
    ) -> Result<Self::Out, MediaError> {
        // Install media or a system save says so; the check follows.
        let media = instmedia::savsys::recognize_tape(d).ok().flatten();
        Ok((tape::volume::chktap(d, &self.0)?, media))
    }
}

fn chktap(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let dev = parms.name("DEV");
    let label = parms.text("LABEL").trim().to_ascii_uppercase();
    let opts = ChkOptions {
        vol: vol(parms, "VOL"),
        seq: match parms.int("SEQNBR").unwrap_or(0) {
            -1 => Some(SeqNbr::End),
            n if n > 0 => u32::try_from(n).ok().map(SeqNbr::Number),
            _ => None,
        },
        file_label: (!label.is_empty() && !label.starts_with('*')).then_some(label),
        end_opt: stream::end_opt(&parms.name("ENDOPT")),
    };
    let vl: Vec<String> = opts.vol.iter().cloned().collect();
    let (r, media) = drive::tape_job(system, &dev, &vl, false, &mut Chk(opts))?;
    let dev = dev.to_ascii_uppercase();
    let id = r.kind.id();
    let m = match &r.file {
        Some(f) => {
            let seq = f.label.file_seq.to_string();
            let created = f
                .label
                .created
                .map(|d| format!("{}.{:03}", d.year, d.day))
                .unwrap_or_default();
            Msg::new(
                FILE_FOUND,
                &[
                    Field::Char(&dev, 10),
                    Field::Char(&id, 6),
                    Field::Char(&seq, 10),
                    Field::Char(&f.label.file_id, 17),
                    Field::Char(&created, 10),
                ],
            )
        }
        None => Msg::new(VOL_FOUND, &[Field::Char(&dev, 10), Field::Char(&id, 6)]),
    };
    let mut reply = done(&m);
    // Install media: what it holds is shown.
    if let Some(c) = &media {
        reply.text = install_media_text(c);
    }
    Ok(reply)
}

/// What install media holds, as `CHKTAP` and `DSPOPT DATA(*SAVRST)`
/// show it: the kind and release, then the payload files.
pub(crate) fn install_media_text(c: &instmedia::layout::Contents) -> String {
    let mut s = String::new();
    let kind = if c.savsys {
        "system save (SAVSYS)"
    } else {
        "distribution media"
    };
    let _ = writeln!(
        s,
        "Install media . . . . . . . : {kind}, release {}",
        c.release
    );
    let _ = writeln!(s, "Volume in the manifest  . . : {}", c.manifest.volume);
    let _ = writeln!(
        s,
        "Products  . . . . . . . . . : {}",
        c.products().join(" ")
    );
    for f in &c.manifest.files {
        let _ = writeln!(s, "  {:<32} {:>12} bytes", f.path, f.size);
    }
    s
}

fn duptap(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let from = parms.name("FROMDEV").to_ascii_uppercase();
    let to = parms.name("TODEV").to_ascii_uppercase();
    if from == to {
        return Err(Fail::new("CPF6740", &[Field::Char(&to, 10)]));
    }
    let e = parms.elems("FROMSEQNBR");
    let num = |i: usize, dft: i64| {
        e.get(i)
            .copied()
            .flatten()
            .and_then(|v| v.source().trim().parse::<i64>().ok())
            .unwrap_or(dft)
    };
    let from_seq = match (num(0, 1), num(1, 16_777_215)) {
        (0, _) => DupSeq::All,
        (a, -1) => DupSeq::Range(u32::try_from(a).unwrap_or(1), u32::try_from(a).unwrap_or(1)),
        (a, b) => DupSeq::Range(
            u32::try_from(a).unwrap_or(1),
            u32::try_from(b).unwrap_or(u32::MAX),
        ),
    };
    let to_seq = match parms.int("TOSEQNBR").unwrap_or(0) {
        0 => ToSeq::FromSeq,
        -1 => ToSeq::End,
        n => ToSeq::Number(u32::try_from(n).unwrap_or(1)),
    };
    let opts = DupOptions {
        from_vol: vols(parms, "FROMVOL").into_iter().next(),
        to_vol: vols(parms, "TOVOL").into_iter().next(),
        from_seq,
        to_seq,
        today: (parms.name("CHECK") != "*NO").then(stream::today),
        allow_active: parms.name("CHECK") == "*NO",
        from_end_opt: stream::end_opt(&parms.name("FROMENDOPT")),
        to_end_opt: stream::end_opt(&parms.name("TOENDOPT")),
    };
    let d = drive::dup_tape(system, &from, &to, &opts)?;
    let files = i32::try_from(d.files).unwrap_or(i32::MAX);
    Ok(done(&Msg::new(
        TAPE_DUPLICATED,
        &[
            Field::Char(&from, 10),
            Field::Char(&d.from_vol, 6),
            Field::Char(&to, 10),
            Field::Bin2(0),
            Field::Bin4(files),
            Field::Bin4(1),
            Field::Bin4(1),
            Field::Char(&d.to_vol, 6),
            Field::Char(&d.to_vol, 6),
        ],
    )))
}

// ------------------------------------------------------------------ optical

/// The optical volume a command names: `DEV` and `VOL`, or the volume
/// wherever it is loaded.
fn optical(
    system: &mut System,
    parms: &Parms,
    vol_kwd: &str,
    dev_kwd: &str,
) -> Result<(drive::Target, String), Fail> {
    let v = vol(parms, vol_kwd);
    let dev = parms.name(dev_kwd);
    if !dev.is_empty() && !dev.starts_with('*') {
        return drive::optical_volume(system, &dev, v.as_deref());
    }
    match v {
        Some(v) => drive::optical_by_volume(system, &v),
        None => Err(bad_parm(dev_kwd).detail(format_args!(
            "{dev_kwd} is needed with {vol_kwd}(*MOUNTED)"
        ))),
    }
}

/// The catalog entry holding volume `vol` on device `dev`: catalog,
/// index, image path.
fn entry_of(dev: &str, vol: &str) -> Result<(String, u16, String), Fail> {
    let c = media::catalog_on(dev)
        .ok_or_else(|| Fail::new(drive::OPT_EMPTY, &[Field::Char(dev, 10)]))?;
    let e = c
        .entries
        .iter()
        .find(|e| e.volume.eq_ignore_ascii_case(vol))
        .ok_or_else(|| Fail::new("OPT1332", &[Field::Char(dev, 10), Field::Char(vol, 32)]))?;
    Ok((c.name.clone(), e.index, c.path_of(e)))
}

/// An image that cannot be opened: `OPT1605` with the cause.
fn unreadable(dev: &str, vol: &str, why: impl std::fmt::Display) -> Fail {
    Fail::new("OPT1605", &[Field::Char(dev, 10), Field::Char(vol, 32)]).detail(why)
}

/// Take a catalog off its device so `/QOPT` lets go of its volume (its
/// pending changes written) before the image is rewritten.
fn release(clg: &str) -> Result<(), String> {
    media::with_catalogs(|r| r.lodimgclg(clg, LoadOption::Unload, ""))
}

/// Put an entry's image back into its catalog after it was rewritten:
/// removed (the file kept), added again at its index so its volume is
/// read anew, the catalog loaded on `dev` again and the entry mounted.
fn reenter(clg: &str, index: u16, path: &str, dev: &str) -> Result<(), String> {
    let file = imgclg::fs::file_name(path).to_string();
    media::with_catalogs(|r| {
        r.rmvimgclge(clg, index, true)?;
        let mut a = AddImgClgE::file(&file);
        a.index = Some(index);
        a.epoch = now_unix();
        r.addimgclge(clg, &a)?;
        r.lodimgclg(clg, LoadOption::Load, dev)?;
        r.lodimgclge(clg, index, EntryOption::Mount)
    })
}

fn inzopt(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let (t, volid) = optical(system, parms, "VOL", "DEV")?;
    let new = parms.text("NEWVOL").trim().to_ascii_uppercase();
    let new = if new.is_empty() || new == "*VOL" {
        volid.clone()
    } else {
        new
    };
    let (clg, index, path) = entry_of(&t.dev, &volid)?;
    release(&clg)?;
    let store = imgclg::StdFs::new(crate::stgsvc::ifs::ROOT);
    let img = imgclg::CatalogFs::image(&store, &path)
        .map_err(|e| unreadable(&t.dev, &volid, format_args!("{path}: {e}")))?;
    let p = optical::volume::ops::InzOpt {
        new_volume: new.clone(),
        check: parms.name("CHECK") != "*NO",
        clear: parms.yes("CLEAR"),
        text: text(parms).unwrap_or_default(),
        capacity: None,
        epoch: now_unix(),
    };
    let r = optical::volume::ops::inzopt(img, &p);
    // Back into the catalog and /QOPT whatever happened.
    reenter(&clg, index, &path, &t.dev)?;
    r.map_err(|e| drive::optical_fail(system, &t, &MediaError::Optical(e), &volid))?;
    Ok(done(&Msg::new(
        OPT_INITIALIZED,
        &[Field::Char(&t.dev, 10), Field::Char(&new, 32)],
    )))
}

/// `DSPOPT`'s view of the volume its parameters name.
pub(crate) fn optical_view_of(
    system: &mut System,
    parms: &Parms,
) -> Result<meddsp::OptView, Fail> {
    let path = parms.text("PATH");
    meddsp::optical_view(
        system,
        &parms.text("VOL"),
        &parms.name("DEV"),
        &parms.name("DATA"),
        &path,
    )
}

/// The text form, and the printed report, of `DSPOPT`.
fn optical_lines(v: &meddsp::OptView) -> Vec<String> {
    let mut s = Vec::new();
    match v {
        meddsp::OptView::Volume(o) => {
            let a = &o.attrs;
            let yn = |b: bool| if b { "*YES" } else { "*NO" };
            let rows = [
                ("Volume identifier", a.volume_id.clone()),
                ("Optical device", o.dev.clone()),
                ("Text", a.text.clone()),
                ("Media format", a.media_format.to_string()),
                ("Writable volume", yn(a.writable_layout).to_string()),
                ("Write protected", yn(a.write_protected).to_string()),
                ("Capacity (bytes)", a.capacity.to_string()),
                ("Space used (bytes)", a.used.to_string()),
                ("Space available (bytes)", a.free.to_string()),
                ("Block size", a.block_size.to_string()),
                ("Volume set", a.volume_set.clone()),
                ("Volumes in set", a.set_size.to_string()),
                ("Sequence in set", a.sequence.to_string()),
            ];
            for (k, v) in rows {
                s.push(format!("{k:.<30} : {v}"));
            }
        }
        meddsp::OptView::Dirs(_, _, dirs) => {
            for d in dirs {
                s.push(format!(
                    "{:<40} files {:>6}  directories {:>6}  bytes {:>12}",
                    d.path, d.files, d.dirs, d.bytes
                ));
            }
        }
        meddsp::OptView::Files(_, _, files) => {
            for f in files {
                s.push(format!("{:<50} {:>12}", f.path, f.size));
            }
        }
        meddsp::OptView::Saves(_, _, saves) => {
            for (f, (desc, rows)) in saves {
                s.push(format!("Optical file {f}"));
                for (k, v) in desc {
                    s.push(format!("  {k:<24} {v}"));
                }
                s.push(format!(
                    "  {:<10} {:<10} {:<10} {:<10} {:>12}  Text",
                    "Object", "Type", "Library", "Attribute", "Size"
                ));
                for r in rows {
                    let c = |i: usize| r.get(i).map_or("", String::as_str);
                    s.push(format!(
                        "  {:<10} {:<10} {:<10} {:<10} {:>12}  {}",
                        c(0),
                        c(1),
                        c(2),
                        c(3),
                        c(4),
                        c(5)
                    ));
                }
            }
        }
    }
    s
}

fn dspopt(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    if parms.name("OUTPUT") == "*OUTFILE" && parms.name("DATA") == "*SAVRST" {
        return Err(bad_parm("OUTPUT").detail("OUTPUT(*OUTFILE) is not valid with DATA(*SAVRST)"));
    }
    let v = optical_view_of(system, parms)?;
    match parms.name("OUTPUT").as_str() {
        "*OUTFILE" => super::medout::optical(system, parms, &v)
            .map(plain)
            .map_err(Fail::from),
        "*PRINT" => {
            crate::svc::security::print_as(system, "QSYSPRT", "DSPOPT", &optical_lines(&v), false)
                .map(plain)
                .map_err(Fail::from)
        }
        _ => Ok(plain(optical_lines(&v).join("\n") + "\n")),
    }
}

fn chkopt(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let (t, volid) = optical(system, parms, "VOL", "DEV")?;
    let r = media::volume_set()
        .with(&volid, |v| Ok(optical::volume::ops::chkopt(v)))
        .map_err(|e| drive::optical_fail(system, &t, &MediaError::Optical(e), &volid))?;
    if !r.errors.is_empty() {
        let list: Vec<String> = r.errors.iter().map(|(p, why)| format!("{p}: {why}")).collect();
        let e = MediaError::Optical(optical::Error::Malformed(list.join("; ")));
        return Err(drive::optical_fail(system, &t, &e, &volid));
    }
    let files = i32::try_from(r.files).unwrap_or(i32::MAX);
    Ok(done(&Msg::new(
        OPT_CHECKED,
        &[
            Field::Char(&t.dev, 10),
            Field::Char(&volid, 32),
            Field::Bin4(files),
        ],
    )))
}

fn dupopt(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let (ft, from) = optical(system, parms, "FROMVOL", "FROMDEV")?;
    let (t, to) = optical(system, parms, "TOVOL", "TODEV")?;
    if from.eq_ignore_ascii_case(&to) {
        return Err(Fail::new("OPT1339", &[Field::Char(&from, 32)]));
    }
    let new = parms.text("NEWVOL").trim().to_ascii_uppercase();
    let new_volume = match new.as_str() {
        "" | "*TOVOL" => Some(to.clone()),
        "*FROMVOL" => None,
        n => Some(n.to_string()),
    };
    let (clg, index, path) = entry_of(&t.dev, &to)?;
    let set = media::volume_set();
    set.sync(&from)
        .map_err(|e| drive::optical_fail(system, &ft, &MediaError::Optical(e), &from))?;
    release(&clg)?;
    let store = imgclg::StdFs::new(crate::stgsvc::ifs::ROOT);
    let img = imgclg::CatalogFs::image(&store, &path)
        .map_err(|e| unreadable(&t.dev, &to, format_args!("{path}: {e}")))?;
    let target_vol = new_volume.clone().unwrap_or_else(|| from.clone());
    let p = optical::volume::ops::DupOpt {
        new_volume,
        text: None,
        capacity: None,
        epoch: now_unix(),
    };
    let r = set.with(&from, |v| {
        optical::volume::ops::dupopt(v, img, &p).map(|_| ())
    });
    reenter(&clg, index, &path, &t.dev)?;
    r.map_err(|e| drive::optical_fail(system, &t, &MediaError::Optical(e), &to))?;
    Ok(done(&Msg::new(
        OPT_DUPLICATED,
        &[
            Field::Char(&from, 32),
            Field::Char(&t.dev, 10),
            Field::Char(&target_vol, 32),
        ],
    )))
}

fn wrkoptvol(system: &mut System, parms: &Parms) -> Result<String, Fail> {
    let rows = meddsp::optical_volumes(system, &parms.name("DEV"), &parms.text("VOL"))?;
    let mut s = String::from(
        "Volume                            Device      Volume Type  Media Type  Catalog     Protected\n",
    );
    for r in &rows {
        let _ = writeln!(
            s,
            "{:<32}  {:<10}  {:<11}  {:<10}  {:<10}  {}",
            r.vol,
            r.dev,
            r.vol_type,
            r.media_type,
            r.catalog,
            if r.protected { "*YES" } else { "*NO" }
        );
    }
    Ok(s)
}

// ------------------------------------------------------------ media library

/// Cartridges changed (added, moved to a category, ejected).
const CTGS_CHANGED: &str = "CPC67F8";

/// `CPC67F8`: `n` cartridges of `dev`, the first `first`, now in
/// category `cgy`.
fn ctgs_changed(dev: &str, first: &str, cgy: &str, n: usize) -> CmdReply {
    done(&Msg::new(
        CTGS_CHANGED,
        &[
            Field::Char(dev, 10),
            Field::Char(first, 6),
            Field::Char("", 6),
            Field::Char(cgy, 10),
            Field::Char("", 8),
            Field::Bin2(i16::try_from(n).unwrap_or(i16::MAX)),
        ],
    ))
}

/// `WRKMLBSTS`'s text form: each library, its status, and for a virtual
/// one its drive resources with their allocation and cartridge.
pub(crate) fn wrkmlbsts_text(system: &mut System, which: &str) -> String {
    let mut s = String::from("Device/Resource   Status        Allocation     Cartridge\n");
    let devs = devices::all(system).unwrap_or_default();
    for (n, d) in devs
        .iter()
        .filter(|(_, d)| d.media_kind() == Some(MediaKind::TapeLibrary))
    {
        if !(which.is_empty()
            || which == "*ALL"
            || *n == which
            || which.strip_suffix('*').is_some_and(|p| n.starts_with(p)))
        {
            continue;
        }
        let _ = writeln!(s, "{n:<16}  {:<12}", d.status.text());
        let Some(m) = &d.media else { continue };
        match media::library::drives(system, n) {
            Ok(rows) => {
                for r in rows {
                    let _ = writeln!(
                        s,
                        "  {:<14}  {:<12}  {:<13}  {}",
                        r.name, "OPERATIONAL", r.allocation, r.cartridge
                    );
                }
            }
            Err(_) => {
                let _ = writeln!(
                    s,
                    "  {:<14}  {:<12}  *ALLOCATED",
                    m.rsrcname,
                    d.status.text()
                );
            }
        }
    }
    s
}

/// The `CTG` values, upper case.
fn ctgs(parms: &Parms) -> Vec<String> {
    parms
        .list("CTG")
        .iter()
        .map(|v| v.source().trim().trim_matches('\'').to_ascii_uppercase())
        .filter(|v| !v.is_empty())
        .collect()
}

/// The category the `CGY` parameter names (its first element).
fn cgy(parms: &Parms) -> String {
    parms
        .elems("CGY")
        .first()
        .copied()
        .flatten()
        .map(|v| v.source().trim().to_ascii_uppercase())
        .unwrap_or_default()
}

/// `WRKTAPCTG` and `DSPTAPCTG`'s rows of library `dev`.
pub(crate) fn tapctg_rows(
    system: &mut System,
    dev: &str,
    ids: &[String],
    cgy: &str,
) -> Result<Vec<media::library::CtgRow>, Fail> {
    media::library::rows(system, dev, ids, cgy)
}

/// `DSPTAPCTG`'s cartridges as text, printed, or in an outfile.
fn dsptapctg(system: &mut System, parms: &Parms) -> Result<CmdReply, Fail> {
    let all = meddsp::cartridges(system, &parms.name("DEV"), &ctgs(parms), &cgy(parms))?;
    if parms.name("OUTPUT") == "*OUTFILE" {
        return super::medout::cartridges(system, parms, &all)
            .map(plain)
            .map_err(Fail::from);
    }
    let mut s = Vec::new();
    for c in &all {
        let r = &c.row;
        s.push(format!("Library device  . . . : {}", c.dev));
        s.push(format!("Cartridge ID  . . . . : {}", r.id));
        s.push(format!("Volume ID . . . . . . : {}", r.volume));
        s.push(format!("Category  . . . . . . : {}", r.category));
        s.push(format!("Status  . . . . . . . : {}", r.status));
        s.push(format!("Location  . . . . . . : {}", r.location));
        s.push(format!("Home slot . . . . . . : {}", r.home));
        s.push(String::new());
    }
    if parms.name("OUTPUT") == "*PRINT" {
        return crate::svc::security::print_as(system, "QSYSPRT", "DSPTAPCTG", &s, false)
            .map(plain)
            .map_err(Fail::from);
    }
    Ok(plain(s.join("\n") + "\n"))
}

#[allow(clippy::too_many_lines)]
fn mlb_ctg(system: &mut System, name: &str, parms: &Parms) -> Result<CmdReply, Fail> {
    let dev = parms.name("DEV").to_ascii_uppercase();
    let d = read_dev(system, &dev)?;
    if d.media_kind() != Some(MediaKind::TapeLibrary) {
        return Err(Fail::new(
            media::library::NOT_LIBRARY,
            &[Field::Char(&dev, 10)],
        ));
    }
    let ids = ctgs(parms);
    let cgy = cgy(parms);
    match name {
        "WRKTAPCTG" => {
            let rows = tapctg_rows(system, &dev, &ids, &cgy)?;
            let mut s = format!("Library device  . . . : {dev}\n");
            let _ = writeln!(s, "Cartridge  Volume  Category    Status      Location");
            for r in &rows {
                let _ = writeln!(
                    s,
                    "{:<9}  {:<6}  {:<10}  {:<10}  {}",
                    r.id, r.volume, r.category, r.status, r.location
                );
            }
            Ok(plain(s))
        }
        "ADDTAPCTG" | "CHGTAPCTG" => {
            let to = if cgy.is_empty() || cgy == "*SAME" {
                None
            } else {
                Category::parse(&cgy)
            };
            if name == "ADDTAPCTG" && to.is_none() {
                return Err(bad_parm("CGY").detail(format_args!("CGY({cgy}) is not a category")));
            }
            let check = name == "ADDTAPCTG" && parms.name("CHECK") != "*NO";
            let all_inserted = ids.iter().any(|i| i == "*INSERT");
            let ids = if all_inserted {
                tapctg_rows(system, &dev, &[], "*INSERT")?
                    .into_iter()
                    .map(|r| r.id)
                    .collect()
            } else {
                ids
            };
            if check {
                for id in &ids {
                    if let Some(v) = media::library::label_of(system, &dev, id)?
                        && v != *id
                        && !matches!(v.as_str(), "*NL" | "*BLANK" | "*UNKNOWN")
                    {
                        return Err(Fail::new(
                            "CPF67A0",
                            &[
                                Field::Char(&dev, 10),
                                Field::Char(id, 6),
                                Field::Char(&v, 6),
                            ],
                        ));
                    }
                }
            }
            let first = ids.first().cloned().unwrap_or_default();
            let Some(to) = to else {
                return Ok(ctgs_changed(&dev, &first, "*SAME", 0));
            };
            media::library::with_library(system, &dev, |_, _, lib| {
                for id in &ids {
                    let r = if name == "ADDTAPCTG" {
                        lib.add(id, to.clone())
                    } else {
                        lib.change(id, to.clone())
                    };
                    r.map_err(|e| media::library::escape(&e, &dev))?;
                }
                Ok::<_, Fail>(())
            })?;
            Ok(ctgs_changed(&dev, &first, to.name(), ids.len()))
        }
        "RMVTAPCTG" => {
            media::library::with_library(system, &dev, |_, _, lib| {
                for id in &ids {
                    lib.remove(id)
                        .map_err(|e| media::library::escape(&e, &dev))?;
                }
                Ok::<_, Fail>(())
            })?;
            let first = ids.first().cloned().unwrap_or_default();
            Ok(ctgs_changed(&dev, &first, "*EJECT", ids.len()))
        }
        _ => Err(Fail::new("CPF6708", &[]).detail(format_args!("{name} is not a cartridge command"))),
    }
}

// ------------------------------------------------------------------ panels

/// A panel variable.
#[allow(clippy::needless_pass_by_value)]
fn var(k: &str, v: impl ToString) -> (String, String) {
    (k.to_string(), v.to_string())
}

/// `*READY` as the panels show it.
fn ready(s: &str) -> &'static str {
    if s == "*READY" { "Ready" } else { "Not ready" }
}

/// A catalog's type as the panels show it (the 7.1 oracle's
/// `Optical`).
fn kind(k: &str) -> &str {
    match k {
        "*OPT" => "Optical",
        "*TAP" => "Tape",
        other => other,
    }
}

/// `WRKIMGCLG`'s panel data: one row per catalog `which` and `ty`
/// select, with its device and the device's status.
pub(crate) fn imgclg_panel(system: &mut System, which: &str, ty: &str) -> stg::proto::PanelData {
    let rows = media::with_catalogs(|r| Ok(r.wrkimgclg())).unwrap_or_default();
    let mut out = Vec::new();
    for c in rows {
        let pass = (which.is_empty()
            || which == "*ALL"
            || c.name == which
            || which
                .strip_suffix('*')
                .is_some_and(|p| c.name.starts_with(p)))
            && (ty.is_empty() || ty == "*ALL" || ty == c.kind);
        if !pass {
            continue;
        }
        let dev = media::with_catalogs(|r| r.wrkimgclge(&c.name))
            .map(|d| d.device)
            .unwrap_or_default();
        let dev = if dev == "*NONE" { String::new() } else { dev };
        let sts = if dev.is_empty() {
            String::new()
        } else {
            devices::read(system, &dev)
                .map(|d| d.status.text().to_string())
                .unwrap_or_default()
        };
        out.push(vec![
            var("CLG", &c.name),
            var("CLGSTS", ready(c.status)),
            var("CLGTYPE", kind(c.kind)),
            var("CLGASP", "*NONE"),
            var("CLGDEV", dev),
            var("DEVSTS", sts),
            var("CLGTEXT", &c.text),
        ]);
    }
    (Vec::new(), out)
}

/// `WRKIMGCLGE`'s panel data.
pub(crate) fn imgclge_panel(name: &str) -> Result<stg::proto::PanelData, String> {
    let d = media::with_catalogs(|r| r.wrkimgclge(name))?;
    let word = |s: &str| match s {
        "*MOUNTED" => "Mounted",
        "*LOADED" => "Loaded",
        _ => "Available",
    };
    let vars = vec![
        var("CLG", &d.name),
        var("CLGSTS", ready(d.status)),
        var("CLGTYPE", kind(d.kind)),
        var("CLGDEV", &d.device),
        var("CLGDIR", &d.dir),
    ];
    let rows = d
        .entries
        .iter()
        .map(|e| {
            vec![
                var("IDX", e.index),
                var("ENTSTS", word(e.status)),
                var("WRTPTC", if e.write_protect == "*YES" { "Y" } else { "N" }),
                var("VOLNAM", &e.volume),
                var("IMGFILE", &e.file),
            ]
        })
        .collect();
    Ok((vars, rows))
}

/// `WRKTAPCTG`'s panel data.
pub(crate) fn tapctg_panel(
    system: &mut System,
    dev: &str,
    ids: &[String],
    cgy: &str,
) -> Result<stg::proto::PanelData, String> {
    let rows = tapctg_rows(system, dev, ids, cgy)?
        .into_iter()
        .map(|r| {
            vec![
                var("CTG", r.id),
                var("CTGVOL", r.volume),
                var("CTGCGY", r.category),
                var("CTGSTS", r.status),
                var("CTGLOC", r.location),
            ]
        })
        .collect();
    Ok((vec![var("MLBDEV", dev)], rows))
}

/// `WRKMLBSTS`'s panel data: each library, then its drives indented.
pub(crate) fn mlbsts_panel(system: &mut System, which: &str) -> stg::proto::PanelData {
    let mut out = Vec::new();
    let devs = devices::all(system).unwrap_or_default();
    for (n, d) in devs
        .iter()
        .filter(|(_, d)| d.media_kind() == Some(MediaKind::TapeLibrary))
    {
        if !(which.is_empty()
            || which == "*ALL"
            || *n == which
            || which.strip_suffix('*').is_some_and(|p| n.starts_with(p)))
        {
            continue;
        }
        out.push(vec![
            var("RSRC", n),
            var("RSRCSTS", d.status.text()),
            var("RSRCLIB", n),
        ]);
        let drives = media::library::drives(system, n).unwrap_or_default();
        for r in drives {
            out.push(vec![
                var("RSRC", format!("  {}", r.name)),
                var("RSRCSTS", "OPERATIONAL"),
                var("ALLOC", r.allocation),
                var("RSRCCTG", r.cartridge),
                var("RSRCLIB", n),
            ]);
        }
    }
    (Vec::new(), out)
}

/// The panels' library actions (`service_op`): `mlb-mount DEV CTG`,
/// `mlb-demount DEV CTG`, `mlb-alloc DEV DRIVE ALLOCATION`. The message
/// for the message line: its identifier, then each value after a tab.
pub(crate) fn mlb_op(
    system: &mut System,
    op: &str,
    args: &[String],
) -> Result<Vec<String>, String> {
    let arg = |i: usize| args.get(i).map_or("", |s| s.trim()).to_ascii_uppercase();
    let (dev, what) = (arg(0), arg(1));
    media::library::with_library(system, &dev, |_, _, lib| {
        let line = match op {
            "mlb-mount" => {
                let i = lib.mount(&what).map_err(|e| media::library::refused(&e))?;
                format!("CPI9BB5\t{what}\t{}", lib.model.drives[i].name)
            }
            "mlb-demount" => {
                let i = lib
                    .model
                    .drives
                    .iter()
                    .position(|d| d.cartridge.as_deref() == Some(what.as_str()))
                    .ok_or_else(|| format!("CPF67A8 cartridge {what} is not mounted"))?;
                lib.demount(i).map_err(|e| media::library::refused(&e))?;
                format!("CPI9BB6\t{what}")
            }
            "mlb-alloc" => {
                let a = match arg(2).as_str() {
                    "*UNPROTECTED" => tape::device::Allocation::Unprotected,
                    "*DEALLOCATED" => tape::device::Allocation::Deallocated,
                    _ => tape::device::Allocation::Allocated,
                };
                lib.allocate(&what, a)
                    .map_err(|e| media::library::refused(&e))?;
                format!("CPI9BB7\t{what}\t{}", a.name())
            }
            _ => return Err(format!("CPF9899 no library action {op}")),
        };
        Ok(vec![line])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stgsvc::hist::message_text;

    /// The completion's message, its text filled from the catalogue.
    fn shown(r: &CmdReply) -> (String, Vec<u8>, String) {
        assert!(r.text.is_empty(), "no words of the command's own");
        let m = &r.messages[0];
        (m.msgid.clone(), m.data.clone(), message_text(&m.msgid, &m.data))
    }

    #[test]
    fn inztap_completions() {
        let r = done(&medio::msg::tape_initialized("TAP01", Some("PRT001"), "*DEVTYPE"));
        let (id, data, text) = shown(&r);
        assert_eq!(id, "CPC6773");
        assert_eq!(
            data,
            medio::msg::data(&[Field::Char("TAP01", 10), Field::Char("PRT001", 6)])
        );
        assert!(text.contains("PRT001") && text.contains("TAP01"), "{text}");
        let (id, data, _) = shown(&done(&medio::msg::tape_initialized("TAP01", None, "*DEVTYPE")));
        assert_eq!((id.as_str(), data.len()), ("CPC6776", 20));
    }

    #[test]
    fn write_protected_escape() {
        let r = Fail::new("CPF6768", &[Field::Char("TAP01", 10)]).reply();
        assert!(r.text.is_empty());
        let e = r.escape.expect("escape");
        assert_eq!(e.msgid, "CPF6768");
        let text = message_text(&e.msgid, &e.data);
        assert!(text.contains("TAP01") && !text.contains('&'), "{text}");
    }

    #[test]
    fn cartridges_changed() {
        let (id, data, text) = shown(&ctgs_changed("TAPMLB01", "CTG001", "*SHARE400", 3));
        assert_eq!(id, "CPC67F8");
        assert_eq!(data.len(), 10 + 6 + 6 + 10 + 8 + 2);
        assert_eq!(&data[data.len() - 2..], &[0, 3]);
        assert!(text.contains('3') && text.contains("TAPMLB01"), "{text}");
    }

    #[test]
    fn catalog_refusals() {
        let f = clg_fail(
            &imgclg::Error::NotFound("X".into()),
            "QS72TAP",
            "CPFBC28",
            &[],
        );
        assert_eq!(f.msgid, "CPFBC45");
        assert_eq!(f.data, medio::msg::data(&[Field::Char("QS72TAP", 10)]));
        let f = clg_fail(
            &imgclg::Error::Full("QS72TAP".into()),
            "QS72TAP",
            "CPFBC28",
            &[Field::Char("QS72TAP", 10)],
        );
        assert_eq!(f.msgid, "CPFBC28");
        assert!(!f.detail.is_empty(), "the cause goes before the escape");
        let r = f.reply();
        assert_eq!(r.messages[0].msgid, "CPF9897");
    }
}
