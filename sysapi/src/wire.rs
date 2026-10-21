// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The provider's types on the wire between a job and the storage
//! service.

use ipc::{wire_enum, wire_struct};

use crate::provider::{
    ApiFail, CommitInfo, DataArea, DataAreaType, DepInfo, DtaqEntry, DtaqInfo, FieldInfo,
    FileInfo, FormatInfo, IdxAttr, LocalNow, MemberInfo, ObjInfo, SysVal, SysValKind, UserInfo,
};

wire_struct!(ApiFail { msgid, data });
wire_struct!(ObjInfo {
    name,
    lib,
    ty,
    attr,
    text,
    owner,
    created,
    changed,
    source,
    user_attr
});
wire_struct!(IdxAttr {
    fixed,
    entry_len,
    keyed,
    key_len,
    immediate,
    sequential,
    text
});
wire_struct!(DtaqEntry { key, data, sender });
wire_struct!(DtaqInfo {
    name,
    lib,
    max_len,
    key_len,
    sequence,
    sender_id,
    text,
    entries,
    allocated
});
wire_enum!(DataAreaType {
    0 => Char,
    1 => Dec,
    2 => Lgl,
});
wire_struct!(DataArea {
    ty,
    lib,
    len,
    decimals,
    value
});
wire_enum!(SysValKind {
    0 => Char,
    1 => Bin,
});
wire_struct!(SysVal { kind, data });
wire_struct!(LocalNow {
    unix,
    offset_min,
    zone,
    dst
});
wire_struct!(FileInfo {
    name,
    lib,
    attr,
    text,
    rec_len,
    keyed,
    source,
    based_on
});
wire_struct!(MemberInfo {
    name,
    srctype,
    created,
    changed,
    text,
    records,
    deleted
});
wire_struct!(FormatInfo {
    name,
    rec_len,
    text,
    fields
});
wire_struct!(FieldInfo {
    name,
    ty,
    pos,
    len,
    digits,
    decimals,
    text,
    headings
});
wire_struct!(DepInfo {
    name,
    lib,
    member,
    kind
});
wire_struct!(CommitInfo {
    lock_level,
    pending,
    journal
});
wire_struct!(UserInfo {
    name,
    status,
    class,
    special,
    text
});
