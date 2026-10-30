// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IBM's IFS structures in job storage, as `QSYSINC` lays them out
//! without `_LARGE_FILES` or `_64_BIT_TIME`: integers big-endian,
//! `int`, `long`, `size_t`, `off_t`, `time_t`, `mode_t`, `ino_t`,
//! `uid_t`, `gid_t` and `dev_t` 4 bytes, `nlink_t` 2, `off64_t` and
//! `dev64_t` 8, pointers 16 on 16-byte boundaries. The tables of
//! `docs/IFS.md` "APIs" repeat these; `tests/ifs_spec.rs` keeps them
//! equal.

use ifs::ops::{Stat, StatVfs};

pub use sockets::layout::{Field, Layout};

const fn f(name: &'static str, offset: usize, size: usize) -> Field {
    Field { name, offset, size }
}

/// `struct stat`.
pub const STAT: Layout = Layout {
    name: "stat",
    size: 128,
    fields: &[
        f("st_mode", 0, 4),
        f("st_ino", 4, 4),
        f("st_nlink", 8, 2),
        f("st_reserved2", 10, 2),
        f("st_uid", 12, 4),
        f("st_gid", 16, 4),
        f("st_size", 20, 4),
        f("st_atime", 24, 4),
        f("st_mtime", 28, 4),
        f("st_ctime", 32, 4),
        f("st_dev", 36, 4),
        f("st_blksize", 40, 4),
        f("st_allocsize", 44, 4),
        f("st_objtype", 48, 11),
        f("st_reserved3", 59, 1),
        f("st_codepage", 60, 2),
        f("st_ccsid", 62, 2),
        f("st_rdev", 64, 4),
        f("st_nlink32", 68, 4),
        f("st_rdev64", 72, 8),
        f("st_dev64", 80, 8),
        f("st_vfs", 88, 4),
        f("st_reserved1", 92, 32),
        f("st_ino_gen_id", 124, 4),
    ],
};

/// `struct stat64`.
pub const STAT64: Layout = Layout {
    name: "stat64",
    size: 128,
    fields: &[
        f("st_mode", 0, 4),
        f("st_ino", 4, 4),
        f("st_uid", 8, 4),
        f("st_gid", 12, 4),
        f("st_size", 16, 8),
        f("st_atime", 24, 4),
        f("st_mtime", 28, 4),
        f("st_ctime", 32, 4),
        f("st_dev", 36, 4),
        f("st_blksize", 40, 4),
        f("st_nlink", 44, 2),
        f("st_codepage", 46, 2),
        f("st_allocsize", 48, 8),
        f("st_ino_gen_id", 56, 4),
        f("st_objtype", 60, 11),
        f("st_reserved2", 71, 5),
        f("st_rdev", 76, 4),
        f("st_rdev64", 80, 8),
        f("st_dev64", 88, 8),
        f("st_nlink32", 96, 4),
        f("st_vfs", 100, 4),
        f("st_reserved1", 104, 22),
        f("st_ccsid", 126, 2),
    ],
};

/// `struct dirent`.
pub const DIRENT: Layout = Layout {
    name: "dirent",
    size: 696,
    fields: &[
        f("d_reserved1", 0, 16),
        f("d_fileno_gen_id", 16, 4),
        f("d_fileno", 20, 4),
        f("d_reclen", 24, 4),
        f("d_reserved3", 28, 4),
        f("d_reserved4", 32, 8),
        f("d_nlsinfo", 40, 12),
        f("d_namelen", 52, 4),
        f("d_name", 56, 640),
    ],
};

/// `struct dirent_lg`.
pub const DIRENT_LG: Layout = Layout {
    name: "dirent_lg",
    size: 2624,
    fields: &[
        f("d_lg_reserved1", 0, 16),
        f("d_lg_fileno_gen_id", 16, 4),
        f("d_lg_fileno", 20, 4),
        f("d_lg_reclen", 24, 4),
        f("d_lg_reserved3", 28, 4),
        f("d_lg_reserved4", 32, 8),
        f("d_lg_qlg", 40, 32),
        f("d_lg_name", 72, 2550),
    ],
};

