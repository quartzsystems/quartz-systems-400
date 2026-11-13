/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRNFSSVR: start the NFS server daemons, one type or all of them. */
             CMD        PROMPT('Start NFS Server') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHP0LCMD) HLPID(STRNFSSVR)
             PARM       KWD(SERVER) TYPE(*INT4) RSTD(*YES) MIN(1) +
                          SPCVAL((*ALL -1) (*RPC -2) (*BIO -3) +
                          (*GSS -8) (*RGY -9) (*SVR -4) (*MNT -5) +
                          (*NSM -6) (*NLM -7)) +
                          CHOICE('*ALL, *RPC, *BIO, *GSS...') +
                          PROMPT('Server daemon')
             PARM       KWD(NBRSVR) TYPE(*INT4) DFT(1) RANGE(1 20) +
                          PMTCTL(PSVR) CHOICE('1-20 server daemons') +
                          PROMPT('Number of server daemons')
             PARM       KWD(NBRBIO) TYPE(*INT4) DFT(1) RANGE(1 20) +
                          PMTCTL(PBIO) CHOICE('1-20 server daemons') +
                          PROMPT('Number of block I/O daemons')
             PARM       KWD(RTVRPCREG) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*NO) SPCVAL((*NO N) (*YES Y)) +
                          PMTCTL(PRPC) PROMPT('Retrieve RPC registration')
             PARM       KWD(STRJOBTIMO) TYPE(*INT4) DFT(30) +
                          RANGE(1 3600) SPCVAL((*NOMAX -1)) +
                          CHOICE('1-3600 seconds') +
                          PROMPT('Timeout for start of daemon')
 PSVR:       PMTCTL     CTL(SERVER) COND((*EQ *SVR) (*EQ *ALL)) +
                          NBRTRUE(*EQ 1)
 PBIO:       PMTCTL     CTL(SERVER) COND((*EQ *BIO) (*EQ *ALL)) +
                          NBRTRUE(*EQ 1)
 PRPC:       PMTCTL     CTL(SERVER) COND((*EQ *RPC) (*EQ *ALL)) +
                          NBRTRUE(*EQ 1)
             DEP        CTL(&NBRSVR) PARM((&SERVER *EQ -4) +
                          (&SERVER *EQ -1)) NBRTRUE(*GT 0) MSGID(CPDA1BB)
             DEP        CTL(&NBRBIO) PARM((&SERVER *EQ -3) +
                          (&SERVER *EQ -1)) NBRTRUE(*GT 0) MSGID(CPDA1BC)
             DEP        CTL(&RTVRPCREG) PARM((&SERVER *EQ -2) +
                          (&SERVER *EQ -1)) NBRTRUE(*GT 0) MSGID(CPDA1C3)
