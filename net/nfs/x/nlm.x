// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// The network lock manager: the Open Group's XNFS, chapter 10 (versions
// 1 and 3, the 32-bit forms) and chapter 14 (version 4, 64-bit offsets),
// with the nlm_prot interface's names. Version 4's shares use the
// version 1 share mode and access enums (the same values on the wire).

const LM_MAXSTRLEN = 1024;
const MAXNAMELEN = 1025;
const MAXNETOBJ_SZ = 1024;

typedef opaque netobj<MAXNETOBJ_SZ>;
typedef int int32;
typedef unsigned int uint32;
typedef hyper int64;
typedef unsigned hyper uint64;

enum fsh_mode {
    fsm_DN = 0,
    fsm_DR = 1,
    fsm_DW = 2,
    fsm_DRW = 3
};

enum fsh_access {
    fsa_NONE = 0,
    fsa_R = 1,
    fsa_W = 2,
    fsa_RW = 3
};

// Versions 1 and 3.
enum nlm_stats {
    nlm_granted = 0,
    nlm_denied = 1,
    nlm_denied_nolocks = 2,
    nlm_blocked = 3,
    nlm_denied_grace_period = 4,
    nlm_deadlck = 5
};

struct nlm_stat {
    nlm_stats stat;
};

struct nlm_res {
    netobj cookie;
    nlm_stat stat;
};

struct nlm_holder {
    bool exclusive;
    int svid;
    netobj oh;
    unsigned int l_offset;
    unsigned int l_len;
};

union nlm_testrply switch (nlm_stats stat) {
case nlm_denied:
    nlm_holder holder;
default:
    void;
};

struct nlm_testres {
    netobj cookie;
    nlm_testrply test_stat;
};

struct nlm_lock {
    string caller_name<LM_MAXSTRLEN>;
    netobj fh;
    netobj oh;
    int svid;
    unsigned int l_offset;
    unsigned int l_len;
};

struct nlm_lockargs {
    netobj cookie;
    bool block;
    bool exclusive;
    nlm_lock alock;
    bool reclaim;
    int state;
};

struct nlm_cancargs {
    netobj cookie;
    bool block;
    bool exclusive;
    nlm_lock alock;
};

struct nlm_testargs {
    netobj cookie;
    bool exclusive;
    nlm_lock alock;
};

struct nlm_unlockargs {
    netobj cookie;
    nlm_lock alock;
};

struct nlm_share {
    string caller_name<LM_MAXSTRLEN>;
    netobj fh;
    netobj oh;
    fsh_mode mode;
    fsh_access access;
};

struct nlm_shareargs {
    netobj cookie;
    nlm_share share;
    bool reclaim;
};

struct nlm_shareres {
    netobj cookie;
    nlm_stats stat;
    int sequence;
};

struct nlm_notify {
    string name<MAXNAMELEN>;
    int state;
};

// What the status monitor sends back when a monitored host restarts.
struct nlm_sm_status {
    string mon_name<LM_MAXSTRLEN>;
    int state;
    opaque priv[16];
};

// Version 4.
enum nlm4_stats {
    NLM4_GRANTED = 0,
    NLM4_DENIED = 1,
    NLM4_DENIED_NOLOCKS = 2,
    NLM4_BLOCKED = 3,
    NLM4_DENIED_GRACE_PERIOD = 4,
    NLM4_DEADLCK = 5,
    NLM4_ROFS = 6,
    NLM4_STALE_FH = 7,
    NLM4_FBIG = 8,
    NLM4_FAILED = 9
};

struct nlm4_stat {
    nlm4_stats stat;
};

struct nlm4_res {
    netobj cookie;
    nlm4_stat stat;
};

struct nlm4_holder {
    bool exclusive;
    int32 svid;
    netobj oh;
    uint64 l_offset;
    uint64 l_len;
};

union nlm4_testrply switch (nlm4_stats stat) {
case NLM4_DENIED:
    nlm4_holder holder;
default:
    void;
};

struct nlm4_testres {
    netobj cookie;
    nlm4_testrply test_stat;
};

struct nlm4_lock {
    string caller_name<LM_MAXSTRLEN>;
    netobj fh;
    netobj oh;
    int32 svid;
    uint64 l_offset;
    uint64 l_len;
};

struct nlm4_lockargs {
    netobj cookie;
    bool block;
    bool exclusive;
    nlm4_lock alock;
    bool reclaim;
    int32 state;
};

struct nlm4_cancargs {
    netobj cookie;
    bool block;
    bool exclusive;
    nlm4_lock alock;
};

struct nlm4_testargs {
    netobj cookie;
    bool exclusive;
    nlm4_lock alock;
};

struct nlm4_unlockargs {
    netobj cookie;
    nlm4_lock alock;
};

struct nlm4_share {
    string caller_name<LM_MAXSTRLEN>;
    netobj fh;
    netobj oh;
    fsh_mode mode;
    fsh_access access;
};