/// `struct statvfs` (and `statvfs64`, laid out the same).
pub const STATVFS: Layout = Layout {
    name: "statvfs",
    size: 160,
    fields: &[
        f("f_bsize", 0, 4),
        f("f_frsize", 4, 4),
        f("f_blocks", 8, 8),
        f("f_bfree", 16, 8),
        f("f_bavail", 24, 8),
        f("f_files", 32, 4),
        f("f_ffree", 36, 4),
        f("f_favail", 40, 4),
        f("f_fsid", 44, 4),
        f("f_flag", 48, 4),
        f("f_namemax", 52, 4),
        f("f_pathmax", 56, 4),
        f("f_objlinkmax", 60, 4),
        f("f_dirlinkmax", 64, 4),
        f("f_reserved1", 68, 4),
        f("f_fsid64", 72, 8),
        f("f_basetype", 80, 80),
    ],
};

/// `Qlg_Path_Name_T`; the name (or a pointer to it) follows at 32.
pub const QLG_PATH_NAME: Layout = Layout {
    name: "Qlg_Path_Name_T",
    size: 32,
    fields: &[
        f("CCSID", 0, 4),
        f("Country_ID", 4, 2),
        f("Language_ID", 6, 3),
        f("Reserved", 9, 3),
        f("Path_Type", 12, 4),
        f("Path_Length", 16, 4),
        f("Path_Name_Delimiter", 20, 2),
        f("Reserved2", 22, 10),
    ],
};

/// `Qp0l_Attr_Header_t`: before each attribute's data.
pub const ATTR_HEADER: Layout = Layout {
    name: "Qp0l_Attr_Header_t",
    size: 16,
    fields: &[
        f("Next_Attr_Offset", 0, 4),
        f("Attr_ID", 4, 4),
        f("Attr_Size", 8, 4),
        f("Reserved", 12, 4),
    ],
};

/// `Qp0l_Checkout_t`.
pub const CHECKOUT: Layout = Layout {
    name: "Qp0l_Checkout_t",
    size: 16,
    fields: &[
        f("Checked_Out_Flag", 0, 1),
        f("User_Name", 1, 10),
        f("Reserved", 11, 1),
        f("Checked_Out_Time", 12, 4),
    ],
};

/// `Qp0l_Authority_General_t`.
pub const AUTH_GENERAL: Layout = Layout {
    name: "Qp0l_Authority_General_t",
    size: 64,
    fields: &[
        f("Object_owner", 0, 10),
        f("Primary_group", 10, 10),
        f("Auth_list_name", 20, 10),
        f("Reserved1", 30, 10),
        f("UserArray_Offset", 40, 4),
        f("Number_of_users", 44, 4),
        f("UserEntry_size", 48, 4),
        f("Reserved2", 52, 12),
    ],
};

/// `Qp0l_Authority_Users_t`.
pub const AUTH_USERS: Layout = Layout {
    name: "Qp0l_Authority_Users_t",
    size: 48,
    fields: &[
        f("User_name", 0, 10),
        f("User_data_auth", 10, 10),
        f("OBJMGT_rights", 20, 1),
        f("OBJEXIST_rights", 21, 1),
        f("OBJALTER_rights", 22, 1),
        f("OBJREF_rights", 23, 1),
        f("Reserved1", 24, 10),
        f("OBJOPR_DataRights", 34, 1),
        f("READ_DataRights", 35, 1),
        f("ADD_DataRights", 36, 1),
        f("UPDATE_DataRights", 37, 1),
        f("DELETE_DataRights", 38, 1),
        f("EXECUTE_DataRights", 39, 1),
        f("EXCLUDE_DataRights", 40, 1),
        f("Reserved2", 41, 7),
    ],
};

