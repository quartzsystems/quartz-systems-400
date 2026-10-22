/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGNETA: change the network attributes; every one defaults to *SAME. */
             CMD        PROMPT('Change Network Attributes') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHCFCMD) HLPID(CHGNETA)
             PARM       KWD(SYSNAME) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('System name')
             PARM       KWD(LCLNETID) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PROMPT('Local network ID')
             PARM       KWD(LCLCPNAME) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Local control point name')
             PARM       KWD(LCLLOCNAME) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Default local location')
             PARM       KWD(DFTMODE) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Default mode')
             PARM       KWD(NODETYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ENDNODE *NETNODE +
                          *LENNODE *BEXNODE) PROMPT('APPN node type')
             PARM       KWD(DTACPR) TYPE(*CHAR) LEN(8) DFT(*SAME) +
                          RANGE(1 2147483647) SPCVAL((*SAME) (*NONE) +
                          (*ALLOW) (*REQUEST) (*REQUIRE)) +
                          PROMPT('Data compression')
             PARM       KWD(DTACPRINM) TYPE(*CHAR) LEN(8) DFT(*SAME) +
                          RANGE(1 2147483647) SPCVAL((*SAME) (*NONE) +
                          (*ALLOW) (*REQUEST) (*REQUIRE)) +
                          PROMPT('Intermediate data compression')
             PARM       KWD(MAXINTSSN) TYPE(*INT2) DFT(*SAME) +
                          RANGE(0 9999) SPCVAL((*SAME -1)) +
                          PROMPT('Max intermediate sessions')
             PARM       KWD(RAR) TYPE(*INT2) DFT(*SAME) RANGE(0 255) +
                          SPCVAL((*SAME -1)) +
                          PROMPT('Route addition resistance')
             PARM       KWD(NETSERVER) TYPE(E1) MAX(5) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Network node servers')
             PARM       KWD(ALRSTS) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ON *OFF *UNATTEND) +
                          PROMPT('Alert status')
             PARM       KWD(ALRPRIFP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Alert primary focal point')
             PARM       KWD(ALRDFTFP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Alert default focal point')
             PARM       KWD(ALRLOGSTS) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *LOCAL *RCV +
                          *ALL) PROMPT('Alert logging status')
             PARM       KWD(ALRBCKFP) TYPE(E1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Alert backup focal point')
             PARM       KWD(ALRRQSFP) TYPE(E1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Alert focal point to request')
             PARM       KWD(ALRCTLD) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) +
                          PROMPT('Alert controller')
             PARM       KWD(ALRHLDCNT) TYPE(*INT2) DFT(*SAME) +
                          RANGE(0 32767) SPCVAL((*SAME -1) (*NOMAX -2)) +
                          PROMPT('Alert hold count')
             PARM       KWD(ALRFTR) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PROMPT('Alert filter')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Message queue')
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Output queue')
             PARM       KWD(JOBACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *REJECT *FILE *SEARCH) +
                          PROMPT('Job action')
             PARM       KWD(MAXHOP) TYPE(*INT2) DFT(*SAME) RANGE(1 255) +
                          SPCVAL((*SAME -1)) PROMPT('Maximum hop count')
             PARM       KWD(DDMACC) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*OBJAUT) (*REJECT)) +
                          PROMPT('DDM request access')
             PARM       KWD(PCSACC) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*OBJAUT) (*REJECT) (*REGFAC)) +
                          PROMPT('Client request access')
             PARM       KWD(DFTCNNLST) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Default ISDN connection list')
             PARM       KWD(ALWANYNET) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Allow ANYNET support')
             PARM       KWD(NWSDOMAIN) TYPE(*CNAME) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE) (*SYSNAME)) +
                          PROMPT('Network server domain')
             PARM       KWD(ALWVRTIP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Allow virtual APPN support')
             PARM       KWD(ALWHPRTWR) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Allow HPR transport tower')
             PARM       KWD(VRTAUTODEV) TYPE(*INT2) DFT(*SAME) +
                          RANGE(0 254) SPCVAL((*SAME -1)) +
                          PROMPT('Virtual autocreate devices')
             PARM       KWD(HPRPTHTMR) TYPE(E2) DFT(*SAME) +
                          SNGVAL((*SAME)) PROMPT('HPR path switch timers')
             PARM       KWD(ALWADDCLU) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *ANY *RQSAUT) +
                          PROMPT('Allow add to cluster')
             PARM       KWD(MDMCNTRYID) TYPE(*CHAR) LEN(2) DFT(*SAME) +
                          SPCVAL((*SAME)) +
                          PROMPT('Modem country or region ID')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CNAME) LEN(8) DFT(*LCLNETID) +
                          SPCVAL((*LCLNETID)) PROMPT('Network ID')
                        ELEM       TYPE(*CNAME) LEN(8) MIN(1) +
                          PROMPT('Control point name')
             E2:        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(1 10000) +
                          SPCVAL((*SAME -1) (*NONE 0)) +
                          PROMPT('Network priority')
                        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(1 10000) +
                          SPCVAL((*SAME -1) (*NONE 0)) +
                          PROMPT('High priority')
                        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(1 10000) +
                          SPCVAL((*SAME -1) (*NONE 0)) +
                          PROMPT('Medium priority')
                        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(1 10000) +
                          SPCVAL((*SAME -1) (*NONE 0)) +
                          PROMPT('Low priority')
