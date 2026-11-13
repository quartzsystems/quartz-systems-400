// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// The binder: RFC 1833 section 2 (rpcbind versions 3 and 4) and
// section 3 (portmap version 2). Strings in procedure signatures are
// named (rpcb_string); lists are written as chains of optionals.

const PMAP_PORT = 111;
const IPPROTO_TCP = 6;
const IPPROTO_UDP = 17;

// Section 3.1: portmap version 2.
struct mapping {
    unsigned int prog;
    unsigned int vers;
    unsigned int prot;
    unsigned int port;
};

struct pmaplist {
    mapping map;
    pmaplist *next;
};

typedef pmaplist *pmaplist_ptr;

struct call_args {
    unsigned int prog;
    unsigned int vers;
    unsigned int proc;
    opaque args<>;
};

struct call_result {
    unsigned int port;
    opaque res<>;
};

// Section 2.1: rpcbind versions 3 and 4.
typedef string rpcb_string<>;

struct rpcb {
    unsigned int r_prog;
    unsigned int r_vers;
    string r_netid<>;
    string r_addr<>;
    string r_owner<>;
};

struct rp__list {
    rpcb rpcb_map;
    rp__list *rpcb_next;
};

typedef rp__list *rpcblist_ptr;

struct rpcb_rmtcallargs {
    unsigned int prog;
    unsigned int vers;
    unsigned int proc;
    opaque args<>;
};

struct rpcb_rmtcallres {
    string addr<>;
    opaque results<>;
};

struct rpcb_entry {
    string r_maddr<>;
    string r_nc_netid<>;
    unsigned int r_nc_semantics;
    string r_nc_protofmly<>;
    string r_nc_proto<>;
};

const NC_TPI_CLTS = 1;
const NC_TPI_COTS = 2;
const NC_TPI_COTS_ORD = 3;
const NC_TPI_RAW = 4;

struct rpcb_entry_list {
    rpcb_entry rpcb_entry_map;
    rpcb_entry_list *rpcb_entry_next;
};

typedef rpcb_entry_list *rpcb_entry_list_ptr;

const rpcb_highproc_2 = RPCBPROC_CALLIT;
const rpcb_highproc_3 = RPCBPROC_TADDR2UADDR;
const rpcb_highproc_4 = RPCBPROC_GETSTAT;

const RPCBSTAT_HIGHPROC = 13;
const RPCBVERS_STAT = 3;
const RPCBVERS_4_STAT = 2;
const RPCBVERS_3_STAT = 1;
const RPCBVERS_2_STAT = 0;

struct rpcbs_addrlist {
    unsigned int prog;
    unsigned int vers;
    int success;
    int failure;
    string netid<>;
    rpcbs_addrlist *next;
};

struct rpcbs_rmtcalllist {
    unsigned int prog;
    unsigned int vers;
    unsigned int proc;
    int success;
    int failure;
    int indirect;
    string netid<>;
    rpcbs_rmtcalllist *next;
};

typedef int rpcbs_proc[RPCBSTAT_HIGHPROC];
typedef rpcbs_addrlist *rpcbs_addrlist_ptr;
typedef rpcbs_rmtcalllist *rpcbs_rmtcalllist_ptr;

struct rpcb_stat {
    rpcbs_proc info;
    int setinfo;
    int unsetinfo;
    rpcbs_addrlist_ptr addrinfo;
    rpcbs_rmtcalllist_ptr rmtinfo;
};

typedef rpcb_stat rpcb_stat_byvers[RPCBVERS_STAT];

struct netbuf {
    unsigned int maxlen;
    opaque buf<>;
};

program PMAP_PROG {
    version PMAP_VERS {
        void PMAPPROC_NULL(void) = 0;
        bool PMAPPROC_SET(mapping) = 1;
        bool PMAPPROC_UNSET(mapping) = 2;
        unsigned int PMAPPROC_GETPORT(mapping) = 3;
        pmaplist_ptr PMAPPROC_DUMP(void) = 4;
        call_result PMAPPROC_CALLIT(call_args) = 5;
    } = 2;
} = 100000;

program RPCBPROG {
    version RPCBVERS {
        void RPCBPROC_NULL(void) = 0;
        bool RPCBPROC_SET(rpcb) = 1;
        bool RPCBPROC_UNSET(rpcb) = 2;
        rpcb_string RPCBPROC_GETADDR(rpcb) = 3;
        rpcblist_ptr RPCBPROC_DUMP(void) = 4;
        rpcb_rmtcallres RPCBPROC_CALLIT(rpcb_rmtcallargs) = 5;
        unsigned int RPCBPROC_GETTIME(void) = 6;
        netbuf RPCBPROC_UADDR2TADDR(rpcb_string) = 7;
        rpcb_string RPCBPROC_TADDR2UADDR(netbuf) = 8;
    } = 3;
    version RPCBVERS4 {
        void RPCBPROC_NULL(void) = 0;
        bool RPCBPROC_SET(rpcb) = 1;
        bool RPCBPROC_UNSET(rpcb) = 2;
        rpcb_string RPCBPROC_GETADDR(rpcb) = 3;
        rpcblist_ptr RPCBPROC_DUMP(void) = 4;
        rpcb_rmtcallres RPCBPROC_BCAST(rpcb_rmtcallargs) = RPCBPROC_CALLIT;
        unsigned int RPCBPROC_GETTIME(void) = 6;
        netbuf RPCBPROC_UADDR2TADDR(rpcb_string) = 7;
        rpcb_string RPCBPROC_TADDR2UADDR(netbuf) = 8;
        rpcb_string RPCBPROC_GETVERSADDR(rpcb) = 9;
        rpcb_rmtcallres RPCBPROC_INDIRECT(rpcb_rmtcallargs) = 10;
        rpcb_entry_list_ptr RPCBPROC_GETADDRLIST(rpcb) = 11;
        rpcb_stat_byvers RPCBPROC_GETSTAT(void) = 12;
    } = 4;
} = 100000;
