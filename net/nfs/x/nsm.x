// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

// The network status monitor, version 1: the Open Group's XNFS,
// chapter 11 (the protocol) and its sm_inter interface. `status` is
// what the monitor sends to the program a monitored host named.

const SM_MAXSTRLEN = 1024;

struct sm_name {
    string mon_name<SM_MAXSTRLEN>;
};

struct my_id {
    string my_name<SM_MAXSTRLEN>;
    int my_prog;
    int my_vers;
    int my_proc;
};

struct mon_id {
    string mon_name<SM_MAXSTRLEN>;
    my_id my_id;
};

struct mon {
    mon_id mon_id;
    opaque priv[16];
};

struct stat_chge {
    string mon_name<SM_MAXSTRLEN>;
    int state;
};

struct status {
    string mon_name<SM_MAXSTRLEN>;
    int state;
    opaque priv[16];
};

enum res {
    stat_succ = 0,
    stat_fail = 1
};

struct sm_stat_res {
    res res_stat;
    int state;
};

struct sm_stat {
    int state;
};

program SM_PROG {
    version SM_VERS {
        void SM_NULL(void) = 0;
        sm_stat_res SM_STAT(sm_name) = 1;
        sm_stat_res SM_MON(mon) = 2;
        sm_stat SM_UNMON(mon_id) = 3;
        sm_stat SM_UNMON_ALL(my_id) = 4;
        void SM_SIMU_CRASH(void) = 5;
        void SM_NOTIFY(stat_chge) = 6;
    } = 1;
} = 100024;
