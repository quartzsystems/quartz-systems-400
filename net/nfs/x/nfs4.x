// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// NFS versions 4.0 and 4.1 as one description: RFC 5662 (the 4.1
// interface, a superset holding the 4.0 operations), checked against
// RFC 7531 (4.0). bitmap4 and attrlist4 stay opaque as in the RFCs;
// the attribute values have their own codec. The files layout types of
// pNFS are left out (pNFS is refused). authsys_parms is RFC 5531's,
// repeated here for callback_sec_parms4.

// RFC 5662 section 2: basic types and constants.
typedef int int32_t;
typedef unsigned int uint32_t;
typedef hyper int64_t;
typedef unsigned hyper uint64_t;

const NFS4_FHSIZE = 128;
const NFS4_VERIFIER_SIZE = 8;
const NFS4_OPAQUE_LIMIT = 1024;
const NFS4_SESSIONID_SIZE = 16;
const NFS4_INT64_MAX = 0x7fffffffffffffff;
const NFS4_UINT64_MAX = 0xffffffffffffffff;
const NFS4_INT32_MAX = 0x7fffffff;
const NFS4_UINT32_MAX = 0xffffffff;
const NFS4_MAXFILELEN = 0xffffffffffffffff;
const NFS4_MAXFILEOFF = 0xfffffffffffffffe;

const AUTH_NONE = 0;
const AUTH_SYS = 1;
const RPCSEC_GSS = 6;

enum nfs_ftype4 {
    NF4REG = 1,
    NF4DIR = 2,
    NF4BLK = 3,
    NF4CHR = 4,
    NF4LNK = 5,
    NF4SOCK = 6,
    NF4FIFO = 7,
    NF4ATTRDIR = 8,
    NF4NAMEDATTR = 9
};

enum nfsstat4 {
    NFS4_OK = 0,
    NFS4ERR_PERM = 1,
    NFS4ERR_NOENT = 2,
    NFS4ERR_IO = 5,
    NFS4ERR_NXIO = 6,
    NFS4ERR_ACCESS = 13,
    NFS4ERR_EXIST = 17,
    NFS4ERR_XDEV = 18,
    NFS4ERR_NOTDIR = 20,
    NFS4ERR_ISDIR = 21,
    NFS4ERR_INVAL = 22,
    NFS4ERR_FBIG = 27,
    NFS4ERR_NOSPC = 28,
    NFS4ERR_ROFS = 30,
    NFS4ERR_MLINK = 31,
    NFS4ERR_NAMETOOLONG = 63,
    NFS4ERR_NOTEMPTY = 66,
    NFS4ERR_DQUOT = 69,
    NFS4ERR_STALE = 70,
    NFS4ERR_BADHANDLE = 10001,
    NFS4ERR_BAD_COOKIE = 10003,
    NFS4ERR_NOTSUPP = 10004,
    NFS4ERR_TOOSMALL = 10005,
    NFS4ERR_SERVERFAULT = 10006,
    NFS4ERR_BADTYPE = 10007,
    NFS4ERR_DELAY = 10008,
    NFS4ERR_SAME = 10009,
    NFS4ERR_DENIED = 10010,
    NFS4ERR_EXPIRED = 10011,
    NFS4ERR_LOCKED = 10012,
    NFS4ERR_GRACE = 10013,
    NFS4ERR_FHEXPIRED = 10014,
    NFS4ERR_SHARE_DENIED = 10015,
    NFS4ERR_WRONGSEC = 10016,
    NFS4ERR_CLID_INUSE = 10017,
    NFS4ERR_RESOURCE = 10018,
    NFS4ERR_MOVED = 10019,
    NFS4ERR_NOFILEHANDLE = 10020,
    NFS4ERR_MINOR_VERS_MISMATCH = 10021,
    NFS4ERR_STALE_CLIENTID = 10022,
    NFS4ERR_STALE_STATEID = 10023,
    NFS4ERR_OLD_STATEID = 10024,
    NFS4ERR_BAD_STATEID = 10025,
    NFS4ERR_BAD_SEQID = 10026,
    NFS4ERR_NOT_SAME = 10027,
    NFS4ERR_LOCK_RANGE = 10028,
    NFS4ERR_SYMLINK = 10029,
    NFS4ERR_RESTOREFH = 10030,
    NFS4ERR_LEASE_MOVED = 10031,
    NFS4ERR_ATTRNOTSUPP = 10032,
    NFS4ERR_NO_GRACE = 10033,
    NFS4ERR_RECLAIM_BAD = 10034,
    NFS4ERR_RECLAIM_CONFLICT = 10035,
    NFS4ERR_BADXDR = 10036,
    NFS4ERR_LOCKS_HELD = 10037,
    NFS4ERR_OPENMODE = 10038,
    NFS4ERR_BADOWNER = 10039,
    NFS4ERR_BADCHAR = 10040,
    NFS4ERR_BADNAME = 10041,
    NFS4ERR_BAD_RANGE = 10042,
    NFS4ERR_LOCK_NOTSUPP = 10043,
    NFS4ERR_OP_ILLEGAL = 10044,
    NFS4ERR_DEADLOCK = 10045,
    NFS4ERR_FILE_OPEN = 10046,
    NFS4ERR_ADMIN_REVOKED = 10047,
    NFS4ERR_CB_PATH_DOWN = 10048,
    NFS4ERR_BADIOMODE = 10049,
    NFS4ERR_BADLAYOUT = 10050,
    NFS4ERR_BAD_SESSION_DIGEST = 10051,
    NFS4ERR_BADSESSION = 10052,
    NFS4ERR_BADSLOT = 10053,
    NFS4ERR_COMPLETE_ALREADY = 10054,
    NFS4ERR_CONN_NOT_BOUND_TO_SESSION = 10055,
    NFS4ERR_DELEG_ALREADY_WANTED = 10056,
    NFS4ERR_BACK_CHAN_BUSY = 10057,
    NFS4ERR_LAYOUTTRYLATER = 10058,
    NFS4ERR_LAYOUTUNAVAILABLE = 10059,
    NFS4ERR_NOMATCHING_LAYOUT = 10060,
    NFS4ERR_RECALLCONFLICT = 10061,
    NFS4ERR_UNKNOWN_LAYOUTTYPE = 10062,
    NFS4ERR_SEQ_MISORDERED = 10063,
    NFS4ERR_SEQUENCE_POS = 10064,
    NFS4ERR_REQ_TOO_BIG = 10065,
    NFS4ERR_REP_TOO_BIG = 10066,
    NFS4ERR_REP_TOO_BIG_TO_CACHE = 10067,
    NFS4ERR_RETRY_UNCACHED_REP = 10068,
    NFS4ERR_UNSAFE_COMPOUND = 10069,
    NFS4ERR_TOO_MANY_OPS = 10070,
    NFS4ERR_OP_NOT_IN_SESSION = 10071,
    NFS4ERR_HASH_ALG_UNSUPP = 10072,
    NFS4ERR_CLIENTID_BUSY = 10074,
    NFS4ERR_PNFS_IO_HOLE = 10075,
    NFS4ERR_SEQ_FALSE_RETRY = 10076,
    NFS4ERR_BAD_HIGH_SLOT = 10077,
    NFS4ERR_DEADSESSION = 10078,
    NFS4ERR_ENCR_ALG_UNSUPP = 10079,
    NFS4ERR_PNFS_NO_LAYOUT = 10080,
    NFS4ERR_NOT_ONLY_OP = 10081,
    NFS4ERR_WRONG_CRED = 10082,
    NFS4ERR_WRONG_TYPE = 10083,
    NFS4ERR_DIRDELEG_UNAVAIL = 10084,
    NFS4ERR_REJECT_DELEG = 10085,
    NFS4ERR_RETURNCONFLICT = 10086,
    NFS4ERR_DELEG_REVOKED = 10087
};

typedef opaque attrlist4<>;
typedef uint32_t bitmap4<>;
typedef uint64_t changeid4;
typedef uint64_t clientid4;
typedef uint32_t count4;
typedef uint64_t length4;
typedef uint32_t mode4;
typedef uint64_t nfs_cookie4;
typedef opaque nfs_fh4<NFS4_FHSIZE>;
typedef uint64_t offset4;
typedef uint32_t qop4;
typedef opaque sec_oid4<>;
typedef uint32_t sequenceid4;
typedef uint32_t seqid4;
typedef opaque sessionid4[NFS4_SESSIONID_SIZE];
typedef uint32_t slotid4;
typedef opaque utf8string<>;
typedef utf8string utf8str_cis;
typedef utf8string utf8str_cs;
typedef utf8string utf8str_mixed;
typedef utf8str_cs component4;
typedef utf8str_cs linktext4;
typedef component4 pathname4<>;
typedef opaque verifier4[NFS4_VERIFIER_SIZE];
typedef string secret4<>;
typedef uint32_t policy4;

struct nfstime4 {
    int64_t seconds;
    uint32_t nseconds;
};

enum time_how4 {
    SET_TO_SERVER_TIME4 = 0,
    SET_TO_CLIENT_TIME4 = 1
};

union settime4 switch (time_how4 set_it) {
case SET_TO_CLIENT_TIME4:
    nfstime4 time;
default:
    void;
};

typedef uint32_t nfs_lease4;

struct fsid4 {
    uint64_t major;
    uint64_t minor;
};

struct change_policy4 {
    uint64_t cp_major;
    uint64_t cp_minor;
};

struct fs_location4 {
    utf8str_cis server<>;
    pathname4 rootpath;
};

struct fs_locations4 {
    pathname4 fs_root;
    fs_location4 locations<>;
};

const ACL4_SUPPORT_ALLOW_ACL = 0x00000001;
const ACL4_SUPPORT_DENY_ACL = 0x00000002;
const ACL4_SUPPORT_AUDIT_ACL = 0x00000004;
const ACL4_SUPPORT_ALARM_ACL = 0x00000008;

typedef uint32_t acetype4;
const ACE4_ACCESS_ALLOWED_ACE_TYPE = 0x00000000;
const ACE4_ACCESS_DENIED_ACE_TYPE = 0x00000001;
const ACE4_SYSTEM_AUDIT_ACE_TYPE = 0x00000002;
const ACE4_SYSTEM_ALARM_ACE_TYPE = 0x00000003;