struct nlm4_shareargs {
    netobj cookie;
    nlm4_share share;
    bool reclaim;
};

struct nlm4_shareres {
    netobj cookie;
    nlm4_stats stat;
    int32 sequence;
};

struct nlm4_notify {
    string name<MAXNAMELEN>;
    int32 state;
};

program NLM_PROG {
    version NLM_SM {
        void NLM_SM_NOTIFY(nlm_sm_status) = 1;
    } = 2;
    version NLM_VERS {
        void NLM_NULL(void) = 0;
        nlm_testres NLM_TEST(nlm_testargs) = 1;
        nlm_res NLM_LOCK(nlm_lockargs) = 2;
        nlm_res NLM_CANCEL(nlm_cancargs) = 3;
        nlm_res NLM_UNLOCK(nlm_unlockargs) = 4;
        nlm_res NLM_GRANTED(nlm_testargs) = 5;
        void NLM_TEST_MSG(nlm_testargs) = 6;
        void NLM_LOCK_MSG(nlm_lockargs) = 7;
        void NLM_CANCEL_MSG(nlm_cancargs) = 8;
        void NLM_UNLOCK_MSG(nlm_unlockargs) = 9;
        void NLM_GRANTED_MSG(nlm_testargs) = 10;
        void NLM_TEST_RES(nlm_testres) = 11;
        void NLM_LOCK_RES(nlm_res) = 12;
        void NLM_CANCEL_RES(nlm_res) = 13;
        void NLM_UNLOCK_RES(nlm_res) = 14;
        void NLM_GRANTED_RES(nlm_res) = 15;
    } = 1;
    version NLM_VERSX {
        void NLM_NULL(void) = 0;
        nlm_testres NLM_TEST(nlm_testargs) = 1;
        nlm_res NLM_LOCK(nlm_lockargs) = 2;
        nlm_res NLM_CANCEL(nlm_cancargs) = 3;
        nlm_res NLM_UNLOCK(nlm_unlockargs) = 4;
        nlm_res NLM_GRANTED(nlm_testargs) = 5;
        void NLM_TEST_MSG(nlm_testargs) = 6;
        void NLM_LOCK_MSG(nlm_lockargs) = 7;
        void NLM_CANCEL_MSG(nlm_cancargs) = 8;
        void NLM_UNLOCK_MSG(nlm_unlockargs) = 9;
        void NLM_GRANTED_MSG(nlm_testargs) = 10;
        void NLM_TEST_RES(nlm_testres) = 11;
        void NLM_LOCK_RES(nlm_res) = 12;
        void NLM_CANCEL_RES(nlm_res) = 13;
        void NLM_UNLOCK_RES(nlm_res) = 14;
        void NLM_GRANTED_RES(nlm_res) = 15;
        nlm_shareres NLM_SHARE(nlm_shareargs) = 20;
        nlm_shareres NLM_UNSHARE(nlm_shareargs) = 21;
        nlm_res NLM_NM_LOCK(nlm_lockargs) = 22;
        void NLM_FREE_ALL(nlm_notify) = 23;
    } = 3;
    version NLM4_VERS {
        void NLMPROC4_NULL(void) = 0;
        nlm4_testres NLMPROC4_TEST(nlm4_testargs) = 1;
        nlm4_res NLMPROC4_LOCK(nlm4_lockargs) = 2;
        nlm4_res NLMPROC4_CANCEL(nlm4_cancargs) = 3;
        nlm4_res NLMPROC4_UNLOCK(nlm4_unlockargs) = 4;
        nlm4_res NLMPROC4_GRANTED(nlm4_testargs) = 5;
        void NLMPROC4_TEST_MSG(nlm4_testargs) = 6;
        void NLMPROC4_LOCK_MSG(nlm4_lockargs) = 7;
        void NLMPROC4_CANCEL_MSG(nlm4_cancargs) = 8;
        void NLMPROC4_UNLOCK_MSG(nlm4_unlockargs) = 9;
        void NLMPROC4_GRANTED_MSG(nlm4_testargs) = 10;
        void NLMPROC4_TEST_RES(nlm4_testres) = 11;
        void NLMPROC4_LOCK_RES(nlm4_res) = 12;
        void NLMPROC4_CANCEL_RES(nlm4_res) = 13;
        void NLMPROC4_UNLOCK_RES(nlm4_res) = 14;
        void NLMPROC4_GRANTED_RES(nlm4_res) = 15;
        nlm4_shareres NLMPROC4_SHARE(nlm4_shareargs) = 20;
        nlm4_shareres NLMPROC4_UNSHARE(nlm4_shareargs) = 21;
        nlm4_res NLMPROC4_NM_LOCK(nlm4_lockargs) = 22;
        void NLMPROC4_FREE_ALL(nlm4_notify) = 23;
    } = 4;
} = 100021;
