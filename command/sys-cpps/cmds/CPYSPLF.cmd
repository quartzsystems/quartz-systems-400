/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYSPLF: copy a spooled file into a database file member. */
             CMD        PROMPT('Copy Spooled File') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(CPYSPLF)
             PARM       KWD(FILE) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Spooled file')
             PARM       KWD(TOFILE) TYPE(QF) MIN(1) FILE(*OUT) +
                          PROMPT('To database file')
             PARM       KWD(JOB) TYPE(QJ) DFT(*) SNGVAL((*)) +
                          PROMPT('Job name')
             PARM       KWD(SPLNBR) TYPE(*INT4) DFT(*ONLY) +
                          RANGE(1 999999) SPCVAL((*ONLY 0) (*LAST -1)) +
                          PROMPT('Spooled file number')
             PARM       KWD(JOBSYSNAME) TYPE(*CHAR) LEN(8) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*CURRENT) (*ANY)) +
                          PROMPT('Job system name')
             PARM       KWD(CRTDATE) TYPE(EC) DFT(*ONLY) +
                          SNGVAL((*ONLY) (*LAST)) +
                          PROMPT('Spooled file created')
             PARM       KWD(TOMBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('To member')
             PARM       KWD(MBROPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             PARM       KWD(CTLCHAR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *FCFC *PRTCTL *S36FMT) +
                          PROMPT('Control character')
             PARM       KWD(CHLVAL) TYPE(E1) MAX(12) DFT(*NORMAL) +
                          SNGVAL((*NORMAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Channel values')
             PARM       KWD(WSCST) TYPE(QW) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Workstation customizing')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALL) (*SYSBAS) (*CURASPGRP)) +
                          PMTCTL(*PMTRQS) PROMPT('ASP device')
             QF:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             QJ:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
             QW:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             EC:        ELEM       TYPE(*DATE) PROMPT('Creation date')
                        ELEM       TYPE(*TIME) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*LAST)) PROMPT('Creation time')
             E1:        ELEM       TYPE(*INT2) RANGE(1 12) MIN(1) +
                          PROMPT('Channel number')
                        ELEM       TYPE(*INT2) RANGE(1 255) MAX(9) +
                          PROMPT('Line numbers')
