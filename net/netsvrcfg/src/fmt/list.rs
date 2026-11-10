// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where a list goes: `QZLSOLST`'s receiver and list information, and
//! `QZLSLSTI`'s user space, laid out as the primary lays them out
//! (`tools/conformance/oracle/netserver/zlsl/`).

use super::text::Rec;

/// `Qzls_List_Info`'s size.
pub const LIST_INFO_SIZE: usize = 64;
/// The generic header's size in a `QZLSLSTI` user space.
pub const SPACE_HEADER: usize = 576;
/// The input parameter section's size.
pub const SPACE_INPUT: usize = 43;
/// Where the list starts in the user space.
pub const SPACE_LIST: usize = SPACE_HEADER + SPACE_INPUT;

/// A list: its records and the record length the header states (0
/// for variable records).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct List {
    /// The records in order.
    pub records: Vec<Vec<u8>>,
    /// The record length; 0 when they vary.
    pub record_len: usize,
}

impl List {
    /// Bytes of every record.
    #[must_use]
    pub fn data_len(&self) -> usize {
        self.records.iter().map(Vec::len).sum()
    }
}

/// `QZLSOLST`'s answer: the list information, then as many whole
/// records as fit `receiver_len` bytes. `created` is `CYYMMDDHHMMSS`.
#[must_use]
pub fn receiver(list: &List, receiver_len: usize, created: &str) -> (Vec<u8>, Vec<u8>) {
    let mut data = Vec::new();
    let mut returned = 0;
    for r in &list.records {
        if data.len() + r.len() > receiver_len {
            break;
        }
        data.extend_from_slice(r);
        returned += 1;
    }
    let mut info = Rec::zeros(LIST_INFO_SIZE);
    info.size4(0, list.records.len());
    info.size4(4, returned);
    info.size4(8, list.record_len);
    info.size4(12, data.len());
    info.char(
        16,
        1,
        if returned == list.records.len() {
            "C"
        } else {
            "P"
        },
    );
    info.char(17, 13, created);
    (info.buf, data)
}

/// The bytes `QZLSLSTI` writes from offset 64 of the user space (the
/// user area before it is left alone): the generic header, the input
/// parameters (`space` and `library` of the user space, `format`,
/// `qualifier`) and the list.
#[must_use]
pub fn space(
    list: &List,
    format: &str,
    qualifier: &[u8],
    space_name: &[u8],
    created: &str,
) -> Vec<u8> {
    let used = SPACE_LIST + list.data_len();
    let mut r = Rec::zeros(used);
    r.size4(0x40, SPACE_HEADER);
    r.char(0x44, 4, "0300");
    r.char(0x48, 8, format);
    r.char(0x50, 10, "QZLSLSTI");
    r.char(0x5A, 13, created);
    r.char(0x67, 1, "C");
    r.size4(0x68, used);
    r.size4(0x6C, SPACE_HEADER);
    r.size4(0x70, SPACE_INPUT);
    r.bin4(0x74, 0);
    r.bin4(0x78, 0);
    r.size4(0x7C, SPACE_LIST);
    r.size4(
        0x80,
        if list.record_len == 0 {
            0
        } else {
            list.data_len()
        },
    );
    r.size4(0x84, list.records.len());
    r.size4(0x88, list.record_len);
    r.bin4(0x8C, 37);
    r.char(0x90, 2, "US");
    r.char(0x92, 3, "ENU");
    r.char(0x95, 1, "0");
    r.char(0xC0, 10, "QZLSLSTI");
    r.blanks(0xCB, 0x1C0 - 0xCB);
    let mut name = space_name.to_vec();
    name.resize(20, 0x40);
    let mut q = qualifier.to_vec();
    q.resize(15, 0x40);
    r.bytes(SPACE_HEADER, &name);
    r.char(SPACE_HEADER + 20, 8, format);
    r.bytes(SPACE_HEADER + 28, &q);
    let mut at = SPACE_LIST;
    for rec in &list.records {
        r.bytes(at, rec);
        at += rec.len();
    }
    r.buf.split_off(0x40)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmt::text::{bin4, fixed};

    #[test]
    fn a_short_receiver_gets_whole_records() {
        let l = List {
            records: vec![vec![1; 16], vec![2; 16], vec![3; 16]],
            record_len: 16,
        };
        let (info, data) = receiver(&l, 40, "1261109083806");
        assert_eq!(bin4(&info, 0), 3);
        assert_eq!(bin4(&info, 4), 2);
        assert_eq!(bin4(&info, 12), 32);
        assert_eq!(info[16], 0xD7);
        assert_eq!(data.len(), 32);
        let (info, _) = receiver(&l, 65535, "1261109083806");
        assert_eq!(info[16], 0xC3);
    }

    #[test]
    fn the_space_header_matches_the_primary() {
        let l = List {
            records: vec![vec![0; 16], vec![0; 16]],
            record_len: 16,
        };
        let s = space(
            &l,
            "ZLSL0800",
            &fixed("*ALL", 15),
            &fixed("ZLSSPC    QTEMP", 20),
            "1261109083950",
        );
        let at = |o: usize| bin4(&s, o - 0x40);
        assert_eq!(at(0x40), 576);
        assert_eq!(at(0x68), 651);
        assert_eq!(at(0x7C), 619);
        assert_eq!(at(0x80), 32);
        assert_eq!(s.len() + 0x40, 651);
    }
}