/// `Qp0l_Journal_Info_t`.
pub const JOURNAL_INFO: Layout = Layout {
    name: "Qp0l_Journal_Info_t",
    size: 36,
    fields: &[
        f("Status", 0, 1),
        f("Options", 1, 1),
        f("Journal_Identifier", 2, 10),
        f("Journal_Name", 12, 10),
        f("Journal_Library", 22, 10),
        f("Time_Last_Started", 32, 4),
    ],
};

/// `Qp0l_QSYS_Info_t` (format `QSYS0100`).
pub const QSYS_INFO: Layout = Layout {
    name: "Qp0l_QSYS_Info_t",
    size: 184,
    fields: &[
        f("Bytes_Returned", 0, 4),
        f("Bytes_Available", 4, 4),
        f("CCSID_Out", 8, 4),
        f("Lib_Name", 12, 28),
        f("Lib_Type", 40, 20),
        f("Obj_Name", 60, 28),
        f("Obj_Type", 88, 20),
        f("Mbr_Name", 108, 28),
        f("Mbr_Type", 136, 20),
        f("Asp_Name", 156, 28),
    ],
};

/// `Qp0l_User_Function_t`.
pub const USER_FUNCTION: Layout = Layout {
    name: "Qp0l_User_Function_t",
    size: 48,
    fields: &[
        f("Function_Type", 0, 4),
        f("Library", 4, 10),
        f("Program", 14, 10),
        f("Mltthdacn", 24, 1),
        f("Reserved", 25, 7),
        f("Procedure", 32, 16),
    ],
};

/// `struct flock`.
pub const FLOCK: Layout = Layout {
    name: "flock",
    size: 48,
    fields: &[
        f("l_type", 0, 2),
        f("l_whence", 2, 2),
        f("l_start", 4, 4),
        f("l_len", 8, 4),
        f("l_pid", 12, 4),
        f("l_reserved0", 16, 16),
        f("l_reserved1", 32, 16),
    ],
};

/// `struct flock64`.
pub const FLOCK64: Layout = Layout {
    name: "flock64",
    size: 64,
    fields: &[
        f("l_type", 0, 2),
        f("l_whence", 2, 2),
        f("l_reserved2", 4, 4),
        f("l_start", 8, 8),
        f("l_len", 16, 8),
        f("l_pid", 24, 4),
        f("l_reserved3", 28, 4),
        f("l_reserved0", 32, 16),
        f("l_reserved1", 48, 16),
    ],
};

/// `struct utimbuf`.
pub const UTIMBUF: Layout = Layout {
    name: "utimbuf",
    size: 8,
    fields: &[f("actime", 0, 4), f("modtime", 4, 4)],
};

/// `Qp0l_RORO0100_Output_T`.
pub const RORO0100: Layout = Layout {
    name: "Qp0l_RORO0100_Output_T",
    size: 24,
    fields: &[
        f("BytesReturned", 0, 4),
        f("BytesAvailable", 4, 4),
        f("SimTypesOffset", 8, 4),
        f("SimTypesLength", 12, 4),
        f("Count", 16, 4),
        f("InUse", 20, 4),
    ],
};

/// `Qp0l_Sim_Ref_Types_Output_T`.
pub const SIM_REF_TYPES: Layout = Layout {
    name: "Qp0l_Sim_Ref_Types_Output_T",
    size: 64,
    fields: &[
        f("Read", 0, 4),
        f("Write", 4, 4),
        f("RdWr", 8, 4),
        f("Execute", 12, 4),
        f("ShareRead", 16, 4),
        f("ShareWrite", 20, 4),
        f("ShareRdWr", 24, 4),
        f("ShareNone", 28, 4),
        f("Attributes", 32, 4),
        f("Save", 36, 4),
        f("SaveInt", 40, 4),
        f("Links", 44, 4),
        f("CheckedOut", 48, 4),
        f("CheckOutName", 52, 10),
        f("Reserved0", 62, 2),
    ],
};

