/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKMSG: the Work with Messages display. */
             CMD        PROMPT('Work with Messages') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(WRKMSG)
             PARM       KWD(MSGQ) TYPE(Q1) PROMPT('Message queue')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(MSGTYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *INFO *INQ *COPY) +
                          PMTCTL(*PMTRQS) PROMPT('Message type')
             PARM       KWD(START) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*LAST) VALUES(*LAST *FIRST) +
                          PMTCTL(*PMTRQS) PROMPT('Start position')
             PARM       KWD(SEV) TYPE(*INT2) DFT(0) RANGE(0 99) +
                          PMTCTL(*PMTRQS) PROMPT('Severity code filter')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*PRV) VALUES(*PRV *USRPRF *BASIC +
                          *INTERMED) PMTCTL(*PMTRQS) +
                          PROMPT('Assistance level')
             Q1:        QUAL       TYPE(*NAME) LEN(10) DFT(*WRKUSR) +
                          SPCVAL((*WRKUSR) (*WRKSTN) (*USRPRF) +
                          (*SYSOPR) (*ALLWS))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