typedef uint32_t aceflag4;
const ACE4_FILE_INHERIT_ACE = 0x00000001;
const ACE4_DIRECTORY_INHERIT_ACE = 0x00000002;
const ACE4_NO_PROPAGATE_INHERIT_ACE = 0x00000004;
const ACE4_INHERIT_ONLY_ACE = 0x00000008;
const ACE4_SUCCESSFUL_ACCESS_ACE_FLAG = 0x00000010;
const ACE4_FAILED_ACCESS_ACE_FLAG = 0x00000020;
const ACE4_IDENTIFIER_GROUP = 0x00000040;
const ACE4_INHERITED_ACE = 0x00000080;

typedef uint32_t acemask4;
const ACE4_READ_DATA = 0x00000001;
const ACE4_LIST_DIRECTORY = 0x00000001;
const ACE4_WRITE_DATA = 0x00000002;
const ACE4_ADD_FILE = 0x00000002;
const ACE4_APPEND_DATA = 0x00000004;
const ACE4_ADD_SUBDIRECTORY = 0x00000004;
const ACE4_READ_NAMED_ATTRS = 0x00000008;
const ACE4_WRITE_NAMED_ATTRS = 0x00000010;
const ACE4_EXECUTE = 0x00000020;
const ACE4_DELETE_CHILD = 0x00000040;
const ACE4_READ_ATTRIBUTES = 0x00000080;
const ACE4_WRITE_ATTRIBUTES = 0x00000100;
const ACE4_WRITE_RETENTION = 0x00000200;
const ACE4_WRITE_RETENTION_HOLD = 0x00000400;
const ACE4_DELETE = 0x00010000;
const ACE4_READ_ACL = 0x00020000;
const ACE4_WRITE_ACL = 0x00040000;
const ACE4_WRITE_OWNER = 0x00080000;
const ACE4_SYNCHRONIZE = 0x00100000;
const ACE4_GENERIC_READ = 0x00120081;
const ACE4_GENERIC_WRITE = 0x00160106;
const ACE4_GENERIC_EXECUTE = 0x001200A0;

struct nfsace4 {
    acetype4 type;
    aceflag4 flag;
    acemask4 access_mask;
    utf8str_mixed who;
};

typedef uint32_t aclflag4;
const ACL4_AUTO_INHERIT = 0x00000001;
const ACL4_PROTECTED = 0x00000002;
const ACL4_DEFAULTED = 0x00000004;

struct nfsacl41 {
    aclflag4 na41_flag;
    nfsace4 na41_aces<>;
};

const MODE4_SUID = 0x800;
const MODE4_SGID = 0x400;
const MODE4_SVTX = 0x200;
const MODE4_RUSR = 0x100;
const MODE4_WUSR = 0x080;
const MODE4_XUSR = 0x040;
const MODE4_RGRP = 0x020;
const MODE4_WGRP = 0x010;
const MODE4_XGRP = 0x008;
const MODE4_ROTH = 0x004;
const MODE4_WOTH = 0x002;
const MODE4_XOTH = 0x001;

struct mode_masked4 {
    mode4 mm_value_to_set;
    mode4 mm_mask_bits;
};

struct specdata4 {
    uint32_t specdata1;
    uint32_t specdata2;
};

const FH4_PERSISTENT = 0x00000000;
const FH4_NOEXPIRE_WITH_OPEN = 0x00000001;
const FH4_VOLATILE_ANY = 0x00000002;
const FH4_VOL_MIGRATION = 0x00000004;
const FH4_VOL_RENAME = 0x00000008;

struct netaddr4 {
    string na_r_netid<>;
    string na_r_addr<>;
};

struct nfs_impl_id4 {
    utf8str_cis nii_domain;
    utf8str_cs nii_name;
    nfstime4 nii_date;
};

struct stateid4 {
    uint32_t seqid;
    opaque other[12];
};

enum layouttype4 {
    LAYOUT4_NFSV4_1_FILES = 0x1,
    LAYOUT4_OSD2_OBJECTS = 0x2,
    LAYOUT4_BLOCK_VOLUME = 0x3
};

struct layout_content4 {
    layouttype4 loc_type;
    opaque loc_body<>;
};

struct layouthint4 {
    layouttype4 loh_type;
    opaque loh_body<>;
};

enum layoutiomode4 {
    LAYOUTIOMODE4_READ = 1,
    LAYOUTIOMODE4_RW = 2,
    LAYOUTIOMODE4_ANY = 3
};

struct layout4 {
    offset4 lo_offset;
    length4 lo_length;
    layoutiomode4 lo_iomode;
    layout_content4 lo_content;
};

const NFS4_DEVICEID4_SIZE = 16;
typedef opaque deviceid4[NFS4_DEVICEID4_SIZE];

struct device_addr4 {
    layouttype4 da_layout_type;
    opaque da_addr_body<>;
};

struct layoutupdate4 {
    layouttype4 lou_type;
    opaque lou_body<>;
};

const LAYOUT4_RET_REC_FILE = 1;
const LAYOUT4_RET_REC_FSID = 2;
const LAYOUT4_RET_REC_ALL = 3;

enum layoutreturn_type4 {
    LAYOUTRETURN4_FILE = LAYOUT4_RET_REC_FILE,
    LAYOUTRETURN4_FSID = LAYOUT4_RET_REC_FSID,
    LAYOUTRETURN4_ALL = LAYOUT4_RET_REC_ALL
};

struct layoutreturn_file4 {
    offset4 lrf_offset;
    length4 lrf_length;
    stateid4 lrf_stateid;
    opaque lrf_body<>;
};

union layoutreturn4 switch (layoutreturn_type4 lr_returntype) {
case LAYOUTRETURN4_FILE:
    layoutreturn_file4 lr_layout;
default:
    void;
};

enum fs4_status_type {
    STATUS4_FIXED = 1,
    STATUS4_UPDATED = 2,
    STATUS4_VERSIONED = 3,
    STATUS4_WRITABLE = 4,
    STATUS4_REFERRAL = 5
};

struct fs4_status {
    bool fss_absent;
    fs4_status_type fss_type;
    utf8str_cs fss_source;
    utf8str_cs fss_current;
    int32_t fss_age;
    nfstime4 fss_version;
};

const TH4_READ_SIZE = 0;
const TH4_WRITE_SIZE = 1;
const TH4_READ_IOSIZE = 2;
const TH4_WRITE_IOSIZE = 3;

typedef length4 threshold4_read_size;
typedef length4 threshold4_write_size;
typedef length4 threshold4_read_iosize;
typedef length4 threshold4_write_iosize;

struct threshold_item4 {
    layouttype4 thi_layout_type;
    bitmap4 thi_hintset;
    opaque thi_hintlist<>;
};

struct mdsthreshold4 {
    threshold_item4 mth_hints<>;
};

const RET4_DURATION_INFINITE = 0xffffffffffffffff;

struct retention_get4 {
    uint64_t rg_duration;
    nfstime4 rg_begin_time<1>;
};

struct retention_set4 {
    bool rs_enable;
    uint64_t rs_duration<1>;
};

const FSCHARSET_CAP4_CONTAINS_NON_UTF8 = 0x1;
const FSCHARSET_CAP4_ALLOWS_ONLY_UTF8 = 0x2;
typedef uint32_t fs_charset_cap4;

struct fs_locations_server4 {
    int32_t fls_currency;
    opaque fls_info<>;
    utf8str_cis fls_server;
};

const FSLI4BX_GFLAGS = 0;
const FSLI4BX_TFLAGS = 1;
const FSLI4BX_CLSIMUL = 2;
const FSLI4BX_CLHANDLE = 3;
const FSLI4BX_CLFILEID = 4;
const FSLI4BX_CLWRITEVER = 5;
const FSLI4BX_CLCHANGE = 6;
const FSLI4BX_CLREADDIR = 7;
const FSLI4BX_READRANK = 8;
const FSLI4BX_WRITERANK = 9;
const FSLI4BX_READORDER = 10;
const FSLI4BX_WRITEORDER = 11;

const FSLI4GF_WRITABLE = 0x01;
const FSLI4GF_CUR_REQ = 0x02;
const FSLI4GF_ABSENT = 0x04;
const FSLI4GF_GOING = 0x08;
const FSLI4GF_SPLIT = 0x10;
const FSLI4TF_RDMA = 0x01;

struct fs_locations_item4 {
    fs_locations_server4 fli_entries<>;
    pathname4 fli_rootpath;
};

struct fs_locations_info4 {
    uint32_t fli_flags;
    int32_t fli_valid_for;
    pathname4 fli_fs_root;
    fs_locations_item4 fli_items<>;
};

const FSLI4IF_VAR_SUB = 0x00000001;

