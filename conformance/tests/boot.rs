// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Boot test. Runs only when `QS400_QEMU=1`; otherwise it skips, so a plain
//! `cargo test` stays green on hosts without QEMU or an image.

use conformance::BootTest;

#[test]
fn boots_through_ipl_complete() {
    if std::env::var_os("QS400_QEMU").is_none() {
        eprintln!("QS400_QEMU not set; skipping boot test");
        return;
    }
    let image = conformance::default_image().expect("disk image from `build disk`");
    let test = BootTest::new(image).expect("qemu and OVMF present");
    match test.run() {
        Ok(r) => assert_eq!(r.seen.len(), test.expected.len()),
        Err(e) => {
            if let Some(log) = e.log() {
                eprintln!("{log}");
            }
            panic!("{e}");
        }
    }
}
