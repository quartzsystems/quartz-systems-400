/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CMPJRNIMG: the before and after images of a journaled file's record */
/* changes compared, the differences printed (QPCMPIMG).                */
             CMD        PROMPT('Compare Journal Images') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(CMPJRNIMG)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) EXPR(*YES) +
                          PROMPT('File')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) EXPR(*YES) PROMPT('Member')
             PARM       KWD(RCVRNG) TYPE(E1) DFT(*CURRENT) +
                          SNGVAL((*CURRENT)) EXPR(*YES) +
                          PROMPT('Range of journal receivers')
             PARM       KWD(FROMENTLRG) TYPE(*CHAR) LEN(20) +
                          DFT(*FIRST) SPCVAL((*FIRST)) EXPR(*YES) +
                          PROMPT('Starting large sequence number')
             PARM       KWD(FROMENT) TYPE(*DEC) LEN(15 0) DFT(*FIRST) +
                          RANGE(1 9999999999) SPCVAL((*FIRST 0)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Starting sequence number')
             PARM       KWD(FROMTIME) TYPE(E2) EXPR(*YES) +
                          PROMPT('Starting date and time')
             PARM       KWD(TOENTLRG) TYPE(*CHAR) LEN(20) DFT(*LAST) +
                          SPCVAL((*LAST)) EXPR(*YES) +
                          PROMPT('Ending large sequence number')
             PARM       KWD(TOENT) TYPE(*DEC) LEN(15 0) DFT(*LAST) +
                          RANGE(1 9999999999) SPCVAL((*LAST -1)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Ending sequence number')
             PARM       KWD(TOTIME) TYPE(E3) EXPR(*YES) +
                          PROMPT('Ending date and time')
             PARM       KWD(CMPOPT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*BOTH) VALUES(*BOTH *AFTER) EXPR(*YES) +
                          PROMPT('Compare option')
             PARM       KWD(RCDNBR) TYPE(*UINT4) DFT(*ALL) REL(*GE 1) +
                          SPCVAL((*ALL 0)) EXPR(*YES) +
                          PROMPT('Record number')
             PARM       KWD(JOB) TYPE(Q4) DFT(*ALL) SNGVAL((*ALL)) +
                          EXPR(*YES) PROMPT('Job name')
             PARM       KWD(PGM) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) EXPR(*YES) PROMPT('Program')
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) EXPR(*YES) +
                          PROMPT('User profile')
             PARM       KWD(CCIDLRG) TYPE(*CHAR) LEN(20) DFT(*ALL) +
                          SPCVAL((*ALL)) EXPR(*YES) +
                          PROMPT('Commit cycle large identifier')
             PARM       KWD(CMTCYCID) TYPE(*DEC) LEN(15 0) DFT(*ALL) +
                          RANGE(1 9999999999) SPCVAL((*ALL 0)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Commit cycle identifier')
             PARM       KWD(OUTFMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CHAR) VALUES(*CHAR *HEX) EXPR(*YES) +
                          PROMPT('Output format')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(Q2) MIN(1) +
                          PROMPT('Starting journal receiver')
                        ELEM       TYPE(Q3) DFT(*CURRENT) +
                          SNGVAL((*CURRENT)) +
                          PROMPT('Ending journal receiver')
             E2:        ELEM       TYPE(*DATE) MIN(1) PROMPT('Starting date')
                        ELEM       TYPE(*TIME) PROMPT('Starting time')
             E3:        ELEM       TYPE(*DATE) MIN(1) PROMPT('Ending date')
                        ELEM       TYPE(*TIME) PROMPT('Ending time')
             Q4:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) +
                          RANGE(000000 999999) FULL(*YES) +
                          PROMPT('Number')
             DEP        CTL(&FROMENT) PARM(&FROMTIME) NBRTRUE(*EQ 0) +
                          MSGID(CPD7061)
             DEP        CTL(&TOENT) PARM(&TOTIME) NBRTRUE(*EQ 0) +
                          MSGID(CPD7062)
             DEP        CTL(&FROMENTLRG) PARM(&FROMTIME) +
                          NBRTRUE(*EQ 0) MSGID(CPD7056)
             DEP        CTL(&TOENTLRG) PARM(&TOTIME) NBRTRUE(*EQ 0) +
                          MSGID(CPD7057)
             DEP        CTL(*ALWAYS) PARM(&FROMENTLRG &FROMENT) +
                          NBRTRUE(*LE 1) MSGID(CPD705A)
             DEP        CTL(*ALWAYS) PARM(&TOENTLRG &TOENT) +
                          NBRTRUE(*LE 1) MSGID(CPD705B)
             DEP        CTL(*ALWAYS) PARM(&CCIDLRG &CMTCYCID) +
                          NBRTRUE(*LE 1) MSGID(CPD705C)
             DEP        CTL((&CMPOPT *EQ *AFTER)) +
                          PARM((&JOB *NE *ALL) (&PGM *NE *ALL) +
                          (&USRPRF *NE *ALL) (&CCIDLRG *NE *ALL) +
                          (&CMTCYCID *NE 0)) NBRTRUE(*EQ 0) +
                          MSGID(CPD7028)
             DEP        CTL((&CMPOPT *EQ *AFTER)) PARM((&RCDNBR *NE 0)) +
                          NBRTRUE(*EQ 1) MSGID(CPD7029)
