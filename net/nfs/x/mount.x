// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// The MOUNT protocol: version 3 after RFC 1813 appendix I; version 1
// (its fhstatus and the procedures showmount uses) after RFC 1094
// appendix A.

const MNTPATHLEN = 1024;
const MNTNAMLEN = 255;
const FHSIZE3 = 64;
const FHSIZE = 32;

typedef opaque fhandle3<FHSIZE3>;
typedef opaque fhandle[FHSIZE];
typedef string dirpath<MNTPATHLEN>;
typedef string name<MNTNAMLEN>;

enum mountstat3 {
    MNT3_OK = 0,
    MNT3ERR_PERM = 1,
    MNT3ERR_NOENT = 2,
    MNT3ERR_IO = 5,
    MNT3ERR_ACCES = 13,
    MNT3ERR_NOTDIR = 20,
    MNT3ERR_INVAL = 22,
    MNT3ERR_NAMETOOLONG = 63,
    MNT3ERR_NOTSUPP = 10004,
    MNT3ERR_SERVERFAULT = 10006
};

struct mountres3_ok {
    fhandle3 fhandle;
    int auth_flavors<>;
};

union mountres3 switch (mountstat3 fhs_status) {
case MNT3_OK:
    mountres3_ok mountinfo;
default:
    void;
};

// Version 1's answer to MNT: 0 and a handle, or an errno.
union fhstatus switch (unsigned int fhs_status) {
case 0:
    fhandle fhs_fhandle;
default:
    void;
};

typedef struct mountbody *mountlist;

struct mountbody {
    name ml_hostname;
    dirpath ml_directory;
    mountlist ml_next;
};

typedef struct groupnode *groups;

struct groupnode {
    name gr_name;
    groups gr_next;
};

typedef struct exportnode *exports;

struct exportnode {
    dirpath ex_dir;
    groups ex_groups;
    exports ex_next;
};

program MOUNT_PROGRAM {
    version MOUNT_V1 {
        void MOUNTPROC_NULL(void) = 0;
        fhstatus MOUNTPROC_MNT(dirpath) = 1;
        mountlist MOUNTPROC_DUMP(void) = 2;
        void MOUNTPROC_UMNT(dirpath) = 3;
        void MOUNTPROC_UMNTALL(void) = 4;
        exports MOUNTPROC_EXPORT(void) = 5;
        exports MOUNTPROC_EXPORTALL(void) = 6;
    } = 1;
    version MOUNT_V3 {
        void MOUNTPROC3_NULL(void) = 0;
        mountres3 MOUNTPROC3_MNT(dirpath) = 1;
        mountlist MOUNTPROC3_DUMP(void) = 2;
        void MOUNTPROC3_UMNT(dirpath) = 3;
        void MOUNTPROC3_UMNTALL(void) = 4;
        exports MOUNTPROC3_EXPORT(void) = 5;
    } = 3;
} = 100005;
