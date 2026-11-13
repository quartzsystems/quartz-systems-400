// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// ONC RPC messages: RFC 5531 sections 8 and 9 (the message), 8.2 and
// appendix A (AUTH_SYS). The flavor is an unsigned int with named
// values rather than an enum, so that an unknown flavor decodes and is
// answered AUTH_BADCRED; the wire is the same. The anonymous unions are
// named. The arguments and results follow the header undescribed.

typedef unsigned int auth_flavor;
const AUTH_NONE = 0;
const AUTH_SYS = 1;
const AUTH_SHORT = 2;
const AUTH_DH = 3;
const RPCSEC_GSS = 6;

const MAX_AUTH_BYTES = 400;
const MAX_MACHINE_NAME = 255;
const MAX_GIDS = 16;

struct opaque_auth {
    auth_flavor flavor;
    opaque body<MAX_AUTH_BYTES>;
};

enum msg_type {
    CALL = 0,
    REPLY = 1
};

enum reply_stat {
    MSG_ACCEPTED = 0,
    MSG_DENIED = 1
};

enum accept_stat {
    SUCCESS = 0,
    PROG_UNAVAIL = 1,
    PROG_MISMATCH = 2,
    PROC_UNAVAIL = 3,
    GARBAGE_ARGS = 4,
    SYSTEM_ERR = 5
};

enum reject_stat {
    RPC_MISMATCH = 0,
    AUTH_ERROR = 1
};

enum auth_stat {
    AUTH_OK = 0,
    AUTH_BADCRED = 1,
    AUTH_REJECTEDCRED = 2,
    AUTH_BADVERF = 3,
    AUTH_REJECTEDVERF = 4,
    AUTH_TOOWEAK = 5,
    AUTH_INVALIDRESP = 6,
    AUTH_FAILED = 7,
    AUTH_KERB_GENERIC = 8,
    AUTH_TIMEEXPIRE = 9,
    AUTH_TKT_FILE = 10,
    AUTH_DECODE = 11,
    AUTH_NET_ADDR = 12,
    RPCSEC_GSS_CREDPROBLEM = 13,
    RPCSEC_GSS_CTXPROBLEM = 14
};

struct call_body {
    unsigned int rpcvers;
    unsigned int prog;
    unsigned int vers;
    unsigned int proc;
    opaque_auth cred;
    opaque_auth verf;
};

struct mismatch_info {
    unsigned int low;
    unsigned int high;
};

union accepted_reply_data switch (accept_stat stat) {
case SUCCESS:
    void;
case PROG_MISMATCH:
    mismatch_info mismatch_info;
default:
    void;
};

struct accepted_reply {
    opaque_auth verf;
    accepted_reply_data reply_data;
};

union rejected_reply switch (reject_stat stat) {
case RPC_MISMATCH:
    mismatch_info mismatch_info;
case AUTH_ERROR:
    auth_stat stat;
};

union reply_body switch (reply_stat stat) {
case MSG_ACCEPTED:
    accepted_reply areply;
case MSG_DENIED:
    rejected_reply rreply;
};

union rpc_msg_body switch (msg_type mtype) {
case CALL:
    call_body cbody;
case REPLY:
    reply_body rbody;
};

struct rpc_msg {
    unsigned int xid;
    rpc_msg_body body;
};

struct authsys_parms {
    unsigned int stamp;
    string machinename<MAX_MACHINE_NAME>;
    unsigned int uid;
    unsigned int gid;
    unsigned int gids<MAX_GIDS>;
};
