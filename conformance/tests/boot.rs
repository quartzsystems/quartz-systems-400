// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Phase 1 suite as a cargo test. Runs only when `QS400_QEMU=1`;
//! otherwise it skips, so a plain `cargo test` stays green on hosts without
//! QEMU or an image.

use conformance::{Media, Suite};

#[test]
fn phase1_suite() {
    if std::env::var_os("QS400_QEMU").is_none() {
        eprintln!("QS400_QEMU not set; skipping the boot suite");
        return;
    }
    let image = conformance::default_image().expect("disk image from `build disk`");
    let suite = Suite::new(
        Some(&image),
        None,
        Media::Disk,
        conformance::target_dir().join("conformance"),
    )
    .expect("qemu and OVMF present");
    match suite.run_all() {
        Ok(reports) => assert_eq!(reports.len(), 6),
        Err(e) => {
            if let Some(log) = e.log() {
                eprintln!("{log}");
            }
            panic!("{e}");
        }
    }
}

#[test]
fn phase1_media_suite() {
    if std::env::var_os("QS400_QEMU").is_none() {
        eprintln!("QS400_QEMU not set; skipping the media suite");
        return;
    }
    let image = conformance::default_image().expect("disk image from `build disk`");
    let iso = conformance::default_iso().expect("ISO from `build iso`");
    let suite = Suite::new(
        Some(&image),
        Some(&iso),
        Media::Iso,
        conformance::target_dir().join("conformance-iso"),
    )
    .expect("qemu and OVMF present");
    match suite.run_all() {
        Ok(reports) => assert_eq!(reports.len(), 3),
        Err(e) => {
            if let Some(log) = e.log() {
                eprintln!("{log}");
            }
            panic!("{e}");
        }
    }
}