/// `Qp0l_RROO0100_Output_T`.
pub const RROO0100: Layout = Layout {
    name: "Qp0l_RROO0100_Output_T",
    size: 24,
    fields: &[
        f("BytesReturned", 0, 4),
        f("BytesAvailable", 4, 4),
        f("ObjectsReturned", 8, 4),
        f("ObjectsAvailable", 12, 4),
        f("ObjListOffset", 16, 4),
        f("ReferenceStatus", 20, 4),
    ],
};

/// `Qp0l_Obj_List_Output_T`.
pub const OBJ_LIST: Layout = Layout {
    name: "Qp0l_Obj_List_Output_T",
    size: 64,
    fields: &[
        f("NextListEntryDisp", 0, 4),
        f("RefDisp", 4, 4),
        f("RefLength", 8, 4),
        f("RefCount", 12, 4),
        f("PathDisp", 16, 4),
        f("PathLength", 20, 4),
        f("FileID", 24, 16),
        f("FileSysId", 40, 8),
        f("FileSysType", 48, 4),
        f("FileIdNumber", 52, 4),
        f("GenId", 56, 4),
        f("Reserved", 60, 4),
    ],
};

/// Every layout, in the order the spec lists them.
#[must_use]
pub fn layouts() -> Vec<&'static Layout> {
    vec![
        &STAT,
        &STAT64,
        &DIRENT,
        &DIRENT_LG,
        &STATVFS,
        &QLG_PATH_NAME,
        &ATTR_HEADER,
        &CHECKOUT,
        &AUTH_GENERAL,
        &AUTH_USERS,
        &JOURNAL_INFO,
        &QSYS_INFO,
        &USER_FUNCTION,
        &FLOCK,
        &FLOCK64,
        &UTIMBUF,
        &RORO0100,
        &SIM_REF_TYPES,
        &RROO0100,
        &OBJ_LIST,
    ]
}

/// Where a field is, by name; panics on a name the layout lacks.
#[must_use]
pub fn at(l: &Layout, name: &str) -> (usize, usize) {
    let fl = l
        .fields
        .iter()
        .find(|x| x.name == name)
        .unwrap_or_else(|| panic!("{} has no {name}", l.name));
    (fl.offset, fl.size)
}

/// Store bytes into a field, cut or zero padded to its size.
pub fn put(b: &mut [u8], l: &Layout, name: &str, v: &[u8]) {
    let (o, n) = at(l, name);
    let dst = &mut b[o..o + n];
    dst.fill(0);
    let k = v.len().min(n);
    dst[..k].copy_from_slice(&v[..k]);
}

/// Store an unsigned integer big-endian into a field of 1, 2, 4 or 8 bytes.
pub fn put_n(b: &mut [u8], l: &Layout, name: &str, v: u64) {
    let (o, n) = at(l, name);
    let bytes = v.to_be_bytes();
    b[o..o + n].copy_from_slice(&bytes[8 - n..]);
}

/// An unsigned big-endian field.
#[must_use]
pub fn get_n(b: &[u8], l: &Layout, name: &str) -> u64 {
    let (o, n) = at(l, name);
    let mut v = [0u8; 8];
    v[8 - n..].copy_from_slice(&b[o..o + n]);
    u64::from_be_bytes(v)
}

/// A signed big-endian field.
#[must_use]
pub fn get_i(b: &[u8], l: &Layout, name: &str) -> i64 {
    let (_, n) = at(l, name);
    let v = get_n(b, l, name);
    let shift = 64 - 8 * n;
    #[allow(clippy::cast_possible_wrap)]
    let s = ((v << shift) as i64) >> shift;
    s
}

/// Seconds of a nanosecond time, as a 32-bit `time_t` holds them.
#[must_use]
pub fn secs(t: i64) -> u64 {
    let s = t.div_euclid(1_000_000_000);
    #[allow(clippy::cast_sign_loss)]
    let v = i32::try_from(s).unwrap_or(if s < 0 { i32::MIN } else { i32::MAX }) as u32;
    u64::from(v)
}

