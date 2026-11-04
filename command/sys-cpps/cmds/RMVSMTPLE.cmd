/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVSMTPLE: remove an SMTP list entry. Keywords, types and values are  */
/* the oracle's (V7R5M0); only *HOSTAUTH entries are kept here.          */
             CMD        PROMPT('Remove SMTP List Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTMCMD) HLPID(RMVSMTPLE)
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(11) MIN(1) RSTD(*YES) +
                          VALUES(*ACCEPT *REJECT *NEAR *DOMAIN *SVRBIND +
                          *CLTBIND *ETRNSVR *FTRSUBJECT *FTRORIGIN +
                          *FTRFILNAME *FTRTYPE *HOSTAUTH *ACCEPTIP6 +
                          *REJECTIP6 *DOMAINIP6 *SVRBNDIP6 *CLTBNDIP6) +
                          EXPR(*YES) PROMPT('Type')
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(15) EXPR(*YES) +
                          PMTCTL(PADR) PROMPT('Internet address')
             PARM       KWD(SUBNETMASK) TYPE(*CHAR) LEN(15) +
                          DFT('255.255.255.255') EXPR(*YES) +
                          PMTCTL(PADR) PROMPT('Subnet mask')
             PARM       KWD(HOSTNAME) TYPE(*CHAR) LEN(255) EXPR(*YES) +
                          CASE(*MIXED) PMTCTL(PHOST) PROMPT('Host name')
             PARM       KWD(FTRDATA) TYPE(*CHAR) LEN(512) EXPR(*YES) +
                          CASE(*MIXED) PMTCTL(PFTR) PROMPT('Filter data')
             PARM       KWD(IP6PFX) TYPE(*CHAR) LEN(44) EXPR(*YES) +
                          PMTCTL(PIP6) PROMPT('IPv6 prefix')
             PARM       KWD(IP6NETADR) TYPE(*CHAR) LEN(40) EXPR(*YES) +
                          PMTCTL(PIP6) PROMPT('IPv6 internet address')
 PADR:       PMTCTL     CTL(TYPE) COND((*EQ *ACCEPT) (*EQ *REJECT) +
                          (*EQ *NEAR) (*EQ *SVRBIND) (*EQ *CLTBIND)) +
                          NBRTRUE((*EQ 1))
 PHOST:      PMTCTL     CTL(TYPE) COND((*EQ *DOMAIN) (*EQ *ETRNSVR) +
                          (*EQ *HOSTAUTH) (*EQ *DOMAINIP6)) +
                          NBRTRUE((*EQ 1))
 PFTR:       PMTCTL     CTL(TYPE) COND((*EQ *FTRSUBJECT) +
                          (*EQ *FTRORIGIN) (*EQ *FTRFILNAME) +
                          (*EQ *FTRTYPE)) NBRTRUE((*EQ 1))
 PIP6:       PMTCTL     CTL(TYPE) COND((*EQ *ACCEPTIP6) +
                          (*EQ *REJECTIP6) (*EQ *SVRBNDIP6) +
                          (*EQ *CLTBNDIP6)) NBRTRUE((*EQ 1))
