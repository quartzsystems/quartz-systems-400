// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The WRKRDBDIRE displays against the pub400 goldens, row by row
//! (rows 1 to 23; row 24 is the vendor's copyright line there), filled
//! with the values the goldens show.

use std::path::PathBuf;

use ddm::objects::panel::{display_entry, wrkrdbdire};
use ddm::objects::{RdbDirectory, RdbEntry};

fn golden(name: &str) -> Vec<String> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase10/misc")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap()
        .replace("\r\n", "\n")
        .lines()
        .map(|l| format!("{l:<80}"))
        .collect()
}

fn compare(name: &str, ours: &[String]) {
    let g = golden(name);
    for (i, (a, b)) in g.iter().zip(ours).take(23).enumerate() {
        assert_eq!(b.trim_end(), a.trim_end(), "{name} row {}", i + 1);
    }
}

fn pub400_directory(local_text: &str) -> RdbDirectory {
    let mut d = RdbDirectory::installed("PUB400");
    d.change("PUB400", |e| e.text = local_text.into()).unwrap();
    d.add(RdbEntry::new("RZKHCOCKPIT", "cockpit.rzkh.de", "*IP")).unwrap();
    d
}

#[test]
fn wrkrdbdire_list() {
    // The list golden shows the text's last character as `|`.
    let d = pub400_directory("please do not change|");
    compare("wrkrdbdire", &wrkrdbdire(&d.select("*ALL"), false));
}

#[test]
fn wrkrdbdire_option_5() {
    let d = pub400_directory("please do not change!");
    compare("wrkrdbdire-5", &display_entry(d.find("PUB400").unwrap()));
}
