// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shared test fixtures.

#![allow(dead_code)]

use std::fmt::Write as _;

use licpgm::{MemStore, SystemInfo, catalog};

/// The test system: serial `QS40001`, processor group `P05`.
pub fn sys() -> SystemInfo {
    SystemInfo::current("QS40001", "P05")
}

/// A store with the shipped catalog installed.
pub fn seeded() -> MemStore {
    let mut s = MemStore::with_libs(&["QSYS", "QSYS2", "QUSRSYS", "QGPL"]);
    catalog::seed(&mut s, 1_800_000_000).unwrap();
    s
}

/// An EBCDIC field of `n` bytes.
pub fn e(s: &str, n: usize) -> Vec<u8> {
    licpgm::api::fixed(s, n)
}

/// A big-endian `i32`.
pub fn b4(v: i32) -> [u8; 4] {
    v.to_be_bytes()
}

/// `PRDI0100`.
pub fn prdi(product: &str, rel: &str, opt: &str, load: &str) -> Vec<u8> {
    let mut b = e(product, 7);
    b.extend(e(rel, 6));
    b.extend(e(opt, 4));
    b.extend(e(load, 10));
    b
}

/// Builds an expected receiver at fixed offsets.
#[derive(Default)]
pub struct Want(pub Vec<u8>);

impl Want {
    pub fn at(&mut self, off: usize, b: &[u8]) -> &mut Self {
        if self.0.len() < off + b.len() {
            self.0.resize(off + b.len(), 0);
        }
        self.0[off..off + b.len()].copy_from_slice(b);
        self
    }

    pub fn ch(&mut self, off: usize, n: usize, s: &str) -> &mut Self {
        self.at(off, &e(s, n))
    }

    pub fn bin(&mut self, off: usize, v: i32) -> &mut Self {
        self.at(off, &b4(v))
    }

    pub fn len(&mut self, n: usize) -> &mut Self {
        self.0.resize(n, 0);
        self
    }
}

/// A small vendor product manifest.
pub const VENDOR: &str = r#"
[product]
id = "1AB1234"
text = "Inventory control"
library = "INVPRD"
first_copyright = 2024
current_copyright = 2026
release_date = "2026-10-30"
registration_type = "*PHONE"
registration_value = "15551234"
license_key = "{KEY}"

[[option]]
option = "*BASE"
text = "Inventory control base"
msgid = "INV0001"
requires = ["QS00SS1:*BASE"]
libraries = [{ name = "INVPRD", post_exit = "INVPOST" }, { name = "INVDTA" }]
pre_exits = ["INVPRE"]
objects = [
  { name = "INVMAIN", type = "*PGM" },
  { name = "INVFILE", type = "*FILE", lib = "INVDTA" },
]
lng_objects = [{ name = "INVMSG", type = "*MSGF" }]
directories = [{ path = "/QIBM/ProdData/INV", obj_auts = ["*OBJEXIST"] }]
paths = ["/QIBM/ProdData/INV/bin"]

[[option]]
option = 1
text = "Inventory reports"
msgid = "INV0002"
feature = "5101"
requires = ["1AB1234:*BASE"]
libraries = [{ name = "INVRPT" }]
objects = [{ name = "RPT01", type = "*PGM" }]

[[license]]
feature = "5001"
compliance = "*KEYED"
usage_limit = 0
grace_days = 30
threshold = 2

[[license]]
feature = "5101"
compliance = "*OPRACTION"
usage_limit = 2

[agreement]
text = "Use it well."
"#;

/// The vendor's signing key.
pub fn vendor_signer() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[9; 32])
}

/// The vendor manifest with its public key filled in.
pub fn vendor_manifest() -> String {
    let pk = vendor_signer().verifying_key().to_bytes();
    let mut hex = String::new();
    for b in pk {
        let _ = write!(hex, "{b:02x}");
    }
    VENDOR.replace("{KEY}", &hex)
}

/// The vendor package.
pub fn vendor() -> licpgm::pkg::Package {
    licpgm::pkg::parse(&vendor_manifest(), licpgm::pkg::Options::default()).unwrap()
}
