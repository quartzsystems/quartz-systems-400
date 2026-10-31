// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The media commands in the service: the tape, optical and media
//! library device descriptions (`CRTDEVTAP`, `CRTDEVOPT`, `CRTDEVMLB`,
//! `CHGDEVxxx`), the image catalog commands, and the tape and optical
//! commands (`INZTAP`, `DSPTAP`, `CHKTAP`, `DUPTAP`, `INZOPT`, `DSPOPT`,
//! `CHKOPT`, `DUPOPT`, `WRKOPTVOL`). The work is `stgsvc::media`'s and
//! the `tape`, `optical` and `imgclg` crates'; here are the parameters,
//! the text forms and the messages.

use std::fmt::Write as _;

use cmd::block::Parms;
use imgclg::ops::{AddImgClgE, AddVrtVol, ChgImgClgE, CrtImgClg, EntryOption, FromFile, LoadOption, ToFile, VfyImgClg, VfyType};
use imgclg::ImgType;
use medio::MediaError;
use stg::proto::CmdReply;
use tape::volume::{Check, ChkOptions, DupOptions, DupSeq, FileReader, InzOptions, ReadOptions, ToSeq, VolumeKind};
use tape::device::Category;
use tape::{MAX_BLOCK, SeqNbr, TapeTransport, VolumeProvider};
use wm::obj::devd::Devd;
use wm::obj::devmed::{MediaDevice, MediaKind};

use super::direct::failed;
use super::done;
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::media::drive::{self, TapeJob};
use crate::stgsvc::media::{self, catalogs, devices, stream};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "CRTDEVTAP" | "CRTDEVOPT" | "CRTDEVMLB" => crtdev(system, name, parms),
        "CHGDEVTAP" | "CHGDEVOPT" | "CHGDEVMLB" => chgdev(system, name, parms),
        "CRTIMGCLG" => crtimgclg(system, parms),
        "DLTIMGCLG" => dltimgclg(system, parms),
        "ADDIMGCLGE" => addimgclge(parms),
        "RMVIMGCLGE" => rmvimgclge(parms),
        "CHGIMGCLGE" => chgimgclge(parms),
        "LODIMGCLG" => lodimgclg(system, parms),
        "LODIMGCLGE" => lodimgclge(parms),
        "VFYIMGCLG" => vfyimgclg(parms),
        "WRKIMGCLG" => Ok(plain(wrkimgclg_text(&parms.name("IMGCLG"), &parms.name("TYPE")))),
        "WRKIMGCLGE" => wrkimgclge_text(&parms.name("IMGCLG")).map(plain),
        "INZTAP" => inztap(system, parms),
        "DSPTAP" => dsptap(system, parms),
        "CHKTAP" => chktap(system, parms),
        "DUPTAP" => duptap(system, parms),
        "INZOPT" => inzopt(system, parms),
        "DSPOPT" => dspopt(system, parms),
        "CHKOPT" => chkopt(system, parms),
        "DUPOPT" => dupopt(system, parms),
        "WRKOPTVOL" => Ok(plain(wrkoptvol(system))),
        "WRKMLBSTS" => Ok(plain(wrkmlbsts_text(system, &parms.name("MLB")))),
        "DSPTAPCTG" | "WRKTAPCTG" | "ADDTAPCTG" | "RMVTAPCTG" | "CHGTAPCTG" => mlb_ctg(system, name, parms),
        _ => return None,
    };
    Some(match r {
        Ok(reply) => reply,
        Err(e) => failed(name, &e),
    })
}

/// Device description created; changed.
const DEV_CREATED: &str = "CPC2618";
const DEV_CHANGED: &str = "CPC2619";
/// Image catalog created, deleted, entry added, removed, changed,
/// catalog loaded, unloaded, verified, entry loaded.
const CLG_CREATED: &str = "CPCBC01";
const CLG_DELETED: &str = "CPCBC02";
const CLGE_ADDED: &str = "CPCBC03";
const CLGE_REMOVED: &str = "CPCBC04";
const CLGE_CHANGED: &str = "CPCBC05";
const CLG_LOADED: &str = "CPCBC06";
const CLG_UNLOADED: &str = "CPCBC07";
const CLG_VERIFIED: &str = "CPCBC08";
const CLGE_LOADED: &str = "CPCBC09";
/// Volume initialized, checked, duplicated.
const VOL_INITIALIZED: &str = "CPC6773";
const VOL_CHECKED: &str = "CPC6774";
const VOL_DUPLICATED: &str = "CPC6775";

/// A completion: its message, and the text led by its identifier.
fn ok(text: &str, msgid: &str, fields: &[(&str, usize)]) -> CmdReply {
    done(format!("{msgid} {text}"), msgid, fields)
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
        n => Some(format!("{}/{n}", if l.is_empty() || l.starts_with('*') { "QSYS".into() } else { l.to_ascii_uppercase() })),
    }
}

fn kind_of(cmd: &str) -> MediaKind {
    match &cmd[cmd.len() - 3..] {
        "TAP" => MediaKind::Tape,
        "OPT" => MediaKind::Optical,
        _ => MediaKind::TapeLibrary,
    }
}