// The attributes' value types and numbers (RFC 5662 section 2, the
// fattr4_ typedefs; 0 to 55 are 4.0's).
typedef bitmap4 fattr4_supported_attrs;
typedef nfs_ftype4 fattr4_type;
typedef uint32_t fattr4_fh_expire_type;
typedef changeid4 fattr4_change;
typedef uint64_t fattr4_size;
typedef bool fattr4_link_support;
typedef bool fattr4_symlink_support;
typedef bool fattr4_named_attr;
typedef fsid4 fattr4_fsid;
typedef bool fattr4_unique_handles;
typedef nfs_lease4 fattr4_lease_time;
typedef nfsstat4 fattr4_rdattr_error;
typedef nfsace4 fattr4_acl<>;
typedef uint32_t fattr4_aclsupport;
typedef bool fattr4_archive;
typedef bool fattr4_cansettime;
typedef bool fattr4_case_insensitive;
typedef bool fattr4_case_preserving;
typedef bool fattr4_chown_restricted;
typedef nfs_fh4 fattr4_filehandle;
typedef uint64_t fattr4_fileid;
typedef uint64_t fattr4_files_avail;
typedef uint64_t fattr4_files_free;
typedef uint64_t fattr4_files_total;
typedef fs_locations4 fattr4_fs_locations;
typedef bool fattr4_hidden;
typedef bool fattr4_homogeneous;
typedef uint64_t fattr4_maxfilesize;
typedef uint32_t fattr4_maxlink;
typedef uint32_t fattr4_maxname;
typedef uint64_t fattr4_maxread;
typedef uint64_t fattr4_maxwrite;
typedef utf8str_cs fattr4_mimetype;
typedef mode4 fattr4_mode;
typedef bool fattr4_no_trunc;
typedef uint32_t fattr4_numlinks;
typedef utf8str_mixed fattr4_owner;
typedef utf8str_mixed fattr4_owner_group;
typedef uint64_t fattr4_quota_avail_hard;
typedef uint64_t fattr4_quota_avail_soft;
typedef uint64_t fattr4_quota_used;
typedef specdata4 fattr4_rawdev;
typedef uint64_t fattr4_space_avail;
typedef uint64_t fattr4_space_free;
typedef uint64_t fattr4_space_total;
typedef uint64_t fattr4_space_used;
typedef bool fattr4_system;
typedef nfstime4 fattr4_time_access;
typedef settime4 fattr4_time_access_set;
typedef nfstime4 fattr4_time_backup;
typedef nfstime4 fattr4_time_create;
typedef nfstime4 fattr4_time_delta;
typedef nfstime4 fattr4_time_metadata;
typedef nfstime4 fattr4_time_modify;
typedef settime4 fattr4_time_modify_set;
typedef uint64_t fattr4_mounted_on_fileid;
typedef nfstime4 fattr4_dir_notif_delay;
typedef nfstime4 fattr4_dirent_notif_delay;
typedef nfsacl41 fattr4_dacl;
typedef nfsacl41 fattr4_sacl;
typedef change_policy4 fattr4_change_policy;
typedef fs4_status fattr4_fs_status;
typedef layouttype4 fattr4_fs_layout_types<>;
typedef layouthint4 fattr4_layout_hint;
typedef layouttype4 fattr4_layout_types<>;
typedef uint32_t fattr4_layout_blksize;
typedef uint32_t fattr4_layout_alignment;
typedef fs_locations_info4 fattr4_fs_locations_info;
typedef mdsthreshold4 fattr4_mdsthreshold;
typedef retention_get4 fattr4_retention_get;
typedef retention_set4 fattr4_retention_set;
typedef retention_get4 fattr4_retentevt_get;
typedef retention_set4 fattr4_retentevt_set;
typedef uint64_t fattr4_retention_hold;
typedef mode_masked4 fattr4_mode_set_masked;
typedef bitmap4 fattr4_suppattr_exclcreat;
typedef fs_charset_cap4 fattr4_fs_charset_cap;

const FATTR4_SUPPORTED_ATTRS = 0;
const FATTR4_TYPE = 1;
const FATTR4_FH_EXPIRE_TYPE = 2;
const FATTR4_CHANGE = 3;
const FATTR4_SIZE = 4;
const FATTR4_LINK_SUPPORT = 5;
const FATTR4_SYMLINK_SUPPORT = 6;
const FATTR4_NAMED_ATTR = 7;
const FATTR4_FSID = 8;
const FATTR4_UNIQUE_HANDLES = 9;
const FATTR4_LEASE_TIME = 10;
const FATTR4_RDATTR_ERROR = 11;
const FATTR4_FILEHANDLE = 19;
const FATTR4_SUPPATTR_EXCLCREAT = 75;
const FATTR4_ACL = 12;
const FATTR4_ACLSUPPORT = 13;
const FATTR4_ARCHIVE = 14;
const FATTR4_CANSETTIME = 15;
const FATTR4_CASE_INSENSITIVE = 16;
const FATTR4_CASE_PRESERVING = 17;
const FATTR4_CHOWN_RESTRICTED = 18;
const FATTR4_FILEID = 20;
const FATTR4_FILES_AVAIL = 21;
const FATTR4_FILES_FREE = 22;
const FATTR4_FILES_TOTAL = 23;
const FATTR4_FS_LOCATIONS = 24;
const FATTR4_HIDDEN = 25;
const FATTR4_HOMOGENEOUS = 26;
const FATTR4_MAXFILESIZE = 27;
const FATTR4_MAXLINK = 28;
const FATTR4_MAXNAME = 29;
const FATTR4_MAXREAD = 30;
const FATTR4_MAXWRITE = 31;
const FATTR4_MIMETYPE = 32;
const FATTR4_MODE = 33;
const FATTR4_NO_TRUNC = 34;
const FATTR4_NUMLINKS = 35;
const FATTR4_OWNER = 36;
const FATTR4_OWNER_GROUP = 37;
const FATTR4_QUOTA_AVAIL_HARD = 38;
const FATTR4_QUOTA_AVAIL_SOFT = 39;
const FATTR4_QUOTA_USED = 40;
const FATTR4_RAWDEV = 41;
const FATTR4_SPACE_AVAIL = 42;
const FATTR4_SPACE_FREE = 43;
const FATTR4_SPACE_TOTAL = 44;
const FATTR4_SPACE_USED = 45;
const FATTR4_SYSTEM = 46;
const FATTR4_TIME_ACCESS = 47;
const FATTR4_TIME_ACCESS_SET = 48;
const FATTR4_TIME_BACKUP = 49;
const FATTR4_TIME_CREATE = 50;
const FATTR4_TIME_DELTA = 51;
const FATTR4_TIME_METADATA = 52;
const FATTR4_TIME_MODIFY = 53;
const FATTR4_TIME_MODIFY_SET = 54;
const FATTR4_MOUNTED_ON_FILEID = 55;
const FATTR4_DIR_NOTIF_DELAY = 56;
const FATTR4_DIRENT_NOTIF_DELAY = 57;
const FATTR4_DACL = 58;
const FATTR4_SACL = 59;
const FATTR4_CHANGE_POLICY = 60;
const FATTR4_FS_STATUS = 61;
const FATTR4_FS_LAYOUT_TYPES = 62;
const FATTR4_LAYOUT_HINT = 63;
const FATTR4_LAYOUT_TYPES = 64;
const FATTR4_LAYOUT_BLKSIZE = 65;
const FATTR4_LAYOUT_ALIGNMENT = 66;
const FATTR4_FS_LOCATIONS_INFO = 67;
const FATTR4_MDSTHRESHOLD = 68;
const FATTR4_RETENTION_GET = 69;
const FATTR4_RETENTION_SET = 70;
const FATTR4_RETENTEVT_GET = 71;
const FATTR4_RETENTEVT_SET = 72;
const FATTR4_RETENTION_HOLD = 73;
const FATTR4_MODE_SET_MASKED = 74;
const FATTR4_FS_CHARSET_CAP = 76;

struct fattr4 {
    bitmap4 attrmask;
    attrlist4 attr_vals;
};

struct change_info4 {
    bool atomic;
    changeid4 before;
    changeid4 after;
};

typedef netaddr4 clientaddr4;

struct cb_client4 {
    uint32_t cb_program;
    netaddr4 cb_location;
};

struct nfs_client_id4 {
    verifier4 verifier;
    opaque id<NFS4_OPAQUE_LIMIT>;
};

struct client_owner4 {
    verifier4 co_verifier;
    opaque co_ownerid<NFS4_OPAQUE_LIMIT>;
};

struct server_owner4 {
    uint64_t so_minor_id;
    opaque so_major_id<NFS4_OPAQUE_LIMIT>;
};

struct state_owner4 {
    clientid4 clientid;
    opaque owner<NFS4_OPAQUE_LIMIT>;
};

typedef state_owner4 open_owner4;
typedef state_owner4 lock_owner4;

enum nfs_lock_type4 {
    READ_LT = 1,
    WRITE_LT = 2,
    READW_LT = 3,
    WRITEW_LT = 4
};

enum ssv_subkey4 {
    SSV4_SUBKEY_MIC_I2T = 1,
    SSV4_SUBKEY_MIC_T2I = 2,
    SSV4_SUBKEY_SEAL_I2T = 3,
    SSV4_SUBKEY_SEAL_T2I = 4
};

struct ssv_mic_plain_tkn4 {
    uint32_t smpt_ssv_seq;
    opaque smpt_orig_plain<>;
};

struct ssv_mic_tkn4 {
    uint32_t smt_ssv_seq;
    opaque smt_hmac<>;
};

struct ssv_seal_plain_tkn4 {
    opaque sspt_confounder<>;
    uint32_t sspt_ssv_seq;
    opaque sspt_orig_plain<>;
    opaque sspt_pad<>;
};

struct ssv_seal_cipher_tkn4 {
    uint32_t ssct_ssv_seq;
    opaque ssct_iv<>;
    opaque ssct_encr_data<>;
    opaque ssct_hmac<>;
};

struct authsys_parms {
    unsigned int stamp;
    string machinename<255>;
    unsigned int uid;
    unsigned int gid;
    unsigned int gids<16>;
};

// RFC 5662 section 2: the operations' arguments and results, 4.0's
// (ACCESS to RELEASE_LOCKOWNER) first.
const ACCESS4_READ = 0x00000001;
const ACCESS4_LOOKUP = 0x00000002;
const ACCESS4_MODIFY = 0x00000004;
const ACCESS4_EXTEND = 0x00000008;
const ACCESS4_DELETE = 0x00000010;
const ACCESS4_EXECUTE = 0x00000020;

struct ACCESS4args {
    uint32_t access;
};

struct ACCESS4resok {
    uint32_t supported;
    uint32_t access;
};

union ACCESS4res switch (nfsstat4 status) {
case NFS4_OK:
    ACCESS4resok resok4;
default:
    void;
};

struct CLOSE4args {
    seqid4 seqid;
    stateid4 open_stateid;
};

union CLOSE4res switch (nfsstat4 status) {
case NFS4_OK:
    stateid4 open_stateid;
default:
    void;
};

struct COMMIT4args {
    offset4 offset;
    count4 count;
};

struct COMMIT4resok {
    verifier4 writeverf;
};

union COMMIT4res switch (nfsstat4 status) {
case NFS4_OK:
    COMMIT4resok resok4;
default:
    void;
};

union createtype4 switch (nfs_ftype4 type) {
case NF4LNK:
    linktext4 linkdata;
case NF4BLK:
case NF4CHR:
    specdata4 devdata;
case NF4SOCK:
case NF4FIFO:
case NF4DIR:
    void;
default:
    void;
};

struct CREATE4args {
    createtype4 objtype;
    component4 objname;
    fattr4 createattrs;
};

struct CREATE4resok {
    change_info4 cinfo;
    bitmap4 attrset;
};

union CREATE4res switch (nfsstat4 status) {
case NFS4_OK:
    CREATE4resok resok4;
default:
    void;
};

struct DELEGPURGE4args {
    clientid4 clientid;
};

struct DELEGPURGE4res {
    nfsstat4 status;
};