/// An object type as `st_objtype` holds it: ten blank-padded EBCDIC
/// characters and a NUL.
fn objtype(s: &str) -> Vec<u8> {
    let mut v = crate::param::fixed(s, 10);
    v.push(0);
    v
}

/// Why a 32-bit field cannot hold a value: `EOVERFLOW`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overflow;

/// `struct stat` of a [`Stat`]; `Overflow` when the size needs 64 bits.
pub fn stat(s: &Stat) -> Result<Vec<u8>, Overflow> {
    let size = u32::try_from(s.size).map_err(|_| Overflow)?;
    let l = &STAT;
    let mut b = vec![0u8; l.size];
    put_n(&mut b, l, "st_mode", u64::from(s.mode));
    put_n(&mut b, l, "st_ino", s.ino & 0xFFFF_FFFF);
    put_n(&mut b, l, "st_nlink", u64::from(s.nlink.min(0xFFFF)));
    put_n(&mut b, l, "st_uid", u64::from(s.uid));
    put_n(&mut b, l, "st_gid", u64::from(s.gid));
    put_n(&mut b, l, "st_size", u64::from(size));
    put_n(&mut b, l, "st_atime", secs(s.atime));
    put_n(&mut b, l, "st_mtime", secs(s.mtime));
    put_n(&mut b, l, "st_ctime", secs(s.ctime));
    put_n(&mut b, l, "st_dev", u64::from(s.dev));
    put_n(&mut b, l, "st_blksize", u64::from(s.blksize));
    put_n(&mut b, l, "st_allocsize", s.allocsize.min(0xFFFF_FFFF));
    put(&mut b, l, "st_objtype", &objtype(&s.objtype));
    put_n(&mut b, l, "st_codepage", u64::from(s.codepage));
    put_n(&mut b, l, "st_ccsid", u64::from(s.ccsid));
    put_n(&mut b, l, "st_rdev", s.rdev & 0xFFFF_FFFF);
    put_n(&mut b, l, "st_nlink32", u64::from(s.nlink));
    put_n(&mut b, l, "st_rdev64", s.rdev);
    put_n(&mut b, l, "st_dev64", u64::from(s.dev));
    put_n(&mut b, l, "st_vfs", u64::from(s.vfs));
    put_n(&mut b, l, "st_ino_gen_id", u64::from(s.ino_gen));
    Ok(b)
}

/// `struct stat64` of a [`Stat`].
#[must_use]
pub fn stat64(s: &Stat) -> Vec<u8> {
    let l = &STAT64;
    let mut b = vec![0u8; l.size];
    put_n(&mut b, l, "st_mode", u64::from(s.mode));
    put_n(&mut b, l, "st_ino", s.ino & 0xFFFF_FFFF);
    put_n(&mut b, l, "st_uid", u64::from(s.uid));
    put_n(&mut b, l, "st_gid", u64::from(s.gid));
    put_n(&mut b, l, "st_size", s.size);
    put_n(&mut b, l, "st_atime", secs(s.atime));
    put_n(&mut b, l, "st_mtime", secs(s.mtime));
    put_n(&mut b, l, "st_ctime", secs(s.ctime));
    put_n(&mut b, l, "st_dev", u64::from(s.dev));
    put_n(&mut b, l, "st_blksize", u64::from(s.blksize));
    put_n(&mut b, l, "st_nlink", u64::from(s.nlink.min(0xFFFF)));
    put_n(&mut b, l, "st_codepage", u64::from(s.codepage));
    put_n(&mut b, l, "st_allocsize", s.allocsize);
    put_n(&mut b, l, "st_ino_gen_id", u64::from(s.ino_gen));
    put(&mut b, l, "st_objtype", &objtype(&s.objtype));
    put_n(&mut b, l, "st_rdev", s.rdev & 0xFFFF_FFFF);
    put_n(&mut b, l, "st_rdev64", s.rdev);
    put_n(&mut b, l, "st_dev64", u64::from(s.dev));
    put_n(&mut b, l, "st_nlink32", u64::from(s.nlink));
    put_n(&mut b, l, "st_vfs", u64::from(s.vfs));
    put_n(&mut b, l, "st_ccsid", u64::from(s.ccsid));
    b
}

