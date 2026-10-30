/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* TRCTCPRTE: TRACEROUTE under its other name. */
             CMD        PROMPT('Trace TCP/IP Route') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTONCMD) HLPID(TRCTCPRTE)
             PARM       KWD(RMTSYS) TYPE(*CHAR) LEN(255) MIN(1) +
                          PROMPT('Remote system')
             PARM       KWD(RANGE) TYPE(E1) PROMPT('Range of hops to probe')
             PARM       KWD(PROBES) TYPE(*INT4) DFT(3) RANGE(1 64) +
                          PROMPT('Probes sent per hop')
             PARM       KWD(WAITTIME) TYPE(*INT4) DFT(3) RANGE(1 120) +
                          PROMPT('Response wait time')
             PARM       KWD(PKTLEN) TYPE(*INT4) DFT(40) RANGE(40 65535) +
                          PROMPT('Packet length (in bytes)')
             PARM       KWD(OUTPUT) TYPE(*INT4) RSTD(*YES) DFT(*MSG) +
                          SPCVAL((*MSG 0) (*VERBOSE 1) (*DTAQ -1)) +
                          PROMPT('Output')
             PARM       KWD(DTAQ) TYPE(Q1) PMTCTL(PDTAQ) +
                          PROMPT('Data queue')
             PARM       KWD(ADRVERFMT) TYPE(*INT4) RSTD(*YES) DFT(*CALC) +
                          SPCVAL((*CALC 1) (*IP4 4) (*IP6 6)) +
                          PMTCTL(*PMTRQS) PROMPT('Address version format')
             PARM       KWD(LCLINTNETA) TYPE(*CHAR) LEN(64) DFT(*ANY) +
                          SPCVAL((*ANY)) PMTCTL(*PMTRQS) +
                          PROMPT('Source IP address')
             PARM       KWD(RMTPORT) TYPE(*INT4) DFT(33434) RANGE(1 +
                          65535) PMTCTL(*PMTRQS) PROMPT('Base remote port')
             PARM       KWD(NAMELOOKUP) TYPE(*INT4) RSTD(*YES) DFT(*YES) +
                          SPCVAL((*YES 1) (*NO 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Lookup host names')
             PARM       KWD(PROBEPCL) TYPE(*INT4) RSTD(*YES) DFT(*ICMP) +
                          SPCVAL((*ICMP 1) (*UDP 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Probing protocol')
             PARM       KWD(FRAGMENT) TYPE(*INT4) RSTD(*YES) DFT(*TCPA) +
                          SPCVAL((*TCPA 0) (*NO 3) (*YES 1)) +
                          PMTCTL(*PMTRQS) PROMPT('Allow fragmentation')
 E1:         ELEM       TYPE(*INT2) DFT(1) RANGE(1 255) +
                          PROMPT('Starting probe TTL (hop limit)')
             ELEM       TYPE(*INT2) DFT(30) RANGE(1 255) +
                          PROMPT('Maximum probe TTL (hop limit)')
 Q1:         QUAL       TYPE(*NAME) LEN(10) MIN(1)
             QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*CURLIB) (*LIBL)) PROMPT('Library')
 PDTAQ:      PMTCTL     CTL(OUTPUT) COND((*EQ -1))