struct DELEGRETURN4args {
    stateid4 deleg_stateid;
};

struct DELEGRETURN4res {
    nfsstat4 status;
};

struct GETATTR4args {
    bitmap4 attr_request;
};

struct GETATTR4resok {
    fattr4 obj_attributes;
};

union GETATTR4res switch (nfsstat4 status) {
case NFS4_OK:
    GETATTR4resok resok4;
default:
    void;
};

struct GETFH4resok {
    nfs_fh4 object;
};

union GETFH4res switch (nfsstat4 status) {
case NFS4_OK:
    GETFH4resok resok4;
default:
    void;
};

struct LINK4args {
    component4 newname;
};

struct LINK4resok {
    change_info4 cinfo;
};

union LINK4res switch (nfsstat4 status) {
case NFS4_OK:
    LINK4resok resok4;
default:
    void;
};

struct open_to_lock_owner4 {
    seqid4 open_seqid;
    stateid4 open_stateid;
    seqid4 lock_seqid;
    lock_owner4 lock_owner;
};

struct exist_lock_owner4 {
    stateid4 lock_stateid;
    seqid4 lock_seqid;
};

union locker4 switch (bool new_lock_owner) {
case TRUE:
    open_to_lock_owner4 open_owner;
case FALSE:
    exist_lock_owner4 lock_owner;
};

struct LOCK4args {
    nfs_lock_type4 locktype;
    bool reclaim;
    offset4 offset;
    length4 length;
    locker4 locker;
};

struct LOCK4denied {
    offset4 offset;
    length4 length;
    nfs_lock_type4 locktype;
    lock_owner4 owner;
};

struct LOCK4resok {
    stateid4 lock_stateid;
};

union LOCK4res switch (nfsstat4 status) {
case NFS4_OK:
    LOCK4resok resok4;
case NFS4ERR_DENIED:
    LOCK4denied denied;
default:
    void;
};

struct LOCKT4args {
    nfs_lock_type4 locktype;
    offset4 offset;
    length4 length;
    lock_owner4 owner;
};

union LOCKT4res switch (nfsstat4 status) {
case NFS4ERR_DENIED:
    LOCK4denied denied;
case NFS4_OK:
    void;
default:
    void;
};

struct LOCKU4args {
    nfs_lock_type4 locktype;
    seqid4 seqid;
    stateid4 lock_stateid;
    offset4 offset;
    length4 length;
};

union LOCKU4res switch (nfsstat4 status) {
case NFS4_OK:
    stateid4 lock_stateid;
default:
    void;
};

struct LOOKUP4args {
    component4 objname;
};

struct LOOKUP4res {
    nfsstat4 status;
};

struct LOOKUPP4res {
    nfsstat4 status;
};

struct NVERIFY4args {
    fattr4 obj_attributes;
};

struct NVERIFY4res {
    nfsstat4 status;
};

const OPEN4_SHARE_ACCESS_READ = 0x00000001;
const OPEN4_SHARE_ACCESS_WRITE = 0x00000002;
const OPEN4_SHARE_ACCESS_BOTH = 0x00000003;
const OPEN4_SHARE_DENY_NONE = 0x00000000;
const OPEN4_SHARE_DENY_READ = 0x00000001;
const OPEN4_SHARE_DENY_WRITE = 0x00000002;
const OPEN4_SHARE_DENY_BOTH = 0x00000003;
const OPEN4_SHARE_ACCESS_WANT_DELEG_MASK = 0xFF00;
const OPEN4_SHARE_ACCESS_WANT_NO_PREFERENCE = 0x0000;
const OPEN4_SHARE_ACCESS_WANT_READ_DELEG = 0x0100;
const OPEN4_SHARE_ACCESS_WANT_WRITE_DELEG = 0x0200;
const OPEN4_SHARE_ACCESS_WANT_ANY_DELEG = 0x0300;
const OPEN4_SHARE_ACCESS_WANT_NO_DELEG = 0x0400;
const OPEN4_SHARE_ACCESS_WANT_CANCEL = 0x0500;
const OPEN4_SHARE_ACCESS_WANT_SIGNAL_DELEG_WHEN_RESRC_AVAIL = 0x10000;
const OPEN4_SHARE_ACCESS_WANT_PUSH_DELEG_WHEN_UNCONTENDED = 0x20000;

enum createmode4 {
    UNCHECKED4 = 0,
    GUARDED4 = 1,
    EXCLUSIVE4 = 2,
    EXCLUSIVE4_1 = 3
};

struct creatverfattr {
    verifier4 cva_verf;
    fattr4 cva_attrs;
};

union createhow4 switch (createmode4 mode) {
case UNCHECKED4:
case GUARDED4:
    fattr4 createattrs;
case EXCLUSIVE4:
    verifier4 createverf;
case EXCLUSIVE4_1:
    creatverfattr ch_createboth;
};

enum opentype4 {
    OPEN4_NOCREATE = 0,
    OPEN4_CREATE = 1
};

union openflag4 switch (opentype4 opentype) {
case OPEN4_CREATE:
    createhow4 how;
default:
    void;
};

enum limit_by4 {
    NFS_LIMIT_SIZE = 1,
    NFS_LIMIT_BLOCKS = 2
};

struct nfs_modified_limit4 {
    uint32_t num_blocks;
    uint32_t bytes_per_block;
};

union nfs_space_limit4 switch (limit_by4 limitby) {
case NFS_LIMIT_SIZE:
    uint64_t filesize;
case NFS_LIMIT_BLOCKS:
    nfs_modified_limit4 mod_blocks;
};

enum open_delegation_type4 {
    OPEN_DELEGATE_NONE = 0,
    OPEN_DELEGATE_READ = 1,
    OPEN_DELEGATE_WRITE = 2,
    OPEN_DELEGATE_NONE_EXT = 3
};

enum open_claim_type4 {
    CLAIM_NULL = 0,
    CLAIM_PREVIOUS = 1,
    CLAIM_DELEGATE_CUR = 2,
    CLAIM_DELEGATE_PREV = 3,
    CLAIM_FH = 4,
    CLAIM_DELEG_CUR_FH = 5,
    CLAIM_DELEG_PREV_FH = 6
};

struct open_claim_delegate_cur4 {
    stateid4 delegate_stateid;
    component4 file;
};

union open_claim4 switch (open_claim_type4 claim) {
case CLAIM_NULL:
    component4 file;
case CLAIM_PREVIOUS:
    open_delegation_type4 delegate_type;
case CLAIM_DELEGATE_CUR:
    open_claim_delegate_cur4 delegate_cur_info;
case CLAIM_DELEGATE_PREV:
    component4 file_delegate_prev;
case CLAIM_FH:
    void;
case CLAIM_DELEG_PREV_FH:
    void;
case CLAIM_DELEG_CUR_FH:
    stateid4 oc_delegate_stateid;
};

struct OPEN4args {
    seqid4 seqid;
    uint32_t share_access;
    uint32_t share_deny;
    open_owner4 owner;
    openflag4 openhow;
    open_claim4 claim;
};

struct open_read_delegation4 {
    stateid4 stateid;
    bool recall;
    nfsace4 permissions;
};

struct open_write_delegation4 {
    stateid4 stateid;
    bool recall;
    nfs_space_limit4 space_limit;
    nfsace4 permissions;
};

enum why_no_delegation4 {
    WND4_NOT_WANTED = 0,
    WND4_CONTENTION = 1,
    WND4_RESOURCE = 2,
    WND4_NOT_SUPP_FTYPE = 3,
    WND4_WRITE_DELEG_NOT_SUPP_FTYPE = 4,
    WND4_NOT_SUPP_UPGRADE = 5,
    WND4_NOT_SUPP_DOWNGRADE = 6,
    WND4_CANCELLED = 7,
    WND4_IS_DIR = 8
};

union open_none_delegation4 switch (why_no_delegation4 ond_why) {
case WND4_CONTENTION:
    bool ond_server_will_push_deleg;
case WND4_RESOURCE:
    bool ond_server_will_signal_avail;
default:
    void;
};

union open_delegation4 switch (open_delegation_type4 delegation_type) {
case OPEN_DELEGATE_NONE:
    void;
case OPEN_DELEGATE_READ:
    open_read_delegation4 read;
case OPEN_DELEGATE_WRITE:
    open_write_delegation4 write;
case OPEN_DELEGATE_NONE_EXT:
    open_none_delegation4 od_whynone;
};

const OPEN4_RESULT_CONFIRM = 0x00000002;
const OPEN4_RESULT_LOCKTYPE_POSIX = 0x00000004;
const OPEN4_RESULT_PRESERVE_UNLINKED = 0x00000008;
const OPEN4_RESULT_MAY_NOTIFY_LOCK = 0x00000020;

struct OPEN4resok {
    stateid4 stateid;
    change_info4 cinfo;
    uint32_t rflags;
    bitmap4 attrset;
    open_delegation4 delegation;
};

union OPEN4res switch (nfsstat4 status) {
case NFS4_OK:
    OPEN4resok resok4;
default:
    void;
};

struct OPENATTR4args {
    bool createdir;
};

struct OPENATTR4res {
    nfsstat4 status;
};

struct OPEN_CONFIRM4args {
    stateid4 open_stateid;
    seqid4 seqid;
};

struct OPEN_CONFIRM4resok {
    stateid4 open_stateid;
};

union OPEN_CONFIRM4res switch (nfsstat4 status) {
case NFS4_OK:
    OPEN_CONFIRM4resok resok4;
default:
    void;
};

struct OPEN_DOWNGRADE4args {
    stateid4 open_stateid;
    seqid4 seqid;
    uint32_t share_access;
    uint32_t share_deny;
};

struct OPEN_DOWNGRADE4resok {
    stateid4 open_stateid;
};

union OPEN_DOWNGRADE4res switch (nfsstat4 status) {
case NFS4_OK:
    OPEN_DOWNGRADE4resok resok4;
default:
    void;
};

struct PUTFH4args {
    nfs_fh4 object;
};

struct PUTFH4res {
    nfsstat4 status;
};

struct PUTPUBFH4res {
    nfsstat4 status;
};

struct PUTROOTFH4res {
    nfsstat4 status;
};

struct READ4args {
    stateid4 stateid;
    offset4 offset;
    count4 count;
};

struct READ4resok {
    bool eof;
    opaque data<>;
};

union READ4res switch (nfsstat4 status) {
case NFS4_OK:
    READ4resok resok4;
default:
    void;
};

