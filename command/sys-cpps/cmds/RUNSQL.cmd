/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RUNSQL: run one SQL statement given as a string. Parameters, order   */
/* and defaults as pub400's prompt (tools/conformance/golden/sql/).     */
             CMD        PROMPT('Run SQL') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(RUNSQL)
             PARM       KWD(SQL) TYPE(*CHAR) LEN(5000) MIN(1) +
                          CASE(*MIXED) PROMPT('SQL')
             PARM       KWD(COMMIT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CHG) VALUES(*CHG *UR *ALL *RS *CS *NONE +
                          *NC *RR) PROMPT('Commitment control')
             PARM       KWD(NAMING) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*SYS) VALUES(*SYS *SQL) PROMPT('Naming')
             PARM       KWD(DATFMT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *USA *ISO *EUR *JIS *MDY +
                          *DMY *YMD *JUL) PMTCTL(*PMTRQS) +
                          PROMPT('Date format')
             PARM       KWD(DATSEP) TYPE(*CHAR) LEN(1) DFT(*JOB) +
                          SPCVAL((*JOB ' ') (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Date separator character')
             PARM       KWD(TIMFMT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*HMS) VALUES(*HMS *USA *ISO *EUR *JIS) +
                          PMTCTL(*PMTRQS) PROMPT('Time format')
             PARM       KWD(TIMSEP) TYPE(*CHAR) LEN(1) DFT(*JOB) +
                          SPCVAL((*JOB ' ') (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Time separator character')
             PARM       KWD(DFTRDBCOL) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Default collection')
             PARM       KWD(DECMPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *SYSVAL *PERIOD *COMMA) +
                          PMTCTL(*PMTRQS) PROMPT('Decimal point')
             PARM       KWD(SRTSEQ) TYPE(Q1) DFT(*JOB) SNGVAL((*JOB) (*JOBRUN) +
                          (*LANGIDSHR) (*LANGIDUNQ) (*HEX)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(7) DFT(*JOB) +
                          SPCVAL((*JOB) (*JOBRUN)) PMTCTL(*PMTRQS) +
                          PROMPT('Language id')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NOLIST) VALUES(*NOLIST *LIST) +
                          PMTCTL(*PMTRQS) PROMPT('Source listing options')
             PARM       KWD(PRTFILE) TYPE(Q2) PMTCTL(*PMTRQS) PROMPT('Print file')
             PARM       KWD(SECLVLTXT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Second level text')
             PARM       KWD(ALWCPYDTA) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*OPTIMIZE) VALUES(*OPTIMIZE *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Allow copy of data')
             PARM       KWD(ALWBLK) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ALLREAD) VALUES(*ALLREAD *NONE *READ) +
                          PMTCTL(*PMTRQS) PROMPT('Allow blocking')
             PARM       KWD(SQLCURRULE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*DB2) VALUES(*DB2 *STD) PMTCTL(*PMTRQS) +
                          PROMPT('SQL rules')
             PARM       KWD(DECRESULT) TYPE(E1) PMTCTL(*PMTRQS) +
                          PROMPT('Decimal result options')
             PARM       KWD(CONACC) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*DFT) VALUES(*DFT *CURCMT *WAIT) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Concurrent access resolution')
             PARM       KWD(SYSTIME) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('System time sensitive')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) DFT(QSYSPRT)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT2) DFT(31) RANGE(31 63) +
                          PROMPT('Maximum precision')
                        ELEM       TYPE(*INT2) DFT(31) RANGE(0 63) +
                          PROMPT('Maximum scale')
                        ELEM       TYPE(*INT2) DFT(0) RANGE(0 9) +
                          PROMPT('Minimum divide scale')
