/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDMSGD: add a message description to a message file. */
             CMD        PROMPT('Add Message Description') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHMHCMD) HLPID(ADDMSGD)
             PARM       KWD(MSGID) TYPE(*NAME) LEN(7) MIN(1) +
                          PROMPT('Message identifier')
             PARM       KWD(MSGF) TYPE(Q1) MIN(1) PROMPT('Message file')
             PARM       KWD(MSG) TYPE(*CHAR) LEN(132) MIN(1) +
                          PROMPT('First-level message text')
             PARM       KWD(SECLVL) TYPE(*CHAR) LEN(3000) DFT(*NONE) +
                          SPCVAL((*NONE)) +
                          PROMPT('Second-level message text')
             PARM       KWD(SEV) TYPE(*INT2) DFT(0) RANGE(0 99) +
                          PROMPT('Severity code')
             PARM       KWD(FMT) TYPE(E1) MAX(99) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Message data fields')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *CHAR *DEC *ALPHA +
                          *NAME) PMTCTL(*PMTRQS) PROMPT('Reply type')
             PARM       KWD(LEN) TYPE(E2) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Reply length')
             PARM       KWD(VALUES) TYPE(*CHAR) LEN(32) MAX(20) +
                          DFT(*NONE) SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Valid reply values')
             PARM       KWD(SPCVAL) TYPE(E3) MAX(20) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Special reply values')
             PARM       KWD(RANGE) TYPE(E4) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Range of reply values')
             PARM       KWD(REL) TYPE(E5) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Relationship for reply')
             PARM       KWD(DFT) TYPE(*CHAR) LEN(32) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Default reply value')
             PARM       KWD(DFTPGM) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Default program')
             PARM       KWD(DMPLST) TYPE(*CHAR) LEN(7) MAX(102) +
                          DFT(*NONE) SNGVAL((*NONE)) +
                          SPCVAL((*JOB) (*JOBINT) (*JOBDMP)) +
                          PMTCTL(*PMTRQS) PROMPT('Data to be dumped')
             PARM       KWD(ALROPT) TYPE(E6) DFT(*NO) SNGVAL((*NO)) +
                          PMTCTL(*PMTRQS) PROMPT('Alert options')
             PARM       KWD(LOGPRB) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Log problem')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*JOB) RANGE(1 65535) +
                          SPCVAL((*JOB 0) (*HEX 65535)) PMTCTL(*PMTRQS) +
                          PROMPT('Coded character set ID')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) MIN(1) +
                          VALUES(*QUAL *CHAR *CCHAR *HEX *DEC *BIN *QTDCHAR +
                          *UBIN *ITV *DTS *UTC *UTCD *UTCT *SYSPTR +
                          *SPP) PROMPT('Data type')
                        ELEM       TYPE(*INT2) RANGE(1 32767) +
                          SPCVAL((*VARY -1)) PROMPT('Length')
                        ELEM       TYPE(*INT2) DFT(0) RANGE(0 9) +
                          PROMPT('Decimal positions')
             E2:        ELEM       TYPE(*INT2) MIN(1) RANGE(1 32) +
                          PROMPT('Length')
                        ELEM       TYPE(*INT2) DFT(0) RANGE(0 9) +
                          PROMPT('Decimal positions')
             E3:        ELEM       TYPE(*CHAR) LEN(32) MIN(1) +
                          PROMPT('From value')
                        ELEM       TYPE(*CHAR) LEN(32) PROMPT('To value')
             E4:        ELEM       TYPE(*CHAR) LEN(32) MIN(1) +
                          PROMPT('Lower value')
                        ELEM       TYPE(*CHAR) LEN(32) MIN(1) +
                          PROMPT('Upper value')
             E5:        ELEM       TYPE(*CHAR) LEN(3) RSTD(*YES) MIN(1) +
                          VALUES(*EQ *LE *GE *GT *LT *NE *NL *NG) +
                          PROMPT('Relational operator')
                        ELEM       TYPE(*CHAR) LEN(32) MIN(1) +
                          PROMPT('Value')
             E6:        ELEM       TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *DEFER *UNATTEND *IMMED) +
                          PROMPT('Alert option')
                        ELEM       TYPE(*INT2) DFT(*NONE) RANGE(1 99) +
                          SPCVAL((*NONE 0)) +
                          PROMPT('Resource name variable')
