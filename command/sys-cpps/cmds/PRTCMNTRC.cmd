/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTCMNTRC: a trace's frames, decoded, to a spooled file. */
             CMD        PROMPT('Print Communications Trace') +
                          ALLOW(*ALL) HLPPNLGRP(QHSSCMD) HLPID(PRTCMNTRC)
             PARM       KWD(CFGOBJ) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Configuration object')
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          VALUES(*LIN *NWI *NWS) MIN(1) PROMPT('Type')
             PARM       KWD(CTLD) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Controller')
             PARM       KWD(CODE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*CALC) VALUES(*CALC *EBCDIC *ASCII) +
                          PROMPT('Character code')
             PARM       KWD(FMTTCP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Format TCP/IP data')
             PARM       KWD(TCPIPADR) TYPE(E1) PMTCTL(PTCP) +
                          PROMPT('IP address')
             PARM       KWD(SLTPORT) TYPE(*INT4) DFT(*ALL) RANGE(1 +
                          65535) SPCVAL((*ALL 0)) PMTCTL(PTCP) +
                          PROMPT('IP port number')
             PARM       KWD(FMTBCST) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Format broadcast data')
 E1:         ELEM       TYPE(*CHAR) LEN(45) DFT(*ALL) SPCVAL((*ALL)) +
                          PROMPT('IP address')
             ELEM       TYPE(*CHAR) LEN(45) DFT(*ALL) SPCVAL((*ALL)) +
                          PROMPT('IP address')
 PTCP:       PMTCTL     CTL(FMTTCP) COND((*EQ *YES))
