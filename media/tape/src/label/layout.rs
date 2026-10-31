// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The 80-byte label layouts. EOF and EOV labels share the HDR layouts.

use crate::spec::{Field, Layout};

const fn f(name: &'static str, offset: usize, size: usize) -> Field {
    Field { name, offset, size }
}

/// Label length.
pub const LABEL_LEN: usize = 80;

/// Label identifier ("VOL", "HDR", "EOF", "EOV").
pub const LABEL_ID: Field = f("label_id", 0, 3);
/// Label number ("1" or "2").
pub const LABEL_NUMBER: Field = f("label_number", 3, 1);

/// VOL1: volume identifier.
pub const VOL_ID: Field = f("volume_id", 4, 6);
/// VOL1: accessibility.
pub const VOL_ACCESS: Field = f("accessibility", 10, 1);
/// VOL1: owner identifier.
pub const VOL_OWNER: Field = f("owner_id", 37, 14);

/// The VOL1 label.
pub const VOL1: Layout = Layout {
    name: "VOL1",
    size: LABEL_LEN,
    fields: &[
        LABEL_ID,
        LABEL_NUMBER,
        VOL_ID,
        VOL_ACCESS,
        f("reserved_1", 11, 26),
        VOL_OWNER,
        f("reserved_2", 51, 29),
    ],
};

/// HDR1: file identifier (the file label).
pub const H1_FILE_ID: Field = f("file_id", 4, 17);
/// HDR1: volume identifier of the file's first volume.
pub const H1_SET_ID: Field = f("file_set_id", 21, 6);
/// HDR1: volume sequence number within the file.
pub const H1_VOL_SEQ: Field = f("volume_seq", 27, 4);
/// HDR1: file sequence number.
pub const H1_FILE_SEQ: Field = f("file_seq", 31, 4);
/// HDR1: generation number.
pub const H1_GENERATION: Field = f("generation", 35, 4);
/// HDR1: generation version.
pub const H1_VERSION: Field = f("version", 39, 2);
/// HDR1: creation date, `cyyddd`.
pub const H1_CREATED: Field = f("creation_date", 41, 6);
/// HDR1: expiration date, `cyyddd`.
pub const H1_EXPIRES: Field = f("expiration_date", 47, 6);
/// HDR1: file security.
pub const H1_SECURITY: Field = f("security", 53, 1);
/// HDR1: block count, low six digits (zeros in HDR1).
pub const H1_BLOCKS: Field = f("block_count", 54, 6);
/// HDR1: system code.
pub const H1_SYSTEM: Field = f("system_code", 60, 13);
/// HDR1: block count, high four digits.
pub const H1_BLOCKS_HIGH: Field = f("block_count_high", 76, 4);

/// The HDR1, EOF1 and EOV1 labels.
pub const HDR1: Layout = Layout {
    name: "HDR1",
    size: LABEL_LEN,
    fields: &[
        LABEL_ID,
        LABEL_NUMBER,
        H1_FILE_ID,
        H1_SET_ID,
        H1_VOL_SEQ,
        H1_FILE_SEQ,
        H1_GENERATION,
        H1_VERSION,
        H1_CREATED,
        H1_EXPIRES,
        H1_SECURITY,
        H1_BLOCKS,
        H1_SYSTEM,
        f("reserved", 73, 3),
        H1_BLOCKS_HIGH,
    ],
};

/// HDR2: record format (F, V or U).
pub const H2_FORMAT: Field = f("record_format", 4, 1);
/// HDR2: block length (zeros when the large block length holds it).
pub const H2_BLOCK_LEN: Field = f("block_length", 5, 5);
/// HDR2: record length.
pub const H2_RECORD_LEN: Field = f("record_length", 10, 5);
/// HDR2: density.
pub const H2_DENSITY: Field = f("density", 15, 1);
/// HDR2: volume switch ("1" on a continuation volume).
pub const H2_SWITCH: Field = f("volume_switch", 16, 1);
/// HDR2: job and step that wrote the file.
pub const H2_JOB: Field = f("job_step", 17, 17);
/// HDR2: block attribute (B blocked, S spanned, blank).
pub const H2_BLOCK_ATTR: Field = f("block_attribute", 38, 1);
/// HDR2: block length over 99999 bytes.
pub const H2_LARGE_BLOCK: Field = f("large_block_length", 70, 10);

/// The HDR2, EOF2 and EOV2 labels.
pub const HDR2: Layout = Layout {
    name: "HDR2",
    size: LABEL_LEN,
    fields: &[
        LABEL_ID,
        LABEL_NUMBER,
        H2_FORMAT,
        H2_BLOCK_LEN,
        H2_RECORD_LEN,
        H2_DENSITY,
        H2_SWITCH,
        H2_JOB,
        f("recording", 34, 2),
        f("control_char", 36, 1),
        f("reserved_1", 37, 1),
        H2_BLOCK_ATTR,
        f("reserved_2", 39, 31),
        H2_LARGE_BLOCK,
    ],
};