/// `struct statvfs` of a [`StatVfs`].
#[must_use]
pub fn statvfs(v: &StatVfs) -> Vec<u8> {
    let l = &STATVFS;
    let mut b = vec![0u8; l.size];
    let cut = |x: u64| x.min(0xFFFF_FFFF);
    put_n(&mut b, l, "f_bsize", cut(v.bsize));
    put_n(&mut b, l, "f_frsize", cut(v.frsize));
    put_n(&mut b, l, "f_blocks", v.blocks);
    put_n(&mut b, l, "f_bfree", v.bfree);
    put_n(&mut b, l, "f_bavail", v.bavail);
    put_n(&mut b, l, "f_files", cut(v.files));
    put_n(&mut b, l, "f_ffree", cut(v.ffree));
    put_n(&mut b, l, "f_favail", cut(v.favail));
    put_n(&mut b, l, "f_fsid", u64::from(v.fsid));
    let mut flag = v.flag;
    if v.case_sensitive {
        flag |= u64::from(super::consts::ST_CASE_SENSITIVE);
    }
    flag |= u64::from(super::consts::ST_CHOWN_RESTRICTED | super::consts::ST_THREAD_SAFE);
    put_n(&mut b, l, "f_flag", cut(flag));
    put_n(&mut b, l, "f_namemax", cut(v.namemax));
    put_n(&mut b, l, "f_pathmax", u64::from(u32::MAX));
    put_n(&mut b, l, "f_objlinkmax", u64::from(ifs::path::LINK_MAX));
    put_n(&mut b, l, "f_dirlinkmax", u64::from(ifs::path::LINK_MAX));
    put_n(&mut b, l, "f_fsid64", u64::from(v.fsid));
    let mut name = ccsid::to_ebcdic(&v.name).unwrap_or_default();
    name.truncate(79);
    name.push(0);
    put(&mut b, l, "f_basetype", &name);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_in_order_and_inside() {
        for l in layouts() {
            let mut end = 0;
            for fl in l.fields {
                assert!(fl.offset >= end, "{}.{} overlaps", l.name, fl.name);
                end = fl.offset + fl.size;
            }
            assert!(end <= l.size, "{} ends past its size", l.name);
        }
    }

    #[test]
    fn a_stat_in_storage() {
        let s = Stat {
            mode: 0o100_644,
            ino: 77,
            nlink: 1,
            size: 5,
            objtype: "*STMF".into(),
            codepage: 1252,
            ccsid: 1208,
            mtime: 3_000_000_000,
            ..Stat::default()
        };
        let b = stat(&s).unwrap();
        assert_eq!(get_n(&b, &STAT, "st_mode"), 0o100_644);
        assert_eq!(get_n(&b, &STAT, "st_size"), 5);
        assert_eq!(get_n(&b, &STAT, "st_mtime"), 3);
        assert_eq!(get_n(&b, &STAT, "st_ccsid"), 1208);
        assert_eq!(&b[48..59], &[0x5C, 0xE2, 0xE3, 0xD4, 0xC6, 0x40, 0x40, 0x40, 0x40, 0x40, 0]);
        let b = stat64(&Stat { size: 1 << 33, ..s.clone() });
        assert_eq!(get_n(&b, &STAT64, "st_size"), 1 << 33);
        assert_eq!(stat(&Stat { size: 1 << 33, ..s }), Err(Overflow));
        assert_eq!(get_i(&[0xFF, 0xFE], &UTIMBUF_PROBE, "x"), -2);
    }

    const UTIMBUF_PROBE: Layout = Layout {
        name: "probe",
        size: 2,
        fields: &[f("x", 0, 2)],
    };
}