struct READDIR4args {
    nfs_cookie4 cookie;
    verifier4 cookieverf;
    count4 dircount;
    count4 maxcount;
    bitmap4 attr_request;
};

struct entry4 {
    nfs_cookie4 cookie;
    component4 name;
    fattr4 attrs;
    entry4 *nextentry;
};

struct dirlist4 {
    entry4 *entries;
    bool eof;
};

struct READDIR4resok {
    verifier4 cookieverf;
    dirlist4 reply;
};

union READDIR4res switch (nfsstat4 status) {
case NFS4_OK:
    READDIR4resok resok4;
default:
    void;
};

struct READLINK4resok {
    linktext4 link;
};

union READLINK4res switch (nfsstat4 status) {
case NFS4_OK:
    READLINK4resok resok4;
default:
    void;
};

struct REMOVE4args {
    component4 target;
};

struct REMOVE4resok {
    change_info4 cinfo;
};

union REMOVE4res switch (nfsstat4 status) {
case NFS4_OK:
    REMOVE4resok resok4;
default:
    void;
};

struct RENAME4args {
    component4 oldname;
    component4 newname;
};

struct RENAME4resok {
    change_info4 source_cinfo;
    change_info4 target_cinfo;
};

union RENAME4res switch (nfsstat4 status) {
case NFS4_OK:
    RENAME4resok resok4;
default:
    void;
};

struct RENEW4args {
    clientid4 clientid;
};

struct RENEW4res {
    nfsstat4 status;
};

struct RESTOREFH4res {
    nfsstat4 status;
};

struct SAVEFH4res {
    nfsstat4 status;
};

struct SECINFO4args {
    component4 name;
};

enum rpc_gss_svc_t {
    RPC_GSS_SVC_NONE = 1,
    RPC_GSS_SVC_INTEGRITY = 2,
    RPC_GSS_SVC_PRIVACY = 3
};

struct rpcsec_gss_info {
    sec_oid4 oid;
    qop4 qop;
    rpc_gss_svc_t service;
};

union secinfo4 switch (uint32_t flavor) {
case RPCSEC_GSS:
    rpcsec_gss_info flavor_info;
default:
    void;
};

typedef secinfo4 SECINFO4resok<>;

union SECINFO4res switch (nfsstat4 status) {
case NFS4_OK:
    SECINFO4resok resok4;
default:
    void;
};

struct SETATTR4args {
    stateid4 stateid;
    fattr4 obj_attributes;
};

struct SETATTR4res {
    nfsstat4 status;
    bitmap4 attrsset;
};

struct SETCLIENTID4args {
    nfs_client_id4 client;
    cb_client4 callback;
    uint32_t callback_ident;
};

struct SETCLIENTID4resok {
    clientid4 clientid;
    verifier4 setclientid_confirm;
};

union SETCLIENTID4res switch (nfsstat4 status) {
case NFS4_OK:
    SETCLIENTID4resok resok4;
case NFS4ERR_CLID_INUSE:
    clientaddr4 client_using;
default:
    void;
};

struct SETCLIENTID_CONFIRM4args {
    clientid4 clientid;
    verifier4 setclientid_confirm;
};

struct SETCLIENTID_CONFIRM4res {
    nfsstat4 status;
};

struct VERIFY4args {
    fattr4 obj_attributes;
};

struct VERIFY4res {
    nfsstat4 status;
};

enum stable_how4 {
    UNSTABLE4 = 0,
    DATA_SYNC4 = 1,
    FILE_SYNC4 = 2
};

struct WRITE4args {
    stateid4 stateid;
    offset4 offset;
    stable_how4 stable;
    opaque data<>;
};

struct WRITE4resok {
    count4 count;
    stable_how4 committed;
    verifier4 writeverf;
};

union WRITE4res switch (nfsstat4 status) {
case NFS4_OK:
    WRITE4resok resok4;
default:
    void;
};

struct RELEASE_LOCKOWNER4args {
    lock_owner4 lock_owner;
};

struct RELEASE_LOCKOWNER4res {
    nfsstat4 status;
};

struct ILLEGAL4res {
    nfsstat4 status;
};

// 4.1's operations (BACKCHANNEL_CTL to RECLAIM_COMPLETE).
typedef opaque gsshandle4_t<>;

struct gss_cb_handles4 {
    rpc_gss_svc_t gcbp_service;
    gsshandle4_t gcbp_handle_from_server;
    gsshandle4_t gcbp_handle_from_client;
};

union callback_sec_parms4 switch (uint32_t cb_secflavor) {
case AUTH_NONE:
    void;
case AUTH_SYS:
    authsys_parms cbsp_sys_cred;
case RPCSEC_GSS:
    gss_cb_handles4 cbsp_gss_handles;
};

struct BACKCHANNEL_CTL4args {
    uint32_t bca_cb_program;
    callback_sec_parms4 bca_sec_parms<>;
};

struct BACKCHANNEL_CTL4res {
    nfsstat4 bcr_status;
};

enum channel_dir_from_client4 {
    CDFC4_FORE = 0x1,
    CDFC4_BACK = 0x2,
    CDFC4_FORE_OR_BOTH = 0x3,
    CDFC4_BACK_OR_BOTH = 0x7
};

struct BIND_CONN_TO_SESSION4args {
    sessionid4 bctsa_sessid;
    channel_dir_from_client4 bctsa_dir;
    bool bctsa_use_conn_in_rdma_mode;
};

enum channel_dir_from_server4 {
    CDFS4_FORE = 0x1,
    CDFS4_BACK = 0x2,
    CDFS4_BOTH = 0x3
};

struct BIND_CONN_TO_SESSION4resok {
    sessionid4 bctsr_sessid;
    channel_dir_from_server4 bctsr_dir;
    bool bctsr_use_conn_in_rdma_mode;
};

union BIND_CONN_TO_SESSION4res switch (nfsstat4 bctsr_status) {
case NFS4_OK:
    BIND_CONN_TO_SESSION4resok bctsr_resok4;
default:
    void;
};

const EXCHGID4_FLAG_SUPP_MOVED_REFER = 0x00000001;
const EXCHGID4_FLAG_SUPP_MOVED_MIGR = 0x00000002;
const EXCHGID4_FLAG_BIND_PRINC_STATEID = 0x00000100;
const EXCHGID4_FLAG_USE_NON_PNFS = 0x00010000;
const EXCHGID4_FLAG_USE_PNFS_MDS = 0x00020000;
const EXCHGID4_FLAG_USE_PNFS_DS = 0x00040000;
const EXCHGID4_FLAG_MASK_PNFS = 0x00070000;
const EXCHGID4_FLAG_UPD_CONFIRMED_REC_A = 0x40000000;
const EXCHGID4_FLAG_CONFIRMED_R = 0x80000000;

struct state_protect_ops4 {
    bitmap4 spo_must_enforce;
    bitmap4 spo_must_allow;
};

struct ssv_sp_parms4 {
    state_protect_ops4 ssp_ops;
    sec_oid4 ssp_hash_algs<>;
    sec_oid4 ssp_encr_algs<>;
    uint32_t ssp_window;
    uint32_t ssp_num_gss_handles;
};

enum state_protect_how4 {
    SP4_NONE = 0,
    SP4_MACH_CRED = 1,
    SP4_SSV = 2
};

union state_protect4_a switch (state_protect_how4 spa_how) {
case SP4_NONE:
    void;
case SP4_MACH_CRED:
    state_protect_ops4 spa_mach_ops;
case SP4_SSV:
    ssv_sp_parms4 spa_ssv_parms;
};

struct EXCHANGE_ID4args {
    client_owner4 eia_clientowner;
    uint32_t eia_flags;
    state_protect4_a eia_state_protect;
    nfs_impl_id4 eia_client_impl_id<1>;
};

struct ssv_prot_info4 {
    state_protect_ops4 spi_ops;
    uint32_t spi_hash_alg;
    uint32_t spi_encr_alg;
    uint32_t spi_ssv_len;
    uint32_t spi_window;
    gsshandle4_t spi_handles<>;
};

union state_protect4_r switch (state_protect_how4 spr_how) {
case SP4_NONE:
    void;
case SP4_MACH_CRED:
    state_protect_ops4 spr_mach_ops;
case SP4_SSV:
    ssv_prot_info4 spr_ssv_info;
};

struct EXCHANGE_ID4resok {
    clientid4 eir_clientid;
    sequenceid4 eir_sequenceid;
    uint32_t eir_flags;
    state_protect4_r eir_state_protect;
    server_owner4 eir_server_owner;
    opaque eir_server_scope<NFS4_OPAQUE_LIMIT>;
    nfs_impl_id4 eir_server_impl_id<1>;
};

union EXCHANGE_ID4res switch (nfsstat4 eir_status) {
case NFS4_OK:
    EXCHANGE_ID4resok eir_resok4;
default:
    void;
};

struct channel_attrs4 {
    count4 ca_headerpadsize;
    count4 ca_maxrequestsize;
    count4 ca_maxresponsesize;
    count4 ca_maxresponsesize_cached;
    count4 ca_maxoperations;
    count4 ca_maxrequests;
    uint32_t ca_rdma_ird<1>;
};

const CREATE_SESSION4_FLAG_PERSIST = 0x00000001;
const CREATE_SESSION4_FLAG_CONN_BACK_CHAN = 0x00000002;
const CREATE_SESSION4_FLAG_CONN_RDMA = 0x00000004;

struct CREATE_SESSION4args {
    clientid4 csa_clientid;
    sequenceid4 csa_sequence;
    uint32_t csa_flags;
    channel_attrs4 csa_fore_chan_attrs;
    channel_attrs4 csa_back_chan_attrs;
    uint32_t csa_cb_program;
    callback_sec_parms4 csa_sec_parms<>;
};

struct CREATE_SESSION4resok {
    sessionid4 csr_sessionid;
    sequenceid4 csr_sequence;
    uint32_t csr_flags;
    channel_attrs4 csr_fore_chan_attrs;
    channel_attrs4 csr_back_chan_attrs;
};

union CREATE_SESSION4res switch (nfsstat4 csr_status) {
case NFS4_OK:
    CREATE_SESSION4resok csr_resok4;
default:
    void;
};

struct DESTROY_SESSION4args {
    sessionid4 dsa_sessionid;
};

struct DESTROY_SESSION4res {
    nfsstat4 dsr_status;
};

