/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RUNSQLSTM: run the SQL statements of a source member or stream file. */
             CMD        PROMPT('Run SQL Statements') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(RUNSQLSTM)
             PARM       KWD(SRCFILE) TYPE(Q1) PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) +
                          PROMPT('Source member')
             PARM       KWD(SRCSTMF) TYPE(*PNAME) LEN(5000) +
                          PROMPT('Source stream file')
             PARM       KWD(COMMIT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CHG) VALUES(*CHG *UR *CS *ALL *RS *NONE +
                          *NC *RR) PROMPT('Commitment control')
             PARM       KWD(NAMING) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*SYS) VALUES(*SYS *SQL) +
                          PROMPT('Naming')
             PARM       KWD(ERRLVL) TYPE(*INT2) DFT(10) RANGE(0 40) +
                          PROMPT('Severity level')
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
             PARM       KWD(DFTRDBCOL) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Default collection')
             PARM       KWD(DECMPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *SYSVAL *PERIOD *COMMA) +
                          PMTCTL(*PMTRQS) PROMPT('Decimal point')
             PARM       KWD(SRTSEQ) TYPE(Q2) DFT(*JOB) SNGVAL((*JOB) +
                          (*LANGIDSHR) (*LANGIDUNQ) (*HEX)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(3) DFT(*JOB) +
                          SPCVAL((*JOB) (*JOBRUN)) PMTCTL(*PMTRQS) +
                          PROMPT('Language identifier')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *PRINT) +
                          PMTCTL(*PMTRQS) PROMPT('Listing output')
             PARM       KWD(MARGINS) TYPE(*DEC) LEN(3) DFT(*SRCFILE) +
                          RANGE(1 240) SPCVAL((*SRCFILE 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Source margins')
             PARM       KWD(SECLVLTXT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Second level text')
             PARM       KWD(PROCESS) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*RUN) VALUES(*RUN *SYN) PMTCTL(*PMTRQS) +
                          PROMPT('Statement processing')
             PARM       KWD(DLYPRP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Delay PREPARE')
             PARM       KWD(ALWCPYDTA) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*OPTIMIZE) VALUES(*OPTIMIZE *YES *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Allow copy of data')
             PARM       KWD(ALWBLK) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALLREAD) VALUES(*ALLREAD *NONE *READ) +
                          PMTCTL(*PMTRQS) PROMPT('Allow blocking')
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
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NOLIST) VALUES(*NOLIST *LIST) +
                          PMTCTL(*PMTRQS) PROMPT('Listing option')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(7) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PMTCTL(*PMTRQS) +
                          PROMPT('Target release')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT2) DFT(31) RANGE(31 63) +
                          PROMPT('Maximum precision')
                        ELEM       TYPE(*INT2) DFT(31) RANGE(0 63) +
                          PROMPT('Maximum scale')
                        ELEM       TYPE(*INT2) DFT(0) RANGE(0 63) +
                          PROMPT('Minimum divide scale')
             DEP        CTL(*ALWAYS) PARM(&SRCMBR &SRCSTMF) +
                          NBRTRUE((*GE 1)) MSGID(SQL8025)
             DEP        CTL(*ALWAYS) PARM(&SRCMBR &SRCSTMF) +
                          NBRTRUE((*LE 1)) MSGID(SQL8019)
