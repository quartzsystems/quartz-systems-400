/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTCPRTE: change a route. */
             CMD        PROMPT('Change TCP/IP Route') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(CHGTCPRTE)
             PARM       KWD(RTEDEST) TYPE(*CHAR) LEN(45) +
                          SPCVAL((*DFTROUTE) (*DFTMCAST) (*DFT6ROUTE +
                          '::') (*DFT6MCAST 'FF00::')) MIN(1) +
                          PROMPT('Destination')
             PARM       KWD(SUBNETMASK) TYPE(*CHAR) LEN(15) DFT(*NONE) +
                          SPCVAL((*NONE) (*HOST '255.255.255.255')) +
                          PROMPT('Subnet mask')
             PARM       KWD(TOS) TYPE(*INT2) RSTD(*YES) DFT(*NORMAL) +
                          SPCVAL((*MINDELAY 2) (*MAXTHRPUT 3) (*MAXRLB +
                          4) (*MINCOST 5) (*NORMAL 6)) +
                          PROMPT('Type of service')
             PARM       KWD(NEXTHOP) TYPE(*CHAR) LEN(45) +
                          PROMPT('Next hop')
             PARM       KWD(ADRPFXLEN) TYPE(*INT4) DFT(64) RANGE(1 128) +
                          SPCVAL((*HOST 128) (*NONE 0)) +
                          PROMPT('Address prefix length')
             PARM       KWD(BINDIFC) TYPE(*CHAR) LEN(15) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Bind to interface')
             PARM       KWD(BINDLIND) TYPE(E1) PROMPT('Bind to line')
             PARM       KWD(MTU) TYPE(*INT4) DFT(*SAME) RANGE(576 +
                          16388) SPCVAL((*SAME -1) (*IFC 0)) +
                          PROMPT('Largest datagram (MTU)')
             PARM       KWD(METRIC) TYPE(*INT2) DFT(*SAME) RANGE(1 16) +
                          SPCVAL((*SAME -1)) PROMPT('Route metric')
             PARM       KWD(REDST) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) SPCVAL((*SAME) (*YES) (*NO)) +
                          PROMPT('Redistribute the route')
             PARM       KWD(DUPRTEPTY) TYPE(*INT2) DFT(*SAME) RANGE(1 +
                          10) SPCVAL((*SAME -1) (*HIGH 10) (*MEDIUM 5) +
                          (*LOW 1)) PROMPT('Priority among duplicates')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PROMPT('Text')
             E1:        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Line description')
                        ELEM       TYPE(*INT4) DFT(*NONE) RANGE(1 4094) +
                          SPCVAL((*NONE 0)) PROMPT('VLAN identifier')