struct FREE_STATEID4args {
    stateid4 fsa_stateid;
};

struct FREE_STATEID4res {
    nfsstat4 fsr_status;
};

typedef nfstime4 attr_notice4;

struct GET_DIR_DELEGATION4args {
    bool gdda_signal_deleg_avail;
    bitmap4 gdda_notification_types;
    attr_notice4 gdda_child_attr_delay;
    attr_notice4 gdda_dir_attr_delay;
    bitmap4 gdda_child_attributes;
    bitmap4 gdda_dir_attributes;
};

struct GET_DIR_DELEGATION4resok {
    verifier4 gddr_cookieverf;
    stateid4 gddr_stateid;
    bitmap4 gddr_notification;
    bitmap4 gddr_child_attributes;
    bitmap4 gddr_dir_attributes;
};

enum gddrnf4_status {
    GDD4_OK = 0,
    GDD4_UNAVAIL = 1
};

union GET_DIR_DELEGATION4res_non_fatal switch (gddrnf4_status gddrnf_status) {
case GDD4_OK:
    GET_DIR_DELEGATION4resok gddrnf_resok4;
case GDD4_UNAVAIL:
    bool gddrnf_will_signal_deleg_avail;
};

union GET_DIR_DELEGATION4res switch (nfsstat4 gddr_status) {
case NFS4_OK:
    GET_DIR_DELEGATION4res_non_fatal gddr_res_non_fatal4;
default:
    void;
};

struct GETDEVICEINFO4args {
    deviceid4 gdia_device_id;
    layouttype4 gdia_layout_type;
    count4 gdia_maxcount;
    bitmap4 gdia_notify_types;
};

struct GETDEVICEINFO4resok {
    device_addr4 gdir_device_addr;
    bitmap4 gdir_notification;
};

union GETDEVICEINFO4res switch (nfsstat4 gdir_status) {
case NFS4_OK:
    GETDEVICEINFO4resok gdir_resok4;
case NFS4ERR_TOOSMALL:
    count4 gdir_mincount;
default:
    void;
};

struct GETDEVICELIST4args {
    layouttype4 gdla_layout_type;
    count4 gdla_maxdevices;
    nfs_cookie4 gdla_cookie;
    verifier4 gdla_cookieverf;
};

struct GETDEVICELIST4resok {
    nfs_cookie4 gdlr_cookie;
    verifier4 gdlr_cookieverf;
    deviceid4 gdlr_deviceid_list<>;
    bool gdlr_eof;
};

union GETDEVICELIST4res switch (nfsstat4 gdlr_status) {
case NFS4_OK:
    GETDEVICELIST4resok gdlr_resok4;
default:
    void;
};

union newtime4 switch (bool nt_timechanged) {
case TRUE:
    nfstime4 nt_time;
case FALSE:
    void;
};

union newoffset4 switch (bool no_newoffset) {
case TRUE:
    offset4 no_offset;
case FALSE:
    void;
};

struct LAYOUTCOMMIT4args {
    offset4 loca_offset;
    length4 loca_length;
    bool loca_reclaim;
    stateid4 loca_stateid;
    newoffset4 loca_last_write_offset;
    newtime4 loca_time_modify;
    layoutupdate4 loca_layoutupdate;
};

union newsize4 switch (bool ns_sizechanged) {
case TRUE:
    length4 ns_size;
case FALSE:
    void;
};

struct LAYOUTCOMMIT4resok {
    newsize4 locr_newsize;
};

union LAYOUTCOMMIT4res switch (nfsstat4 locr_status) {
case NFS4_OK:
    LAYOUTCOMMIT4resok locr_resok4;
default:
    void;
};

struct LAYOUTGET4args {
    bool loga_signal_layout_avail;
    layouttype4 loga_layout_type;
    layoutiomode4 loga_iomode;
    offset4 loga_offset;
    length4 loga_length;
    length4 loga_minlength;
    stateid4 loga_stateid;
    count4 loga_maxcount;
};

struct LAYOUTGET4resok {
    bool logr_return_on_close;
    stateid4 logr_stateid;
    layout4 logr_layout<>;
};

union LAYOUTGET4res switch (nfsstat4 logr_status) {
case NFS4_OK:
    LAYOUTGET4resok logr_resok4;
case NFS4ERR_LAYOUTTRYLATER:
    bool logr_will_signal_layout_avail;
default:
    void;
};

struct LAYOUTRETURN4args {
    bool lora_reclaim;
    layouttype4 lora_layout_type;
    layoutiomode4 lora_iomode;
    layoutreturn4 lora_layoutreturn;
};

union layoutreturn_stateid switch (bool lrs_present) {
case TRUE:
    stateid4 lrs_stateid;
case FALSE:
    void;
};

union LAYOUTRETURN4res switch (nfsstat4 lorr_status) {
case NFS4_OK:
    layoutreturn_stateid lorr_stateid;
default:
    void;
};

enum secinfo_style4 {
    SECINFO_STYLE4_CURRENT_FH = 0,
    SECINFO_STYLE4_PARENT = 1
};

typedef secinfo_style4 SECINFO_NO_NAME4args;
typedef SECINFO4res SECINFO_NO_NAME4res;

struct SEQUENCE4args {
    sessionid4 sa_sessionid;
    sequenceid4 sa_sequenceid;
    slotid4 sa_slotid;
    slotid4 sa_highest_slotid;
    bool sa_cachethis;
};

const SEQ4_STATUS_CB_PATH_DOWN = 0x00000001;
const SEQ4_STATUS_CB_GSS_CONTEXTS_EXPIRING = 0x00000002;
const SEQ4_STATUS_CB_GSS_CONTEXTS_EXPIRED = 0x00000004;
const SEQ4_STATUS_EXPIRED_ALL_STATE_REVOKED = 0x00000008;
const SEQ4_STATUS_EXPIRED_SOME_STATE_REVOKED = 0x00000010;
const SEQ4_STATUS_ADMIN_STATE_REVOKED = 0x00000020;
const SEQ4_STATUS_RECALLABLE_STATE_REVOKED = 0x00000040;
const SEQ4_STATUS_LEASE_MOVED = 0x00000080;
const SEQ4_STATUS_RESTART_RECLAIM_NEEDED = 0x00000100;
const SEQ4_STATUS_CB_PATH_DOWN_SESSION = 0x00000200;
const SEQ4_STATUS_BACKCHANNEL_FAULT = 0x00000400;
const SEQ4_STATUS_DEVID_CHANGED = 0x00000800;
const SEQ4_STATUS_DEVID_DELETED = 0x00001000;

struct SEQUENCE4resok {
    sessionid4 sr_sessionid;
    sequenceid4 sr_sequenceid;
    slotid4 sr_slotid;
    slotid4 sr_highest_slotid;
    slotid4 sr_target_highest_slotid;
    uint32_t sr_status_flags;
};

union SEQUENCE4res switch (nfsstat4 sr_status) {
case NFS4_OK:
    SEQUENCE4resok sr_resok4;
default:
    void;
};

struct ssa_digest_input4 {
    SEQUENCE4args sdi_seqargs;
};

struct SET_SSV4args {
    opaque ssa_ssv<>;
    opaque ssa_digest<>;
};

struct ssr_digest_input4 {
    SEQUENCE4res sdi_seqres;
};

struct SET_SSV4resok {
    opaque ssr_digest<>;
};

union SET_SSV4res switch (nfsstat4 ssr_status) {
case NFS4_OK:
    SET_SSV4resok ssr_resok4;
default:
    void;
};

struct TEST_STATEID4args {
    stateid4 ts_stateids<>;
};

struct TEST_STATEID4resok {
    nfsstat4 tsr_status_codes<>;
};

union TEST_STATEID4res switch (nfsstat4 tsr_status) {
case NFS4_OK:
    TEST_STATEID4resok tsr_resok4;
default:
    void;
};

union deleg_claim4 switch (open_claim_type4 dc_claim) {
case CLAIM_FH:
    void;
case CLAIM_DELEG_PREV_FH:
    void;
case CLAIM_PREVIOUS:
    open_delegation_type4 dc_delegate_type;
};

struct WANT_DELEGATION4args {
    uint32_t wda_want;
    deleg_claim4 wda_claim;
};

union WANT_DELEGATION4res switch (nfsstat4 wdr_status) {
case NFS4_OK:
    open_delegation4 wdr_resok4;
default:
    void;
};

struct DESTROY_CLIENTID4args {
    clientid4 dca_clientid;
};

struct DESTROY_CLIENTID4res {
    nfsstat4 dcr_status;
};

struct RECLAIM_COMPLETE4args {
    bool rca_one_fs;
};

struct RECLAIM_COMPLETE4res {
    nfsstat4 rcr_status;
};

// The compound: operation numbers and the argument and result unions.
enum nfs_opnum4 {
    OP_ACCESS = 3,
    OP_CLOSE = 4,
    OP_COMMIT = 5,
    OP_CREATE = 6,
    OP_DELEGPURGE = 7,
    OP_DELEGRETURN = 8,
    OP_GETATTR = 9,
    OP_GETFH = 10,
    OP_LINK = 11,
    OP_LOCK = 12,
    OP_LOCKT = 13,
    OP_LOCKU = 14,
    OP_LOOKUP = 15,
    OP_LOOKUPP = 16,
    OP_NVERIFY = 17,
    OP_OPEN = 18,
    OP_OPENATTR = 19,
    OP_OPEN_CONFIRM = 20,
    OP_OPEN_DOWNGRADE = 21,
    OP_PUTFH = 22,
    OP_PUTPUBFH = 23,
    OP_PUTROOTFH = 24,
    OP_READ = 25,
    OP_READDIR = 26,
    OP_READLINK = 27,
    OP_REMOVE = 28,
    OP_RENAME = 29,
    OP_RENEW = 30,
    OP_RESTOREFH = 31,
    OP_SAVEFH = 32,
    OP_SECINFO = 33,
    OP_SETATTR = 34,
    OP_SETCLIENTID = 35,
    OP_SETCLIENTID_CONFIRM = 36,
    OP_VERIFY = 37,
    OP_WRITE = 38,
    OP_RELEASE_LOCKOWNER = 39,
    OP_BACKCHANNEL_CTL = 40,
    OP_BIND_CONN_TO_SESSION = 41,
    OP_EXCHANGE_ID = 42,
    OP_CREATE_SESSION = 43,
    OP_DESTROY_SESSION = 44,
    OP_FREE_STATEID = 45,
    OP_GET_DIR_DELEGATION = 46,
    OP_GETDEVICEINFO = 47,
    OP_GETDEVICELIST = 48,
    OP_LAYOUTCOMMIT = 49,
    OP_LAYOUTGET = 50,
    OP_LAYOUTRETURN = 51,
    OP_SECINFO_NO_NAME = 52,
    OP_SEQUENCE = 53,
    OP_SET_SSV = 54,
    OP_TEST_STATEID = 55,
    OP_WANT_DELEGATION = 56,
    OP_DESTROY_CLIENTID = 57,
    OP_RECLAIM_COMPLETE = 58,
    OP_ILLEGAL = 10044
};