fn crtdev(system: &mut System, cmd: &str, parms: &Parms) -> Result<CmdReply, String> {
    let kind = kind_of(cmd);
    if kind == MediaKind::TapeLibrary && parms.name("DEVCLS") == "*OPT" {
        return Err("CPD0084 DEVCLS(*OPT): optical media libraries are not available in this release".into());
    }
    let name = parms.name("DEVD");
    let rsrc_in = parms.name("RSRCNAME");
    let virt = rsrc_in == "*VRT" || parms.name("TYPE") == "*VRT";
    let rsrc = if virt {
        devices::next_virtual(system, kind)?
    } else if rsrc_in == "*NONE" {
        String::new()
    } else {
        rsrc_in
    };
    let (dt, dm) = if virt { kind.virtual_type() } else { devices::real_type(kind) };
    let pick = |kwd: &str, dft: &str| {
        let v = parms.name(kwd);
        if v.is_empty() || v.starts_with('*') { dft.to_string() } else { v }
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
    devices::create(system, &name, &d, &who)?;
    Ok(ok(&format!("Description for device {name} created.\n"), DEV_CREATED, &[(&name, 10)]))
}

fn chgdev(system: &mut System, cmd: &str, parms: &Parms) -> Result<CmdReply, String> {
    let name = parms.name("DEVD");
    let mut d = devices::read(system, &name)?;
    let kind = kind_of(cmd);
    let Some(m) = d.media.as_mut() else { unreachable!("read checks") };
    if m.kind != kind {
        return Err(format!("{} device {name} is a {} device", devices::NOT_FOUND, m.kind.devcls()));
    }
    let rsrc = parms.name("RSRCNAME");
    if !rsrc.is_empty() && rsrc != "*SAME" {
        if devices::usable(&d) {
            return Err(format!("{} device {name} is varied on", cmncfg::msgs::MUST_VARY_OFF));
        }
        let new = if rsrc == "*VRT" { devices::next_virtual(system, kind)? } else { rsrc };
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
    for (kwd, slot) in [("UNLOADWAIT", &mut m.unload_wait), ("MAXDEVTIME", &mut m.max_dev_time)] {
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
    devices::write(system, &name, &d)?;
    Ok(ok(&format!("Description for device {name} changed.\n"), DEV_CHANGED, &[(&name, 10)]))
}

// ----------------------------------------------------------------- catalogs

fn crtimgclg(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let name = parms.name("IMGCLG");
    if parms.yes("IMPORT") {
        return Err("CPD0084 IMPORT(*YES) is not available in this release".into());
    }
    let kind = if parms.name("TYPE") == "*TAP" { ImgType::Tap } else { ImgType::Opt };
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
    };
    media::with_catalogs(|r| r.crtimgclg(&p))?;
    let who = owner(system);
    if let Err(e) = catalogs::create_object(system, &name, &format!("/{}", p.dir.trim_matches('/')), &text, &who) {
        let _ = media::with_catalogs(|r| r.dltimgclg(&name, true));
        return Err(e);
    }
    Ok(ok(&format!("Image catalog {name} created.\n"), CLG_CREATED, &[(&name, 10)]))
}

fn dltimgclg(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let want = parms.name("IMGCLG");
    let keep = parms.name("KEEP") != "*NO";
    let names: Vec<String> = media::with_catalogs(|r| Ok(r.names()))?
        .into_iter()
        .filter(|n| *n == want || want.strip_suffix('*').is_some_and(|p| n.starts_with(p)))
        .collect();
    if names.is_empty() {
        return Err(format!("CPFBC01 image catalog {want} does not exist"));
    }
    let mut text = String::new();
    for n in &names {
        media::with_catalogs(|r| r.dltimgclg(n, keep))?;
        catalogs::delete_object(system, n)?;
        let _ = writeln!(text, "Image catalog {n} deleted.");
    }
    Ok(ok(&text, CLG_DELETED, &[(&names[0], 10)]))
}

fn index_of(parms: &Parms) -> Option<u16> {
    parms.int("IMGCLGIDX").and_then(|i| u16::try_from(i).ok()).filter(|i| *i > 0)
}

fn addimgclge(parms: &Parms) -> Result<CmdReply, String> {
    let name = parms.name("IMGCLG");
    if parms.specified("FROMDEV") {
        return Err("CPD0084 FROMDEV: images from an optical device are not available in this release; give FROMFILE".into());
    }
    let from = parms.text("FROMFILE");
    let from = from.trim().trim_matches('\'');
    if from.is_empty() {
        return Err("CPD0084 FROMFILE is needed".into());
    }
    let to = parms.text("TOFILE");
    let to = to.trim().trim_matches('\'');
    let vol = parms.text("VOLNAM");
    let vol = vol.trim().trim_matches('\'');
    let p = AddImgClgE {
        from: if from.eq_ignore_ascii_case("*NEW") { FromFile::New } else { FromFile::Path(from.to_string()) },
        to_file: match to.to_ascii_uppercase().as_str() {
            "" | "*GEN" => ToFile::Gen,
            "*FROMFILE" => ToFile::FromFile,
            _ => ToFile::Name(to.to_string()),
        },
        index: index_of(parms),
        size_mb: parms.int("IMGSIZ").and_then(|n| u64::try_from(n).ok()).or(Some(4700)),
        volume: (!vol.is_empty() && !vol.starts_with('*')).then(|| vol.to_ascii_uppercase()),
        write_protect: parms.specified("WRTPTC").then(|| parms.yes("WRTPTC")),
        text: text(parms).unwrap_or_default(),
        epoch: now_unix(),
    };
    let replace = parms.yes("REPLACE");
    let i = media::with_catalogs(|r| {
        if replace && let Some(i) = p.index {
            let _ = r.rmvimgclge(&name, i, true);
        }
        r.addimgclge(&name, &p)
    })?;
    Ok(ok(&format!("Entry {i} added to image catalog {name}.\n"), CLGE_ADDED, &[(&name, 10), (&i.to_string(), 3)]))
}

fn rmvimgclge(parms: &Parms) -> Result<CmdReply, String> {
    let name = parms.name("IMGCLG");
    let keep = parms.name("KEEP") != "*NO";
    let vol = parms.text("VOL").trim().to_ascii_uppercase();
    let i = media::with_catalogs(|r| {
        let i = match index_of(parms) {
            Some(i) => i,
            None => r
                .catalog(&name)?
                .entries
                .iter()
                .find(|e| e.volume.eq_ignore_ascii_case(&vol))
                .map(|e| e.index)
                .ok_or_else(|| imgclg::Error::Invalid(format!("volume {vol} is not in image catalog {name}")))?,
        };
        r.rmvimgclge(&name, i, keep).map(|()| i)
    })?;
    Ok(ok(&format!("Entry {i} removed from image catalog {name}.\n"), CLGE_REMOVED, &[(&name, 10), (&i.to_string(), 3)]))
}

fn chgimgclge(parms: &Parms) -> Result<CmdReply, String> {
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
    media::with_catalogs(|r| r.chgimgclge(&name, i, &p))?;
    Ok(ok(&format!("Entry {i} of image catalog {name} changed.\n"), CLGE_CHANGED, &[(&name, 10), (&i.to_string(), 3)]))
}

fn lodimgclg(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let name = parms.name("IMGCLG");
    let unload = parms.name("OPTION") == "*UNLOAD";
    let dev = parms.name("DEV");
    if unload {
        media::with_catalogs(|r| r.lodimgclg(&name, LoadOption::Unload, ""))?;
        return Ok(ok(&format!("Image catalog {name} unloaded.\n"), CLG_UNLOADED, &[(&name, 10)]));
    }
    if dev.is_empty() {
        return Err("CPD0084 DEV is needed for OPTION(*LOAD)".into());
    }
    let d = devices::read(system, &dev)?;
    let m = d.media.unwrap_or_default();
    let kind = media::with_catalogs(|r| Ok(r.catalog(&name)?.kind))?;
    let fits = match kind {
        ImgType::Opt => m.kind == MediaKind::Optical,
        ImgType::Tap => m.kind != MediaKind::Optical,
    };
    if !m.is_virtual() || !fits {
        return Err(format!(
            "CPFBC0B device {dev} is not a virtual {} device",
            if kind == ImgType::Opt { "optical" } else { "tape" }
        ));
    }
    media::with_catalogs(|r| r.lodimgclg(&name, LoadOption::Load, &dev))?;
    Ok(ok(&format!("Image catalog {name} loaded on device {dev}.\n"), CLG_LOADED, &[(&name, 10), (&dev, 10)]))
}

fn lodimgclge(parms: &Parms) -> Result<CmdReply, String> {
    let name = parms.name("IMGCLG");
    let i = index_of(parms).unwrap_or(0);
    let opt = match parms.name("OPTION").as_str() {
        "*UNLOAD" => EntryOption::Unload,
        "*MOUNT" => EntryOption::Mount,
        _ => EntryOption::Load,
    };
    media::with_catalogs(|r| r.lodimgclge(&name, i, opt))?;
    Ok(ok(&format!("Entry {i} of image catalog {name} changed.\n"), CLGE_LOADED, &[(&name, 10), (&i.to_string(), 3)]))
}

fn vfyimgclg(parms: &Parms) -> Result<CmdReply, String> {
    let name = parms.name("IMGCLG");
    let p = VfyImgClg {
        kind: match parms.name("TYPE").as_str() {
            "*PTF" => VfyType::Ptf,
            "*LIC" => VfyType::Lic,
            "*OTHER" => VfyType::Other,
            _ => VfyType::Upgrade,
        },
        sort: parms.yes("SORT"),
    };
    let r = media::with_catalogs(|r| r.vfyimgclg(&name, &p))?;
    let mut text = String::new();
    for prob in &r.problems {
        let _ = writeln!(text, "CPFBC0D {prob:?}");
    }
    if !r.ok() {
        return Err(format!("CPFBC0D image catalog {name} did not verify: {} problem(s)\n{text}", r.problems.len()));
    }
    let _ = writeln!(text, "Image catalog {name} verified: {} entries{}.", r.checked, if r.sorted { ", sorted" } else { "" });
    Ok(ok(&text, CLG_VERIFIED, &[(&name, 10)]))
}

/// `WRKIMGCLG`'s text form.
pub(crate) fn wrkimgclg_text(which: &str, ty: &str) -> String {
    let rows = media::with_catalogs(|r| Ok(r.wrkimgclg())).unwrap_or_default();
    let mut s = String::from("Catalog     Status      Type  Text\n");
    for c in rows {
        let pass = (which.is_empty() || which == "*ALL" || c.name == which || which.strip_suffix('*').is_some_and(|p| c.name.starts_with(p)))
            && (ty.is_empty() || ty == "*ALL" || ty == c.kind);
        if pass {
            let _ = writeln!(s, "{:<10}  {:<10}  {:<4}  {}", c.name, c.status, c.kind, c.text);
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
    let _ = writeln!(s, "Index  Status      WrtPtc  Volume                            File");
    for e in &d.entries {
        let _ = writeln!(s, "{:>5}  {:<10}  {:<6}  {:<32}  {}", e.index, e.status, e.write_protect, e.volume, e.file);
    }
    Ok(s)
}

// --------------------------------------------------------------------- tape

/// A `VOL` value, `None` for `*MOUNTED`.
fn vol(parms: &Parms, kwd: &str) -> Option<String> {
    let v = parms.text(kwd).trim().trim_matches('\'').to_ascii_uppercase();
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
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, _: &mut dyn VolumeProvider<D>) -> Result<(), MediaError> {
        Ok(tape::volume::inztap(d, &self.0)?)
    }
}

fn inztap(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
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
        new_owner: if owner.trim() == "*BLANK" { String::new() } else { owner.trim().to_string() },
        vol: v.clone(),
        check: match parms.name("CHECK").as_str() {
            "*NO" => Check::No,
            "*FIRST" => Check::First,
            _ => Check::Yes,
        },
        density: Some(parms.name("DENSITY")).filter(|d| !d.is_empty() && !d.starts_with('*')),
        code: if parms.name("CODE") == "*ASCII" { tape::label::Code::Ascii } else { tape::label::Code::Ebcdic },
        end_opt: stream::end_opt(&parms.name("ENDOPT")),
        clear: parms.yes("CLEAR"),
        today: Some(stream::today()),
    };
    let vl: Vec<String> = v.into_iter().collect();
    drive::with_tape(system, &dev, &vl, true, &mut Inz(opts))?;
    let id = new_vol.unwrap_or_else(|| "*NL".into());
    Ok(ok(&format!("Volume {id} initialized on device {dev}.\n"), VOL_INITIALIZED, &[(&id, 6), (&dev, 10)]))
}

/// DSPTAP's job: the labels, and for `*SAVRST` each save's stream.
struct Dsp {
    vol: Option<String>,
    label: Option<String>,
    range: (u32, u32),
    saves: bool,
    end_opt: tape::EndOpt,
}

type DspOut = (tape::volume::VolumeLabels, Vec<(u32, String, Vec<u8>)>);

impl TapeJob for Dsp {
    type Out = DspOut;
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, p: &mut dyn VolumeProvider<D>) -> Result<DspOut, MediaError> {
        let l = tape::volume::labels(d, self.vol.as_deref(), tape::EndOpt::Rewind)?;
        let mut out = Vec::new();
        if self.saves {
            let wanted: Vec<(u32, String)> = l
                .files
                .iter()
                .filter(|f| f.label.volume_seq <= 1)
                .filter(|f| (self.range.0..=self.range.1).contains(&f.label.file_seq))
                .filter(|f| self.label.as_ref().is_none_or(|x| *x == f.label.file_id))
                .map(|f| (f.label.file_seq, f.label.file_id.clone()))
                .collect();
            for (seq, id) in wanted {
                let opts = ReadOptions {
                    seq: Some(seq),
                    ..ReadOptions::default()
                };
                let mut r = FileReader::open(&mut *d, &mut *p, opts)?;
                let mut buf = vec![0u8; MAX_BLOCK];
                let mut bytes = Vec::new();
                while let Some(n) = r.read_block(&mut buf)? {
                    bytes.extend_from_slice(&buf[..n]);
                }
                r.finish()?;
                out.push((seq, id, bytes));
            }
        }
        self.end_opt.apply(d)?;
        Ok((l, out))
    }
}

fn dsptap(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let dev = parms.name("DEV");
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
    let label = parms.text("LABEL").trim().to_ascii_uppercase();
    let data = parms.name("DATA");
    let mut job = Dsp {
        vol: vol(parms, "VOL"),
        label: (!label.is_empty() && label != "*ALL").then_some(label),
        range: (first, last),
        saves: data == "*SAVRST",
        end_opt: stream::end_opt(&parms.name("ENDOPT")),
    };
    let vl: Vec<String> = job.vol.iter().cloned().collect();
    let (l, saves) = drive::with_tape(system, &dev, &vl, false, &mut job)?;
    let mut s = String::new();
    match &l.kind {
        VolumeKind::Labeled { vol1, code } => {
            let _ = writeln!(s, "Volume identifier . . . . . : {}", vol1.volume_id);
            let _ = writeln!(s, "Owner identifier  . . . . . : {}", vol1.owner);
            let _ = writeln!(s, "Volume type . . . . . . . . : SL");
            let _ = writeln!(s, "Code  . . . . . . . . . . . : {}", if *code == tape::label::Code::Ascii { "*ASCII" } else { "*EBCDIC" });
        }
        k => {
            let _ = writeln!(s, "Volume identifier . . . . . : {}", k.id());
        }
    }
    if data == "*VOLLBL" {
        return Ok(plain(s));
    }
    if job.saves {
        for (seq, id, bytes) in &saves {
            let _ = writeln!(s, "\nFile sequence {seq}, label {id}");
            match super::savcmd::stream_listing(bytes) {
                Ok((desc, rows)) => {
                    for (k, v) in &desc {
                        let _ = writeln!(s, "  {k:<24} {v}");
                    }
                    let _ = writeln!(s, "  {:<10} {:<10} {:<10} {:<10} {:>12}  Text", "Object", "Type", "Library", "Attribute", "Size");
                    for r in &rows {
                        let _ = writeln!(s, "  {:<10} {:<10} {:<10} {:<10} {:>12}  {}", r[0], r[1], r[2], r[3], r[4], r[5]);
                    }
                }
                Err(e) => {
                    let _ = writeln!(s, "  not a save: {e}");
                }
            }
        }
        return Ok(plain(s));
    }
    let _ = writeln!(s, "Seq   Label              VolSeq  Created  Expires  Blocks  End");
    for f in l.files.iter().filter(|f| (first..=last).contains(&f.label.file_seq)) {
        let when = |d: Option<tape::label::Date>| d.map_or_else(|| "*NONE".to_string(), |d| format!("{}.{:03}", d.year, d.day));
        let exp = match f.label.expires {
            tape::label::Expiry::None => "*NONE".to_string(),
            tape::label::Expiry::Perm => "*PERM".to_string(),
            tape::label::Expiry::Date(d) => format!("{}.{:03}", d.year, d.day),
        };
        let end = match f.trailer {
            tape::label::LabelSet::EndOfFile => "EOF",
            tape::label::LabelSet::EndOfVolume => "EOV",
            tape::label::LabelSet::Header => "*NONE",
        };
        let _ = writeln!(
            s,
            "{:>4}  {:<17}  {:>6}  {:<8} {:<8} {:>6}  {end}",
            f.label.file_seq, f.label.file_id, f.label.volume_seq, when(f.label.created), exp, f.label.block_count
        );
    }
    Ok(plain(s))
}

struct Chk(ChkOptions);

impl TapeJob for Chk {
    type Out = (tape::volume::ChkResult, Option<instmedia::layout::Contents>);
    fn run<D: TapeTransport + ?Sized>(&mut self, d: &mut D, _: &mut dyn VolumeProvider<D>) -> Result<Self::Out, MediaError> {
        // Install media or a system save says so; the check follows.
        let media = instmedia::savsys::recognize_tape(d).ok().flatten();
        Ok((tape::volume::chktap(d, &self.0)?, media))
    }
}

fn chktap(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
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
    let (r, media) = drive::with_tape(system, &dev, &vl, false, &mut Chk(opts))?;
    let id = r.kind.id();
    let mut text = format!("Volume {id} on device {dev} checked.\n");
    if let Some(f) = &r.file {
        let _ = writeln!(text, "File sequence {}, label {} found.", f.label.file_seq, f.label.file_id);
    }
    if let Some(c) = &media {
        text.push_str(&install_media_text(c));
    }
    Ok(ok(&text, VOL_CHECKED, &[(&id, 6), (&dev, 10)]))
}

/// What install media holds, as `CHKTAP` and `DSPOPT DATA(*SAVRST)`
/// show it: the kind and release, then the payload files.
pub(crate) fn install_media_text(c: &instmedia::layout::Contents) -> String {
    let mut s = String::new();
    let kind = if c.savsys { "system save (SAVSYS)" } else { "distribution media" };
    let _ = writeln!(s, "Install media . . . . . . . : {kind}, release {}", c.release);
    let _ = writeln!(s, "Volume in the manifest  . . : {}", c.manifest.volume);
    let _ = writeln!(s, "Products  . . . . . . . . . : {}", c.products().join(" "));
    for f in &c.manifest.files {
        let _ = writeln!(s, "  {:<32} {:>12} bytes", f.path, f.size);
    }
    s
}

fn duptap(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let from = parms.name("FROMDEV");
    let to = parms.name("TODEV");
    if from == to {
        return Err("CPD0084 FROMDEV and TODEV must be different devices".into());
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
        (a, b) => DupSeq::Range(u32::try_from(a).unwrap_or(1), u32::try_from(b).unwrap_or(u32::MAX)),
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
    let n = drive::dup_tape(system, &from, &to, &opts)?;
    Ok(ok(&format!("{n} file(s) duplicated from device {from} to device {to}.\n"), VOL_DUPLICATED, &[(&from, 10), (&to, 10)]))
}

// ------------------------------------------------------------------ optical

/// The optical volume a command names: `DEV` and `VOL`, or the volume
/// wherever it is loaded.
fn optical(system: &mut System, parms: &Parms, vol_kwd: &str, dev_kwd: &str) -> Result<(drive::Target, String), String> {
    let v = vol(parms, vol_kwd);
    let dev = parms.name(dev_kwd);
    if !dev.is_empty() && !dev.starts_with('*') {
        return drive::optical(system, &dev, v.as_deref());
    }
    match v {
        Some(v) => drive::optical_by_volume(system, &v),
        None => Err(format!("CPD0084 {dev_kwd} is needed with {vol_kwd}(*MOUNTED)")),
    }
}

/// The catalog entry holding volume `vol` on device `dev`: catalog,
/// index, image path.
fn entry_of(dev: &str, vol: &str) -> Result<(String, u16, String), String> {
    let c = media::catalog_on(dev).ok_or_else(|| format!("{} optical device {dev} is empty", drive::OPT_EMPTY))?;
    let e = c
        .entries
        .iter()
        .find(|e| e.volume.eq_ignore_ascii_case(vol))
        .ok_or_else(|| format!("OPT1115 volume {vol} not found"))?;
    Ok((c.name.clone(), e.index, c.path_of(e)))
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

fn inzopt(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let (t, volid) = optical(system, parms, "VOL", "DEV")?;
    let new = parms.text("NEWVOL").trim().to_ascii_uppercase();
    let new = if new.is_empty() || new == "*VOL" { volid.clone() } else { new };
    let (clg, index, path) = entry_of(&t.dev, &volid)?;
    release(&clg)?;
    let store = imgclg::StdFs::new(crate::stgsvc::ifs::ROOT);
    let img = imgclg::CatalogFs::image(&store, &path).map_err(|e| format!("OPT1605 {path}: {e}"))?;
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
    r.map_err(|e| drive::optical_failed(system, &t, &MediaError::Optical(e)))?;
    Ok(ok(&format!("Volume {new} initialized on device {}.\n", t.dev), VOL_INITIALIZED, &[(&new, 32), (&t.dev, 10)]))
}

fn dspopt(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let (_, volid) = optical(system, parms, "VOL", "DEV")?;
    let set = media::volume_set();
    let data = parms.name("DATA");
    let path = parms.text("PATH");
    let path = if path.trim().is_empty() { "/".to_string() } else { path.trim().trim_matches('\'').to_string() };
    let mut s = String::new();
    let opt = |e: optical::Error| format!("{} {e}", e.msgid());
    match data.as_str() {
        "*DIRATR" => {
            for d in set.with(&volid, |v| optical::volume::ops::dir_attrs(v, &path, true)).map_err(opt)? {
                let _ = writeln!(s, "{:<40} files {:>6}  directories {:>6}  bytes {:>12}", d.path, d.files, d.dirs, d.bytes);
            }
        }
        "*FILATR" => {
            for f in set.with(&volid, |v| optical::volume::ops::file_attrs(v, &path)).map_err(opt)? {
                let _ = writeln!(s, "{:<50} {:>12}", f.path, f.size);
            }
        }
        "*SAVRST" => {
            let mut files: Vec<String> = set
                .with(&volid, |v| optical::volume::ops::file_attrs(v, "/"))
                .map_err(opt)?
                .into_iter()
                .map(|f| f.path)
                .collect();
            if let Ok(sys) = set.with(&volid, |v| optical::volume::ops::file_attrs(v, "/QSAVSYS")) {
                files.extend(sys.into_iter().map(|f| f.path));
            }
            // Install media: what it is, then its product loads too.
            if let Some(c) = set.with(&volid, |v| Ok(instmedia::savsys::recognize_volume(v).ok().flatten())).map_err(opt)? {
                s.push_str(&install_media_text(&c));
                let lp = format!("/{}", instmedia::layout::LP_DIR);
                if let Ok(l) = set.with(&volid, |v| optical::volume::ops::file_attrs(v, &lp)) {
                    files.extend(l.into_iter().map(|f| f.path));
                }
            }
            for f in files {
                let bytes = set.with(&volid, |v| v.read_all(&f)).map_err(opt)?;
                let Ok((desc, rows)) = super::savcmd::stream_listing(&bytes) else { continue };
                let _ = writeln!(s, "Optical file {f}");
                for (k, v) in &desc {
                    let _ = writeln!(s, "  {k:<24} {v}");
                }
                let _ = writeln!(s, "  {:<10} {:<10} {:<10} {:<10} {:>12}  Text", "Object", "Type", "Library", "Attribute", "Size");
                for r in &rows {
                    let _ = writeln!(s, "  {:<10} {:<10} {:<10} {:<10} {:>12}  {}", r[0], r[1], r[2], r[3], r[4], r[5]);
                }
            }
        }
        _ => {
            let a = set.with(&volid, |v| optical::volume::ops::volume_attrs(v)).map_err(opt)?;
            let yn = |b: bool| if b { "*YES" } else { "*NO" };
            let rows = [
                ("Volume identifier", a.volume_id.clone()),
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
                let _ = writeln!(s, "{k:.<30} : {v}");
            }
        }
    }
    Ok(plain(s))
}

fn chkopt(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let (t, volid) = optical(system, parms, "VOL", "DEV")?;
    let r = media::volume_set()
        .with(&volid, |v| Ok(optical::volume::ops::chkopt(v)))
        .map_err(|e| format!("{} {e}", e.msgid()))?;
    let mut text = String::new();
    for (p, why) in &r.errors {
        let _ = writeln!(text, "{p}: {why}");
    }
    if !r.errors.is_empty() {
        let e = MediaError::Optical(optical::Error::Malformed(format!("{} file(s) do not read", r.errors.len())));
        return Err(format!("{}\n{text}", drive::optical_failed(system, &t, &e)));
    }
    let _ = writeln!(text, "Volume {volid} checked: {} files, {} bytes.", r.files, r.bytes);
    Ok(ok(&text, VOL_CHECKED, &[(&volid, 32), (&t.dev, 10)]))
}

fn dupopt(system: &mut System, parms: &Parms) -> Result<CmdReply, String> {
    let (_, from) = optical(system, parms, "FROMVOL", "FROMDEV")?;
    let (t, to) = optical(system, parms, "TOVOL", "TODEV")?;
    if from.eq_ignore_ascii_case(&to) {
        return Err("OPT1203 a volume cannot be copied onto itself".into());
    }
    let new = parms.text("NEWVOL").trim().to_ascii_uppercase();
    let new_volume = match new.as_str() {
        "" | "*TOVOL" => Some(to.clone()),
        "*FROMVOL" => None,
        n => Some(n.to_string()),
    };
    let (clg, index, path) = entry_of(&t.dev, &to)?;
    let set = media::volume_set();
    set.sync(&from).map_err(|e| format!("{} {e}", e.msgid()))?;
    release(&clg)?;
    let store = imgclg::StdFs::new(crate::stgsvc::ifs::ROOT);
    let img = imgclg::CatalogFs::image(&store, &path).map_err(|e| format!("OPT1605 {path}: {e}"))?;
    let p = optical::volume::ops::DupOpt {
        new_volume,
        text: None,
        capacity: None,
        epoch: now_unix(),
    };
    let r = set.with(&from, |v| optical::volume::ops::dupopt(v, img, &p).map(|_| ()));
    reenter(&clg, index, &path, &t.dev)?;
    r.map_err(|e| drive::optical_failed(system, &t, &MediaError::Optical(e)))?;
    Ok(ok(&format!("Volume {from} duplicated onto device {}.\n", t.dev), VOL_DUPLICATED, &[(&from, 32), (&t.dev, 10)]))
}

fn wrkoptvol(system: &mut System) -> String {
    let mut s = String::from("Volume                            Device      Catalog     Protected\n");
    let devs = devices::all(system).unwrap_or_default();
    for (n, d) in devs.iter().filter(|(_, d)| d.media_kind() == Some(MediaKind::Optical)) {
        let _ = d;
        if let Some(c) = media::catalog_on(n)
            && let Some(e) = c.mounted()
        {
            let _ = writeln!(s, "{:<32}  {:<10}  {:<10}  {}", e.volume, n, c.name, if e.write_protect { "*YES" } else { "*NO" });
        }
    }
    s
}

// ------------------------------------------------------------ media library

/// Cartridge added or changed, ejected.
const CTG_CATEGORY: &str = "CPC6776";
const CTG_EJECTED: &str = "CPC6777";

/// `WRKMLBSTS`'s text form: each library, its status, and for a virtual
/// one its drive resources with their allocation and cartridge.
pub(crate) fn wrkmlbsts_text(system: &mut System, which: &str) -> String {
    let mut s = String::from("Device/Resource   Status        Allocation     Cartridge\n");
    let devs = devices::all(system).unwrap_or_default();
    for (n, d) in devs.iter().filter(|(_, d)| d.media_kind() == Some(MediaKind::TapeLibrary)) {
        if !(which.is_empty() || which == "*ALL" || *n == which || which.strip_suffix('*').is_some_and(|p| n.starts_with(p))) {
            continue;
        }
        let _ = writeln!(s, "{n:<16}  {:<12}", d.status.text());
        let Some(m) = &d.media else { continue };
        match media::library::drives(system, n) {
            Ok(rows) => {
                for r in rows {
                    let _ = writeln!(s, "  {:<14}  {:<12}  {:<13}  {}", r.name, "OPERATIONAL", r.allocation, r.cartridge);
                }
            }
            Err(_) => {
                let _ = writeln!(s, "  {:<14}  {:<12}  *ALLOCATED", m.rsrcname, d.status.text());
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
pub(crate) fn tapctg_rows(system: &mut System, dev: &str, ids: &[String], cgy: &str) -> Result<Vec<media::library::CtgRow>, String> {
    media::library::rows(system, dev, ids, cgy)
}

fn mlb_ctg(system: &mut System, name: &str, parms: &Parms) -> Result<CmdReply, String> {
    let dev = parms.name("DEV");
    let d = devices::read(system, &dev)?;
    if d.media_kind() != Some(MediaKind::TapeLibrary) {
        return Err(format!("CPF6745 device {dev} is not a tape media library"));
    }
    let ids = ctgs(parms);
    let cgy = cgy(parms);
    match name {
        "WRKTAPCTG" | "DSPTAPCTG" => {
            let rows = tapctg_rows(system, &dev, &ids, &cgy)?;
            let mut s = format!("Library device  . . . : {dev}\n");
            if name == "DSPTAPCTG" {
                for r in &rows {
                    let _ = writeln!(s, "\nCartridge ID  . . . . : {}", r.id);
                    let _ = writeln!(s, "Volume ID . . . . . . : {}", r.volume);
                    let _ = writeln!(s, "Category  . . . . . . : {}", r.category);
                    let _ = writeln!(s, "Status  . . . . . . . : {}", r.status);
                    let _ = writeln!(s, "Location  . . . . . . : {}", r.location);
                    let _ = writeln!(s, "Home slot . . . . . . : {}", r.home);
                }
                return Ok(plain(s));
            }
            let _ = writeln!(s, "Cartridge  Volume  Category    Status      Location");
            for r in &rows {
                let _ = writeln!(s, "{:<9}  {:<6}  {:<10}  {:<10}  {}", r.id, r.volume, r.category, r.status, r.location);
            }
            Ok(plain(s))
        }
        "ADDTAPCTG" | "CHGTAPCTG" => {
            let to = if cgy.is_empty() || cgy == "*SAME" { None } else { Category::parse(&cgy) };
            if name == "ADDTAPCTG" && to.is_none() {
                return Err(format!("CPD0084 CGY({cgy}) is not a category"));
            }
            let check = name == "ADDTAPCTG" && parms.name("CHECK") != "*NO";
            let all_inserted = ids.iter().any(|i| i == "*INSERT");
            let ids = if all_inserted {
                tapctg_rows(system, &dev, &[], "*INSERT")?.into_iter().map(|r| r.id).collect()
            } else {
                ids
            };
            if check {
                for id in &ids {
                    if let Some(v) = media::library::label_of(system, &dev, id)?
                        && v != *id
                        && !matches!(v.as_str(), "*NL" | "*BLANK" | "*UNKNOWN")
                    {
                        return Err(format!("CPF6720 cartridge {id} on {dev} carries volume {v}"));
                    }
                }
            }
            let Some(to) = to else {
                return Ok(plain(format!("No cartridge of {dev} was changed.\n")));
            };
            media::library::with_library(system, &dev, |_, _, lib| {
                for id in &ids {
                    let r = if name == "ADDTAPCTG" { lib.add(id, to.clone()) } else { lib.change(id, to.clone()) };
                    r.map_err(|e| media::library::refused(&e))?;
                }
                Ok(())
            })?;
            let mut text = String::new();
            for id in &ids {
                let _ = writeln!(text, "Cartridge {id} is now in category {} of {dev}.", to.name());
            }
            let first = ids.first().cloned().unwrap_or_default();
            Ok(ok(&text, CTG_CATEGORY, &[(&first, 6), (to.name(), 10), (&dev, 10)]))
        }
        "RMVTAPCTG" => {
            let moved = media::library::with_library(system, &dev, |_, _, lib| {
                let mut out = Vec::new();
                for id in &ids {
                    let m = lib.remove(id).map_err(|e| media::library::refused(&e))?;
                    out.push((id.clone(), !m.is_empty()));
                }
                Ok(out)
            })?;
            let mut text = String::new();
            for (id, moved) in &moved {
                let _ = writeln!(
                    text,
                    "Cartridge {id} is ejected from {dev}{}.",
                    if *moved { " to an import/export station" } else { "; it stays in its slot" }
                );
            }
            let first = ids.first().cloned().unwrap_or_default();
            Ok(ok(&text, CTG_EJECTED, &[(&first, 6), (&dev, 10)]))
        }
        _ => Err(format!("CPF6745 {name} is not a cartridge command")),
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

/// `WRKIMGCLG`'s panel data: one row per catalog `which` and `ty`
/// select, with its device and the device's status.
pub(crate) fn imgclg_panel(system: &mut System, which: &str, ty: &str) -> stg::proto::PanelData {
    let rows = media::with_catalogs(|r| Ok(r.wrkimgclg())).unwrap_or_default();
    let mut out = Vec::new();
    for c in rows {
        let pass = (which.is_empty() || which == "*ALL" || c.name == which || which.strip_suffix('*').is_some_and(|p| c.name.starts_with(p)))
            && (ty.is_empty() || ty == "*ALL" || ty == c.kind);
        if !pass {
            continue;
        }
        let dev = media::with_catalogs(|r| r.wrkimgclge(&c.name)).map(|d| d.device).unwrap_or_default();
        let dev = if dev == "*NONE" { String::new() } else { dev };
        let sts = if dev.is_empty() {
            String::new()
        } else {
            devices::read(system, &dev).map(|d| d.status.text().to_string()).unwrap_or_default()
        };
        out.push(vec![
            var("CLG", &c.name),
            var("CLGSTS", ready(c.status)),
            var("CLGTYPE", c.kind),
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
        var("CLGTYPE", d.kind),
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
pub(crate) fn tapctg_panel(system: &mut System, dev: &str, ids: &[String], cgy: &str) -> Result<stg::proto::PanelData, String> {
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
    for (n, d) in devs.iter().filter(|(_, d)| d.media_kind() == Some(MediaKind::TapeLibrary)) {
        if !(which.is_empty() || which == "*ALL" || *n == which || which.strip_suffix('*').is_some_and(|p| n.starts_with(p))) {
            continue;
        }
        out.push(vec![var("RSRC", n), var("RSRCSTS", d.status.text()), var("RSRCLIB", n)]);
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
/// `mlb-demount DEV CTG`, `mlb-alloc DEV DRIVE ALLOCATION`. A line for
/// the message line.
pub(crate) fn mlb_op(system: &mut System, op: &str, args: &[String]) -> Result<Vec<String>, String> {
    let arg = |i: usize| args.get(i).map_or("", |s| s.trim()).to_ascii_uppercase();
    let (dev, what) = (arg(0), arg(1));
    media::library::with_library(system, &dev, |_, _, lib| {
        let line = match op {
            "mlb-mount" => {
                let i = lib.mount(&what).map_err(|e| media::library::refused(&e))?;
                format!("Cartridge {what} mounted in {}.", lib.model.drives[i].name)
            }
            "mlb-demount" => {
                let i = lib
                    .model
                    .drives
                    .iter()
                    .position(|d| d.cartridge.as_deref() == Some(what.as_str()))
                    .ok_or_else(|| format!("CPF6747 cartridge {what} is not mounted"))?;
                lib.demount(i).map_err(|e| media::library::refused(&e))?;
                format!("Cartridge {what} demounted.")
            }
            "mlb-alloc" => {
                let a = match arg(2).as_str() {
                    "*UNPROTECTED" => tape::device::Allocation::Unprotected,
                    "*DEALLOCATED" => tape::device::Allocation::Deallocated,
                    _ => tape::device::Allocation::Allocated,
                };
                lib.allocate(&what, a).map_err(|e| media::library::refused(&e))?;
                format!("Resource {what} is {}.", a.name())
            }
            _ => return Err(format!("CPF9899 no library action {op}")),
        };
        Ok(vec![line])
    })
}
