/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVNETA: return the network attributes into CL variables; each */
/* parameter is the variable that receives one attribute.           */
             CMD        PROMPT('Retrieve Network Attributes') MAXPOS(0) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCFCMD) HLPID(RTVNETA)
             PARM       KWD(SYSNAME) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('System name (8)')
             PARM       KWD(PNDSYSNAME) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('Pending system name (8)')
             PARM       KWD(LCLNETID) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('Local network ID (8)')
             PARM       KWD(LCLCPNAME) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('Local control point (8)')
             PARM       KWD(LCLLOCNAME) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('Default local location (8)')
             PARM       KWD(DFTMODE) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('Default mode (8)')
             PARM       KWD(NODETYPE) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('APPN node type (8)')
             PARM       KWD(DTACPR) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Data compression (10)')
             PARM       KWD(DTACPRINM) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Intermediate compression (10)')
             PARM       KWD(MAXINTSSN) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('Max intermediate sessions (5)')
             PARM       KWD(RAR) TYPE(*DEC) LEN(3 0) RTNVAL(*YES) +
                          PROMPT('Route addition resistance (3)')
             PARM       KWD(NETSERVER) TYPE(*CHAR) LEN(85) RTNVAL(*YES) +
                          PROMPT('Network node servers (85)')
             PARM       KWD(ALRSTS) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Alert status (10)')
             PARM       KWD(ALRPRIFP) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('Alert primary focal point (4)')
             PARM       KWD(ALRDFTFP) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('Alert default focal point (4)')
             PARM       KWD(ALRLOGSTS) TYPE(*CHAR) LEN(6) RTNVAL(*YES) +
                          PROMPT('Alert logging status (6)')
             PARM       KWD(ALRBCKFP) TYPE(*CHAR) LEN(16) RTNVAL(*YES) +
                          PROMPT('Alert backup focal point (16)')
             PARM       KWD(ALRRQSFP) TYPE(*CHAR) LEN(16) RTNVAL(*YES) +
                          PROMPT('Alert focal point to rqs (16)')
             PARM       KWD(ALRCTLD) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Alert controller (10)')
             PARM       KWD(ALRHLDCNT) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('Alert hold count (5 0)')
             PARM       KWD(ALRFTR) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Alert filter (10)')
             PARM       KWD(ALRFTRLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Alert filter library (10)')
             PARM       KWD(MSGQ) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Message queue (10)')
             PARM       KWD(MSGQLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Message queue library (10)')
             PARM       KWD(OUTQ) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Output queue (10)')
             PARM       KWD(OUTQLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Output queue library (10)')
             PARM       KWD(JOBACN) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('Job action (7)')
             PARM       KWD(MAXHOP) TYPE(*DEC) LEN(3 0) RTNVAL(*YES) +
                          PROMPT('Maximum hop count (3 0)')
             PARM       KWD(DDMACC) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('DDM request access (10)')
             PARM       KWD(DDMACCLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('DDM access library (10)')
             PARM       KWD(PCSACC) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Client request access (10)')
             PARM       KWD(PCSACCLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Client access library (10)')
             PARM       KWD(DFTCNNLST) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('Default connection list (10)')
             PARM       KWD(ALWANYNET) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('Allow ANYNET support (4)')
             PARM       KWD(NWSDOMAIN) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('Network server domain (8)')
             PARM       KWD(ALWVRTIP) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('Allow virtual APPN (4)')
             PARM       KWD(ALWHPRTWR) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('Allow HPR transport tower (4)')
             PARM       KWD(VRTAUTODEV) TYPE(*DEC) LEN(3 0) RTNVAL(*YES) +
                          PROMPT('Virtual autocreate devs (3 0)')
             PARM       KWD(HPRPTHTMR) TYPE(*CHAR) LEN(40) RTNVAL(*YES) +
                          PROMPT('HPR path switch timers (40)')
             PARM       KWD(ALWADDCLU) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('Allow add to cluster (7)')
             PARM       KWD(MDMCNTRYID) TYPE(*CHAR) LEN(2) RTNVAL(*YES) +
                          PROMPT('Modem country or region (2)')