union nfs_argop4 switch (nfs_opnum4 argop) {
case OP_ACCESS: ACCESS4args opaccess;
case OP_CLOSE: CLOSE4args opclose;
case OP_COMMIT: COMMIT4args opcommit;
case OP_CREATE: CREATE4args opcreate;
case OP_DELEGPURGE: DELEGPURGE4args opdelegpurge;
case OP_DELEGRETURN: DELEGRETURN4args opdelegreturn;
case OP_GETATTR: GETATTR4args opgetattr;
case OP_GETFH: void;
case OP_LINK: LINK4args oplink;
case OP_LOCK: LOCK4args oplock;
case OP_LOCKT: LOCKT4args oplockt;
case OP_LOCKU: LOCKU4args oplocku;
case OP_LOOKUP: LOOKUP4args oplookup;
case OP_LOOKUPP: void;
case OP_NVERIFY: NVERIFY4args opnverify;
case OP_OPEN: OPEN4args opopen;
case OP_OPENATTR: OPENATTR4args opopenattr;
case OP_OPEN_CONFIRM: OPEN_CONFIRM4args opopen_confirm;
case OP_OPEN_DOWNGRADE: OPEN_DOWNGRADE4args opopen_downgrade;
case OP_PUTFH: PUTFH4args opputfh;
case OP_PUTPUBFH: void;
case OP_PUTROOTFH: void;
case OP_READ: READ4args opread;
case OP_READDIR: READDIR4args opreaddir;
case OP_READLINK: void;
case OP_REMOVE: REMOVE4args opremove;
case OP_RENAME: RENAME4args oprename;
case OP_RENEW: RENEW4args oprenew;
case OP_RESTOREFH: void;
case OP_SAVEFH: void;
case OP_SECINFO: SECINFO4args opsecinfo;
case OP_SETATTR: SETATTR4args opsetattr;
case OP_SETCLIENTID: SETCLIENTID4args opsetclientid;
case OP_SETCLIENTID_CONFIRM: SETCLIENTID_CONFIRM4args opsetclientid_confirm;
case OP_VERIFY: VERIFY4args opverify;
case OP_WRITE: WRITE4args opwrite;
case OP_RELEASE_LOCKOWNER: RELEASE_LOCKOWNER4args oprelease_lockowner;
case OP_BACKCHANNEL_CTL: BACKCHANNEL_CTL4args opbackchannel_ctl;
case OP_BIND_CONN_TO_SESSION: BIND_CONN_TO_SESSION4args opbind_conn_to_session;
case OP_EXCHANGE_ID: EXCHANGE_ID4args opexchange_id;
case OP_CREATE_SESSION: CREATE_SESSION4args opcreate_session;
case OP_DESTROY_SESSION: DESTROY_SESSION4args opdestroy_session;
case OP_FREE_STATEID: FREE_STATEID4args opfree_stateid;
case OP_GET_DIR_DELEGATION: GET_DIR_DELEGATION4args opget_dir_delegation;
case OP_GETDEVICEINFO: GETDEVICEINFO4args opgetdeviceinfo;
case OP_GETDEVICELIST: GETDEVICELIST4args opgetdevicelist;
case OP_LAYOUTCOMMIT: LAYOUTCOMMIT4args oplayoutcommit;
case OP_LAYOUTGET: LAYOUTGET4args oplayoutget;
case OP_LAYOUTRETURN: LAYOUTRETURN4args oplayoutreturn;
case OP_SECINFO_NO_NAME: SECINFO_NO_NAME4args opsecinfo_no_name;
case OP_SEQUENCE: SEQUENCE4args opsequence;
case OP_SET_SSV: SET_SSV4args opset_ssv;
case OP_TEST_STATEID: TEST_STATEID4args optest_stateid;
case OP_WANT_DELEGATION: WANT_DELEGATION4args opwant_delegation;
case OP_DESTROY_CLIENTID: DESTROY_CLIENTID4args opdestroy_clientid;
case OP_RECLAIM_COMPLETE: RECLAIM_COMPLETE4args opreclaim_complete;
case OP_ILLEGAL: void;
};

union nfs_resop4 switch (nfs_opnum4 resop) {
case OP_ACCESS: ACCESS4res opaccess;
case OP_CLOSE: CLOSE4res opclose;
case OP_COMMIT: COMMIT4res opcommit;
case OP_CREATE: CREATE4res opcreate;
case OP_DELEGPURGE: DELEGPURGE4res opdelegpurge;
case OP_DELEGRETURN: DELEGRETURN4res opdelegreturn;
case OP_GETATTR: GETATTR4res opgetattr;
case OP_GETFH: GETFH4res opgetfh;
case OP_LINK: LINK4res oplink;
case OP_LOCK: LOCK4res oplock;
case OP_LOCKT: LOCKT4res oplockt;
case OP_LOCKU: LOCKU4res oplocku;
case OP_LOOKUP: LOOKUP4res oplookup;
case OP_LOOKUPP: LOOKUPP4res oplookupp;
case OP_NVERIFY: NVERIFY4res opnverify;
case OP_OPEN: OPEN4res opopen;
case OP_OPENATTR: OPENATTR4res opopenattr;
case OP_OPEN_CONFIRM: OPEN_CONFIRM4res opopen_confirm;
case OP_OPEN_DOWNGRADE: OPEN_DOWNGRADE4res opopen_downgrade;
case OP_PUTFH: PUTFH4res opputfh;
case OP_PUTPUBFH: PUTPUBFH4res opputpubfh;
case OP_PUTROOTFH: PUTROOTFH4res opputrootfh;
case OP_READ: READ4res opread;
case OP_READDIR: READDIR4res opreaddir;
case OP_READLINK: READLINK4res opreadlink;
case OP_REMOVE: REMOVE4res opremove;
case OP_RENAME: RENAME4res oprename;
case OP_RENEW: RENEW4res oprenew;
case OP_RESTOREFH: RESTOREFH4res oprestorefh;
case OP_SAVEFH: SAVEFH4res opsavefh;
case OP_SECINFO: SECINFO4res opsecinfo;
case OP_SETATTR: SETATTR4res opsetattr;
case OP_SETCLIENTID: SETCLIENTID4res opsetclientid;
case OP_SETCLIENTID_CONFIRM: SETCLIENTID_CONFIRM4res opsetclientid_confirm;
case OP_VERIFY: VERIFY4res opverify;
case OP_WRITE: WRITE4res opwrite;
case OP_RELEASE_LOCKOWNER: RELEASE_LOCKOWNER4res oprelease_lockowner;
case OP_BACKCHANNEL_CTL: BACKCHANNEL_CTL4res opbackchannel_ctl;
case OP_BIND_CONN_TO_SESSION: BIND_CONN_TO_SESSION4res opbind_conn_to_session;
case OP_EXCHANGE_ID: EXCHANGE_ID4res opexchange_id;
case OP_CREATE_SESSION: CREATE_SESSION4res opcreate_session;
case OP_DESTROY_SESSION: DESTROY_SESSION4res opdestroy_session;
case OP_FREE_STATEID: FREE_STATEID4res opfree_stateid;
case OP_GET_DIR_DELEGATION: GET_DIR_DELEGATION4res opget_dir_delegation;
case OP_GETDEVICEINFO: GETDEVICEINFO4res opgetdeviceinfo;
case OP_GETDEVICELIST: GETDEVICELIST4res opgetdevicelist;
case OP_LAYOUTCOMMIT: LAYOUTCOMMIT4res oplayoutcommit;
case OP_LAYOUTGET: LAYOUTGET4res oplayoutget;
case OP_LAYOUTRETURN: LAYOUTRETURN4res oplayoutreturn;
case OP_SECINFO_NO_NAME: SECINFO_NO_NAME4res opsecinfo_no_name;
case OP_SEQUENCE: SEQUENCE4res opsequence;
case OP_SET_SSV: SET_SSV4res opset_ssv;
case OP_TEST_STATEID: TEST_STATEID4res optest_stateid;
case OP_WANT_DELEGATION: WANT_DELEGATION4res opwant_delegation;
case OP_DESTROY_CLIENTID: DESTROY_CLIENTID4res opdestroy_clientid;
case OP_RECLAIM_COMPLETE: RECLAIM_COMPLETE4res opreclaim_complete;
case OP_ILLEGAL: ILLEGAL4res opillegal;
};

struct COMPOUND4args {
    utf8str_cs tag;
    uint32_t minorversion;
    nfs_argop4 argarray<>;
};

struct COMPOUND4res {
    nfsstat4 status;
    utf8str_cs tag;
    nfs_resop4 resarray<>;
};

program NFS4_PROGRAM {
    version NFS_V4 {
        void NFSPROC4_NULL(void) = 0;
        COMPOUND4res NFSPROC4_COMPOUND(COMPOUND4args) = 1;
    } = 4;
} = 100003;

// The callback program's operations (CB_GETATTR, CB_RECALL for 4.0;
// the rest for 4.1).
struct CB_GETATTR4args {
    nfs_fh4 fh;
    bitmap4 attr_request;
};

struct CB_GETATTR4resok {
    fattr4 obj_attributes;
};

union CB_GETATTR4res switch (nfsstat4 status) {
case NFS4_OK:
    CB_GETATTR4resok resok4;
default:
    void;
};

struct CB_RECALL4args {
    stateid4 stateid;
    bool truncate;
    nfs_fh4 fh;
};

struct CB_RECALL4res {
    nfsstat4 status;
};

struct CB_ILLEGAL4res {
    nfsstat4 status;
};

