// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel data of the media displays (request 270, `cppdata.rs`):
//! `DSPTAP`, `DSPTAPCTG`, `DSPOPT` and `WRKOPTVOL`, read by
//! `crate::cpp::meddsp` for the job's drivers in
//! `work/qsjob/src/panels/info/media.rs`.

use stg::proto::Sys2Data;

use crate::cpp::meddsp::{self, OptView, TapAsk};
use crate::ipl::system::System;

type Rows = Vec<Vec<(String, String)>>;

fn pairs(p: &[(&str, String)]) -> Vec<(String, String)> {
    p.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

fn arg(args: &[String], i: usize) -> String {
    args.get(i).cloned().unwrap_or_default()
}

/// The six columns of a saved object, after `lead`.
fn object(lead: (&str, String), r: &[String]) -> Vec<(String, String)> {
    let c = |i: usize| r.get(i).cloned().unwrap_or_default();
    pairs(&[lead, ("OBJ", c(0)), ("OTYPE", c(1)), ("OLIB", c(2)), ("OATTR", c(3)), ("OSIZE", c(4)), ("OTEXT", c(5))])
}

/// `DSPTAP DEV VOL LABEL first last DATA ENDOPT`.
pub(super) fn tape(system: &mut System, args: &[String]) -> Result<Sys2Data, String> {
    let n = |i: usize, d: u32| arg(args, i).trim().parse().unwrap_or(d);
    let (dev, vol, label, data, endopt) = (arg(args, 0), arg(args, 1), arg(args, 2), arg(args, 5), arg(args, 6));
    let v = meddsp::tape_view(
        system,
        &TapAsk {
            dev: &dev,
            vol: &vol,
            label: &label,
            range: (n(3, 1), n(4, 16_777_215)),
            data: &data,
            end_opt: &endopt,
        },
    )?;
    let vars = pairs(&[
        ("DEV", v.dev.clone()),
        ("VOLID", v.vol.clone()),
        ("OWNER", v.owner.clone()),
        ("VOLTYPE", v.vol_type.to_string()),
        ("CODE", v.code.to_string()),
    ]);
    let files: Rows = v
        .files
        .iter()
        .map(|f| {
            pairs(&[
                ("FSEQ", f.seq.to_string()),
                ("FLABEL", f.label.clone()),
                ("FVSEQ", f.volseq.to_string()),
                ("FCRT", f.created.clone()),
                ("FEXP", f.expires.clone()),
                ("FBLK", f.blocks.to_string()),
                ("FEND", f.end.to_string()),
            ])
        })
        .collect();
    let objs: Rows = v.objects.iter().map(|o| object(("OSEQ", o.seq.to_string()), &o.row)).collect();
    Ok((vars, vec![("FILES".into(), files), ("OBJS".into(), objs)]))
}

/// `DSPTAPCTG DEV CTG(comma-separated) CGY`: one row per cartridge.
pub(super) fn cartridges(system: &mut System, args: &[String]) -> Result<Sys2Data, String> {
    let ids: Vec<String> = arg(args, 1)
        .split(',')
        .map(|s| s.trim().to_ascii_uppercase())
        .filter(|s| !s.is_empty())
        .collect();
    let sys = system.system_name();
    let rows: Rows = meddsp::cartridges(system, &arg(args, 0), &ids, &arg(args, 2))?
        .into_iter()
        .map(|c| {
            let r = c.row;
            pairs(&[
                ("MLBDEV", c.dev),
                ("CTG", r.id),
                ("CTGVOL", r.volume),
                ("CTGCGY", r.category),
                ("CTGSYS", sys.clone()),
                ("CTGSTS", r.status),
                ("CTGLOC", r.location),
                ("CTGHOME", r.home),
            ])
        })
        .collect();
    Ok((Vec::new(), vec![("CTGS".into(), rows)]))
}

/// `DSPOPT VOL DEV DATA PATH`: `VIEW` says which panel.
pub(super) fn optical(system: &mut System, args: &[String]) -> Result<Sys2Data, String> {
    let v = meddsp::optical_view(system, &arg(args, 0), &arg(args, 1), &arg(args, 2), &arg(args, 3))?;
    Ok(match v {
        OptView::Volume(o) => {
            let a = &o.attrs;
            let yn = |b: bool| if b { "Yes" } else { "No" }.to_string();
            let when = |t: Option<&optical::iso::Timestamp>| {
                t.map(|t| format!("{}  {}", meddsp::opt_mdy(t), meddsp::opt_hms(t))).unwrap_or_default()
            };
            let vars = pairs(&[
                ("VIEW", "VOLATR".into()),
                ("VOLID", a.volume_id.clone()),
                ("DEV", o.dev.clone()),
                ("VOLTYPE", "*PRIMARY".into()),
                ("MEDTYPE", if a.writable_layout { "*DVD-RAM" } else { "*CD-ROM" }.into()),
                ("MEDFMT", a.media_format.to_string()),
                ("VOLTEXT", a.text.clone()),
                ("WRTPTC", yn(a.write_protected)),
                ("CRTDTS", when(a.created.as_ref())),
                ("CHGDTS", when(a.changed.as_ref())),
                ("VOLCAP", a.capacity.to_string()),
                ("VOLUSED", a.used.to_string()),
                ("VOLFREE", a.free.to_string()),
                ("BLKSIZ", a.block_size.to_string()),
                ("VOLSET", a.volume_set.clone()),
                ("SETSIZE", a.set_size.to_string()),
                ("SETSEQ", a.sequence.to_string()),
            ]);
            (vars, Vec::new())
        }
        OptView::Dirs(dev, vol, dirs) => {
            let rows: Rows = dirs
                .iter()
                .map(|d| {
                    pairs(&[
                        ("DPATH", d.path.clone()),
                        ("DFILES", d.files.to_string()),
                        ("DDIRS", d.dirs.to_string()),
                        ("DBYTES", d.bytes.to_string()),
                    ])
                })
                .collect();
            (pairs(&[("VIEW", "DIRATR".into()), ("DEV", dev), ("VOLID", vol)]), vec![("DIRS".into(), rows)])
        }
        OptView::Files(dev, vol, files) => {
            let rows: Rows = files
                .iter()
                .map(|f| pairs(&[("FPATH", f.path.clone()), ("FSIZE", f.size.to_string())]))
                .collect();
            (pairs(&[("VIEW", "FILATR".into()), ("DEV", dev), ("VOLID", vol)]), vec![("OFILES".into(), rows)])
        }
        OptView::Saves(dev, vol, saves) => {
            let rows: Rows = saves
                .iter()
                .flat_map(|(f, (_, rows))| rows.iter().map(move |r| object(("OFILE", f.clone()), r)))
                .collect();
            (pairs(&[("VIEW", "SAVRST".into()), ("DEV", dev), ("VOLID", vol)]), vec![("OSAVS".into(), rows)])
        }
    })
}

/// `WRKOPTVOL DEV VOL CSI`.
pub(super) fn volumes(system: &mut System, args: &[String]) -> Result<Sys2Data, String> {
    let rows: Rows = meddsp::optical_volumes(system, &arg(args, 0), &arg(args, 1))?
        .into_iter()
        .map(|r| {
            pairs(&[
                ("OPT", String::new()),
                ("VOL", r.vol),
                ("DEV", r.dev),
                ("VOLTYPE", r.vol_type.into()),
                ("MEDTYPE", r.media_type.into()),
                ("AUTL", r.autl.into()),
                ("CLG", r.catalog),
            ])
        })
        .collect();
    let all = |s: String| if s.trim().is_empty() { "*ALL".to_string() } else { s };
    let vars = pairs(&[("WDEV", all(arg(args, 0))), ("CSI", all(arg(args, 2)))]);
    Ok((vars, vec![("VOLS".into(), rows)]))
}
