// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS commands the service runs (docs/NFS.md section 8):
//! `EXPORTFS` and `CHGNFSEXP` over the export table
//! (`stgsvc::nfs::export`), `RLSIFSLCK` (`stgsvc::nfs::locks`), and
//! `DSPMFSINF`/`STATFS`'s text and `OUTPUT(*PRINT)` listing
//! (`ifscmd::mfsinf`). Exports and lock releases need `*IOSYSCFG`
//! (`CPFA1B8`).

use cmd::block::Parms;
use ifscmd::{Msg, Reply};
use sec::profile::SpecialAuth;
use stg::proto::CmdReply;

use super::stmfcmd::reply;
use crate::ipl::system::{System, now_unix};
use crate::stgsvc::nfs::{export, locks};

/// Run `name`, or `None` when it is not one of these.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    let r = match name {
        "EXPORTFS" | "CHGNFSEXP" => {
            if let Some(r) = need_iosyscfg(name) {
                return Some(reply(r, ""));
            }
            let a = export::Args {
                options: parms.text("OPTIONS"),
                dir: parms.text("DIR"),
                hosts: export::host_opts(parms),
            };
            export::run(&mut export::Service { system }, &a)
        }
        "RLSIFSLCK" => {
            if let Some(r) = need_iosyscfg(name) {
                return Some(reply(r, ""));
            }
            let yes = |k: &str| matches!(parms.name(k).as_str(), "Y" | "*YES");
            let a = locks::Args {
                remote: parms.text("RMTLOCNAME"),
                obj: parms.text("OBJ"),
                nfs: yes("NFSSVRLCK"),
                local: yes("LCLBRNGLCK"),
                whole_fs: yes("OBJMFS"),
            };
            locks::run(system, &a)
        }
        "DSPMFSINF" | "STATFS" => dspmfsinf(system, parms),
        _ => return None,
    };
    Some(reply(r, ""))
}

/// `CPFA1B8` unless the job's user has `*IOSYSCFG`.
fn need_iosyscfg(cmd: &str) -> Option<Reply> {
    let p = crate::stgsvc::security::actor()?;
    (!p.has(SpecialAuth::IOSYSCFG)).then(|| Reply::fail(Msg::new("CPFA1B8").chars(cmd, 20)))
}

/// `DSPMFSINF` for a job without a display, or `OUTPUT(*PRINT)`.
fn dspmfsinf(system: &mut System, parms: &Parms) -> Reply {
    let obj = parms.text("OBJ");
    let view = super::stmf::with_job(system, |c| ifscmd::mfsinf::view(c, &obj));
    let v = match view {
        Ok(v) => v,
        Err(e) => return Reply::errno(e, &obj),
    };
    if !matches!(parms.name("OUTPUT").as_str(), "P" | "*PRINT") {
        return Reply::text(ifscmd::mfsinf::text(&v));
    }
    let lines = listing(system, &v);
    match crate::svc::security::print_as(system, "QP0LCDMN", "DSPMFSINF", &lines, true) {
        Ok(t) => Reply::text(t),
        Err(e) => Reply::fail(Msg::new("CPF9898").vary(&e)),
    }
}

/// The listing as IBM's lays it out (`oracle/nfs/print/`): the title
/// and page number, the product and release, the system and when, the
/// rows, the end mark. The first character of each line is its forms
/// control.
fn listing(system: &mut System, v: &ifscmd::mfsinf::View) -> Vec<String> {
    let (date, time) = wm::joblog::stamp_of(now_unix().saturating_mul(1_000_000));
    let title = crate::stgsvc::msgs::text(system, "TXT0E31", &[]);
    let page = crate::stgsvc::msgs::text(system, "TXT0E32", &[]);
    let mut l1 = format!("1{:46}{title}", "");
    l1 = format!("{l1:<121}{page} {:>5}", 1);
    let product = format!(" {} {}", release::PRODUCT_ID, release::RELEASE);
    let l2 = format!("{product:<99}{:<8}  {date}  {time}", system.system_name());
    let mut out = vec![l1, l2];
    for (i, row) in ifscmd::mfsinf::print_lines(v).into_iter().enumerate() {
        // The first row after two blank lines (0), the rest single.
        let ctl = if i == 0 { '0' } else { ' ' };
        out.push(format!("{ctl}{row}"));
    }
    out.push(format!(
        "-{:40}* * * * *   E N D   O F   L I S T I N G   * * * * *",
        ""
    ));
    out
}

#[cfg(test)]
mod tests {
    /// The listing's lines line up with the primary's.
    #[test]
    fn the_listing_lines_up_with_the_primarys() {
        let want = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tools/conformance/oracle/nfs/print/dspmfsinf-root-print.txt"),
        )
        .unwrap()
        .replace("\r\n", "\n");
        let want: Vec<&str> = want.lines().collect();
        let v = ifscmd::mfsinf::View {
            object: "/".into(),
            fs_type: "\"root\" (/)".into(),
            rows: Vec::new(),
        };
        let rows = ifscmd::mfsinf::print_lines(&v);
        assert_eq!(format!("0{}", rows[0]), want[2]);
        assert_eq!(format!(" {}", rows[1]), want[3]);
        let title = format!("1{:46}Display Mount Statistics", "");
        assert!(want[0].starts_with(&title));
        assert!(want[0].ends_with("Page     1"));
        assert_eq!(want[0].len(), format!("{title:<121}Page     1").len());
        assert_eq!(
            want.last().unwrap().trim_end(),
            format!(
                "-{:40}* * * * *   E N D   O F   L I S T I N G   * * * * *",
                ""
            )
        );
    }
}