enum layoutrecall_type4 {
    LAYOUTRECALL4_FILE = LAYOUT4_RET_REC_FILE,
    LAYOUTRECALL4_FSID = LAYOUT4_RET_REC_FSID,
    LAYOUTRECALL4_ALL = LAYOUT4_RET_REC_ALL
};

struct layoutrecall_file4 {
    nfs_fh4 lor_fh;
    offset4 lor_offset;
    length4 lor_length;
    stateid4 lor_stateid;
};

union layoutrecall4 switch (layoutrecall_type4 lor_recalltype) {
case LAYOUTRECALL4_FILE:
    layoutrecall_file4 lor_layout;
case LAYOUTRECALL4_FSID:
    fsid4 lor_fsid;
case LAYOUTRECALL4_ALL:
    void;
};

struct CB_LAYOUTRECALL4args {
    layouttype4 clora_type;
    layoutiomode4 clora_iomode;
    bool clora_changed;
    layoutrecall4 clora_recall;
};

struct CB_LAYOUTRECALL4res {
    nfsstat4 clorr_status;
};

enum notify_type4 {
    NOTIFY4_CHANGE_CHILD_ATTRS = 0,
    NOTIFY4_CHANGE_DIR_ATTRS = 1,
    NOTIFY4_REMOVE_ENTRY = 2,
    NOTIFY4_ADD_ENTRY = 3,
    NOTIFY4_RENAME_ENTRY = 4,
    NOTIFY4_CHANGE_COOKIE_VERIFIER = 5
};

struct notify_entry4 {
    component4 ne_file;
    fattr4 ne_attrs;
};

struct prev_entry4 {
    notify_entry4 pe_prev_entry;
    nfs_cookie4 pe_prev_entry_cookie;
};

struct notify_remove4 {
    notify_entry4 nrm_old_entry;
    nfs_cookie4 nrm_old_entry_cookie;
};

struct notify_add4 {
    notify_remove4 nad_old_entry<1>;
    notify_entry4 nad_new_entry;
    nfs_cookie4 nad_new_entry_cookie<1>;
    prev_entry4 nad_prev_entry<1>;
    bool nad_last_entry;
};

struct notify_attr4 {
    notify_entry4 na_changed_entry;
};

struct notify_rename4 {
    notify_remove4 nrn_old_entry;
    notify_add4 nrn_new_entry;
};

struct notify_verifier4 {
    verifier4 nv_old_cookieverf;
    verifier4 nv_new_cookieverf;
};

typedef opaque notifylist4<>;

struct notify4 {
    bitmap4 notify_mask;
    notifylist4 notify_vals;
};

struct CB_NOTIFY4args {
    stateid4 cna_stateid;
    nfs_fh4 cna_fh;
    notify4 cna_changes<>;
};

struct CB_NOTIFY4res {
    nfsstat4 cnr_status;
};

struct CB_PUSH_DELEG4args {
    nfs_fh4 cpda_fh;
    open_delegation4 cpda_delegation;
};

struct CB_PUSH_DELEG4res {
    nfsstat4 cpdr_status;
};

const RCA4_TYPE_MASK_RDATA_DLG = 0;
const RCA4_TYPE_MASK_WDATA_DLG = 1;
const RCA4_TYPE_MASK_DIR_DLG = 2;
const RCA4_TYPE_MASK_FILE_LAYOUT = 3;
const RCA4_TYPE_MASK_BLK_LAYOUT = 4;
const RCA4_TYPE_MASK_OBJ_LAYOUT_MIN = 8;
const RCA4_TYPE_MASK_OBJ_LAYOUT_MAX = 9;
const RCA4_TYPE_MASK_OTHER_LAYOUT_MIN = 12;
const RCA4_TYPE_MASK_OTHER_LAYOUT_MAX = 15;

struct CB_RECALL_ANY4args {
    uint32_t craa_objects_to_keep;
    bitmap4 craa_type_mask;
};

struct CB_RECALL_ANY4res {
    nfsstat4 crar_status;
};

typedef CB_RECALL_ANY4args CB_RECALLABLE_OBJ_AVAIL4args;

struct CB_RECALLABLE_OBJ_AVAIL4res {
    nfsstat4 croa_status;
};

struct CB_RECALL_SLOT4args {
    slotid4 rsa_target_highest_slotid;
};

struct CB_RECALL_SLOT4res {
    nfsstat4 rsr_status;
};

struct referring_call4 {
    sequenceid4 rc_sequenceid;
    slotid4 rc_slotid;
};

struct referring_call_list4 {
    sessionid4 rcl_sessionid;
    referring_call4 rcl_referring_calls<>;
};

struct CB_SEQUENCE4args {
    sessionid4 csa_sessionid;
    sequenceid4 csa_sequenceid;
    slotid4 csa_slotid;
    slotid4 csa_highest_slotid;
    bool csa_cachethis;
    referring_call_list4 csa_referring_call_lists<>;
};

struct CB_SEQUENCE4resok {
    sessionid4 csr_sessionid;
    sequenceid4 csr_sequenceid;
    slotid4 csr_slotid;
    slotid4 csr_highest_slotid;
    slotid4 csr_target_highest_slotid;
};

union CB_SEQUENCE4res switch (nfsstat4 csr_status) {
case NFS4_OK:
    CB_SEQUENCE4resok csr_resok4;
default:
    void;
};

struct CB_WANTS_CANCELLED4args {
    bool cwca_contended_wants_cancelled;
    bool cwca_resourced_wants_cancelled;
};

struct CB_WANTS_CANCELLED4res {
    nfsstat4 cwcr_status;
};

struct CB_NOTIFY_LOCK4args {
    nfs_fh4 cnla_fh;
    lock_owner4 cnla_lock_owner;
};

struct CB_NOTIFY_LOCK4res {
    nfsstat4 cnlr_status;
};

enum notify_deviceid_type4 {
    NOTIFY_DEVICEID4_CHANGE = 1,
    NOTIFY_DEVICEID4_DELETE = 2
};

struct notify_deviceid_delete4 {
    layouttype4 ndd_layouttype;
    deviceid4 ndd_deviceid;
};

struct notify_deviceid_change4 {
    layouttype4 ndc_layouttype;
    deviceid4 ndc_deviceid;
    bool ndc_immediate;
};

struct CB_NOTIFY_DEVICEID4args {
    notify4 cnda_changes<>;
};

struct CB_NOTIFY_DEVICEID4res {
    nfsstat4 cndr_status;
};

enum nfs_cb_opnum4 {
    OP_CB_GETATTR = 3,
    OP_CB_RECALL = 4,
    OP_CB_LAYOUTRECALL = 5,
    OP_CB_NOTIFY = 6,
    OP_CB_PUSH_DELEG = 7,
    OP_CB_RECALL_ANY = 8,
    OP_CB_RECALLABLE_OBJ_AVAIL = 9,
    OP_CB_RECALL_SLOT = 10,
    OP_CB_SEQUENCE = 11,
    OP_CB_WANTS_CANCELLED = 12,
    OP_CB_NOTIFY_LOCK = 13,
    OP_CB_NOTIFY_DEVICEID = 14,
    OP_CB_ILLEGAL = 10044
};

union nfs_cb_argop4 switch (unsigned argop) {
case OP_CB_GETATTR: CB_GETATTR4args opcbgetattr;
case OP_CB_RECALL: CB_RECALL4args opcbrecall;
case OP_CB_LAYOUTRECALL: CB_LAYOUTRECALL4args opcblayoutrecall;
case OP_CB_NOTIFY: CB_NOTIFY4args opcbnotify;
case OP_CB_PUSH_DELEG: CB_PUSH_DELEG4args opcbpush_deleg;
case OP_CB_RECALL_ANY: CB_RECALL_ANY4args opcbrecall_any;
case OP_CB_RECALLABLE_OBJ_AVAIL: CB_RECALLABLE_OBJ_AVAIL4args opcbrecallable_obj_avail;
case OP_CB_RECALL_SLOT: CB_RECALL_SLOT4args opcbrecall_slot;
case OP_CB_SEQUENCE: CB_SEQUENCE4args opcbsequence;
case OP_CB_WANTS_CANCELLED: CB_WANTS_CANCELLED4args opcbwants_cancelled;
case OP_CB_NOTIFY_LOCK: CB_NOTIFY_LOCK4args opcbnotify_lock;
case OP_CB_NOTIFY_DEVICEID: CB_NOTIFY_DEVICEID4args opcbnotify_deviceid;
case OP_CB_ILLEGAL: void;
};

union nfs_cb_resop4 switch (unsigned resop) {
case OP_CB_GETATTR: CB_GETATTR4res opcbgetattr;
case OP_CB_RECALL: CB_RECALL4res opcbrecall;
case OP_CB_LAYOUTRECALL: CB_LAYOUTRECALL4res opcblayoutrecall;
case OP_CB_NOTIFY: CB_NOTIFY4res opcbnotify;
case OP_CB_PUSH_DELEG: CB_PUSH_DELEG4res opcbpush_deleg;
case OP_CB_RECALL_ANY: CB_RECALL_ANY4res opcbrecall_any;
case OP_CB_RECALLABLE_OBJ_AVAIL: CB_RECALLABLE_OBJ_AVAIL4res opcbrecallable_obj_avail;
case OP_CB_RECALL_SLOT: CB_RECALL_SLOT4res opcbrecall_slot;
case OP_CB_SEQUENCE: CB_SEQUENCE4res opcbsequence;
case OP_CB_WANTS_CANCELLED: CB_WANTS_CANCELLED4res opcbwants_cancelled;
case OP_CB_NOTIFY_LOCK: CB_NOTIFY_LOCK4res opcbnotify_lock;
case OP_CB_NOTIFY_DEVICEID: CB_NOTIFY_DEVICEID4res opcbnotify_deviceid;
case OP_CB_ILLEGAL: CB_ILLEGAL4res opcbillegal;
};

struct CB_COMPOUND4args {
    utf8str_cs tag;
    uint32_t minorversion;
    uint32_t callback_ident;
    nfs_cb_argop4 argarray<>;
};

struct CB_COMPOUND4res {
    nfsstat4 status;
    utf8str_cs tag;
    nfs_cb_resop4 resarray<>;
};

program NFS4_CALLBACK {
    version NFS_CB {
        void CB_NULL(void) = 0;
        CB_COMPOUND4res CB_COMPOUND(CB_COMPOUND4args) = 1;
    } = 1;
} = 0x40000000;
