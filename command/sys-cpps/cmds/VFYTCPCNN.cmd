/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* VFYTCPCNN: PING under its other name. */
             CMD        PROMPT('Verify TCP/IP Connection') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTONCMD) HLPID(VFYTCPCNN)
             PARM       KWD(RMTSYS) TYPE(*CHAR) LEN(255) MIN(1) +
                          SPCVAL((*INTNETADR)) PROMPT('Remote system')
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(64) +
                          PMTCTL(PINTNET) +
                          PROMPT('Remote internet address')
             PARM       KWD(ADRVERFMT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*CALC) SPCVAL((*CALC) (*IP4) (*IP6)) +
                          PMTCTL(*PMTRQS) PROMPT('Address version format')
             PARM       KWD(MSGMODE) TYPE(E1) PMTCTL(*PMTRQS) +
                          PROMPT('Message mode')
             PARM       KWD(PKTLEN) TYPE(*INT4) DFT(256) RANGE(8 65500) +
                          PMTCTL(*PMTRQS) PROMPT('Packet length (in bytes)')
             PARM       KWD(NBRPKT) TYPE(*INT4) DFT(5) RANGE(1 999) +
                          PMTCTL(*PMTRQS) PROMPT('Number of packets')
             PARM       KWD(WAITTIME) TYPE(*INT4) DFT(1) RANGE(1 120) +
                          PMTCTL(*PMTRQS) PROMPT('Wait time (in seconds)')
             PARM       KWD(LCLINTNETA) TYPE(*CHAR) LEN(64) DFT(*ANY) +
                          SPCVAL((*ANY)) PMTCTL(*PMTRQS) +
                          PROMPT('Local internet address')
             PARM       KWD(TOS) TYPE(*INT2) RSTD(*YES) DFT(*NORMAL) +
                          SPCVAL((*MINDELAY 2) (*MAXTHRPUT 3) (*MAXRLB +
                          4) (*MINCOST 5) (*NORMAL 6)) PMTCTL(*PMTRQS) +
                          PROMPT('Type of service')
             PARM       KWD(IPTTL) TYPE(*INT4) DFT(*DFT) RANGE(1 255) +
                          SPCVAL((*DFT 0)) PMTCTL(*PMTRQS) +
                          PROMPT('IP time to live (hop limit)')
 E1:         ELEM       TYPE(*CHAR) LEN(10) RSTD(*YES) DFT(*VERBOSE) +
                          VALUES(*VERBOSE *QUIET) +
                          PROMPT('Response message detail')
             ELEM       TYPE(*CHAR) LEN(10) RSTD(*YES) DFT(*COMP) +
                          VALUES(*COMP *ESCAPE) +
                          PROMPT('Summary, if response errors')
 PINTNET:    PMTCTL     CTL(RMTSYS) COND((*EQ *INTNETADR))
