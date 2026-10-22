/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRSQL: start the interactive SQL session. */
             CMD        PROMPT('Start SQL Interactive Session') MAXPOS(0) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHDBCMD) HLPID(STRSQL)
             PARM       KWD(COMMIT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CHG) VALUES(*CHG *UR *NONE *NC *CS *ALL +
                          *RS *RR) PROMPT('Commitment control')
             PARM       KWD(NAMING) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*SYS) VALUES(*SYS *SQL) PROMPT('Naming')
             PARM       KWD(PROCESS) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*RUN) VALUES(*RUN *SYN) +
                          PROMPT('Statement processing')
             PARM       KWD(LIBOPT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*LIB) VALUES(*LIB *NONE) +
                          PROMPT('List libraries')
             PARM       KWD(DATFMT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *USA *ISO *EUR *JIS *MDY +
                          *DMY *YMD *JUL) PMTCTL(*PMTRQS) +
                          PROMPT('Date format')
             PARM       KWD(DATSEP) TYPE(*CHAR) LEN(1) DFT(*JOB) +
                          SPCVAL((*JOB ' ') (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Date separator')
             PARM       KWD(TIMFMT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*HMS) VALUES(*HMS *USA *ISO *EUR *JIS) +
                          PMTCTL(*PMTRQS) PROMPT('Time format')
             PARM       KWD(TIMSEP) TYPE(*CHAR) LEN(1) DFT(*JOB) +
                          SPCVAL((*JOB ' ') (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Time separator')
             PARM       KWD(DECPNT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *SYSVAL *PERIOD *COMMA) +
                          PMTCTL(*PMTRQS) PROMPT('Decimal point')
             PARM       KWD(PGMLNG) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *C *CBL *PLI *RPG +
                          *FTN) PMTCTL(*PMTRQS) PROMPT('Program language')
             PARM       KWD(SRTSEQ) TYPE(Q1) DFT(*JOB) SNGVAL((*JOB) +
                          (*LANGIDSHR) (*LANGIDUNQ) (*HEX)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(3) DFT(*JOB) +
                          SPCVAL((*JOB) (*JOBRUN)) PMTCTL(*PMTRQS) +
                          PROMPT('Language identifier')
             PARM       KWD(SQLSTRDLM) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*APOSTSQL) VALUES(*APOSTSQL *QUOTESQL) +
                          PMTCTL(*PMTRQS) PROMPT('SQL string delimiter')
             PARM       KWD(ALWCPYDTA) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*OPTIMIZE) VALUES(*OPTIMIZE *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Allow copy of data')
             PARM       KWD(DFTRDBCOL) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Default collection')
             PARM       KWD(SQLCURRULE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*DB2) VALUES(*DB2 *STD) PMTCTL(*PMTRQS) +
                          PROMPT('SQL rules')
             PARM       KWD(DECRESULT) TYPE(E1) PMTCTL(*PMTRQS) +
                          PROMPT('Decimal result options')
             PARM       KWD(DECFLTRND) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*HALFEVEN) VALUES(*HALFEVEN *UP *DOWN +
                          *CEILING *FLOOR *HALFUP *HALFDOWN) +
                          PMTCTL(*PMTRQS) PROMPT('DECFLOAT rounding mode')
             PARM       KWD(CONACC) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*DFT) VALUES(*DFT *CURCMT *WAIT) +
                          PMTCTL(*PMTRQS) PROMPT('Concurrent access')
             PARM       KWD(SYSTIME) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('System time sensitive')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT2) DFT(31) RANGE(31 63) +
                          PROMPT('Maximum precision')
                        ELEM       TYPE(*INT2) DFT(31) RANGE(0 63) +
                          PROMPT('Maximum scale')
                        ELEM       TYPE(*INT2) DFT(0) RANGE(0 63) +
                          PROMPT('Minimum divide scale')
