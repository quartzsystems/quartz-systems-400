// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped profiles against `DSPUSRPRF OUTPUT(*OUTFILE)` of an IBM i
//! 7.1 (`tools/conformance/oracle/sec/expected/usrprf.csv`, the records
//! cut at 1000 bytes by the export): every attribute the capture holds,
//! field by field. Skipped while the capture is absent.

use std::path::PathBuf;

use sec::profile::shipped::{PROFILES, profile_of};

fn capture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tools")
        .join("conformance")
        .join("oracle")
        .join("sec")
        .join("expected")
        .join("usrprf.csv")
}

/// The fields of one record: quoted text (`""` an embedded quote) or a
/// bare number, comma-separated, each trimmed.
fn fields(line: &str) -> Vec<String> {
    let b: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '"' {
            let mut s = String::new();
            i += 1;
            while i < b.len() {
                if b[i] == '"' {
                    if b.get(i + 1) == Some(&'"') {
                        s.push('"');
                        i += 2;
                        continue;
                    }
                    break;
                }
                s.push(b[i]);
                i += 1;
            }
            out.push(s.trim().to_string());
            i += 1;
            if i < b.len() && b[i] == ',' {
                i += 1;
            }
        } else {
            let start = i;
            while i < b.len() && b[i] != ',' {
                i += 1;
            }
            out.push(b[start..i].iter().collect::<String>().trim().to_string());
            i += 1;
        }
    }
    out
}

#[test]
fn shipped_profiles_match_the_outfile() {
    let path = capture();
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("skipped: {} not captured yet", path.display());
        return;
    };
    let mut lines = text.lines();
    let header = fields(lines.next().unwrap_or(""));
    let col = |name: &str| header.iter().position(|h| h == name);
    let mut differ = Vec::new();
    let mut unknown = Vec::new();
    for line in lines {
        let f = fields(line);
        // The export cut every record at 1000 bytes: the last field may
        // be partial, the rest absent.
        let complete = f.len().saturating_sub(1);
        let get = |name: &str| -> Option<&str> {
            col(name)
                .filter(|&i| i < complete)
                .map(|i| f[i].as_str())
        };
        let Some(name) = get("UPUPRF") else { continue };
        let Some(s) = PROFILES.iter().find(|s| s.name == name) else {
            if name.starts_with('Q') {
                unknown.push(name.to_string());
            }
            continue;
        };
        let p = profile_of(s, 40, 3, 1_700_000_000);
        let mut spcaut: Vec<&str> = p.spcaut.names();
        spcaut.retain(|a| *a != "*NONE");
        spcaut.sort_unstable();
        let mut theirs_spcaut: Vec<&str> = get("UPSPAU")
            .unwrap_or("")
            .split_whitespace()
            .filter(|a| *a != "*NONE")
            .collect();
        theirs_spcaut.sort_unstable();
        if get("UPSPAU").is_some() && spcaut != theirs_spcaut {
            differ.push(format!(
                "{name} UPSPAU: ours `{}`, machine `{}`",
                spcaut.join(" "),
                theirs_spcaut.join(" ")
            ));
        }
        let mut check = |field: &str, ours: String| {
            if let Some(theirs) = get(field)
                && theirs != ours
            {
                differ.push(format!("{name} {field}: ours `{ours}`, machine `{theirs}`"));
            }
        };
        check("UPUSCL", p.usrcls.name().into());
        check("UPPRLT", p.ptylmt.to_string());
        check("UPMXST", p.maxstg.to_string());
        check("UPINPG", p.inlpgm.0.clone());
        check("UPINPL", p.inlpgm.1.clone());
        check("UPJBDS", p.jobd.0.clone());
        check("UPJBDL", p.jobd.1.clone());
        check("UPOWNR", (if p.owner_grpprf { "*GRPPRF" } else { "*USRPRF" }).into());
        check("UPGRPF", p.grpprf.clone());
        check("UPGRAU", p.grpaut.clone());
        check("UPACCD", p.acgcde.clone());
        check("UPMGQU", p.msgq.0.clone());
        check("UPMGQL", p.msgq.1.clone());
        check("UPOTQU", p.outq.0.clone());
        check("UPOTQL", p.outq.1.clone());
        check("UPSPEN", p.spcenv.clone());
        check("UPCRLB", p.curlib.clone());
        check("UPINMN", p.inlmnu.0.clone());
        check("UPINML", p.inlmnu.1.clone());
        check("UPLTCP", p.lmtcpb.name().into());
        check("UPDLVY", p.dlvry.clone());
        check("UPSVRT", p.sev.to_string());
        check("UPPRDV", p.prtdev.clone());
        check("UPATPG", p.atnpgm.0.clone());
        check("UPATPL", p.atnpgm.1.clone());
        check("UPSTAT", (if p.disabled { "*DISABLED" } else { "*ENABLED" }).into());
        check("UPKBDB", p.kbdbuf.clone());
        check("UPASTL", p.astlvl.clone());
        check("UPLANG", p.langid.clone());
        check("UPPWON", (if p.pwd_none { "*YES" } else { "*NO" }).into());
        check("UPDSIN", p.dspsgninf.name().into());
        check("UPLDVS", p.lmtdevssn.name().into());
        check(
            "UPPWEI",
            match p.pwdexpitv {
                sec::profile::payload::SYSVAL => "0".into(),
                sec::profile::payload::NOMAX => "-1".into(),
                n => n.to_string(),
            },
        );
    }
    assert!(
        differ.is_empty(),
        "shipped profiles the capture settles differently:\n{}",
        differ.join("\n")
    );
    assert!(
        unknown.is_empty(),
        "profiles the machine ships and this system does not: {}",
        unknown.join(" ")
    );
}
