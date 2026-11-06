/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCVJRNE: a journal's entries handed one at a time to an exit program */
/* in IBM's *TYPEn layouts. OBJFID and OBJJID (TYPE(*HEX) in IBM's      */
/* definition) are not part of this one: CPD0043 refuses them.          */
             CMD        PROMPT('Receive Journal Entry') MAXPOS(4) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(RCVJRNE)
             PARM       KWD(JRN) TYPE(Q1) MIN(1) EXPR(*YES) +
                          PROMPT('Journal')
             PARM       KWD(EXITPGM) TYPE(Q2) MIN(1) EXPR(*YES) +
                          PROMPT('Program to receive entries')
             PARM       KWD(FILE) TYPE(E1) SNGVAL((*ALLFILE)) MAX(300) +
                          EXPR(*YES) PROMPT('Journaled file')
             PARM       KWD(OBJ) TYPE(E8) MAX(300) EXPR(*YES) +
                          PROMPT('Objects')
             PARM       KWD(OBJPATH) TYPE(E9) MAX(300) EXPR(*YES) +
                          PROMPT('Objects')
             PARM       KWD(SUBTREE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) +
                          PROMPT('Directory subtree')
             PARM       KWD(PATTERN) TYPE(E10) MAX(20) EXPR(*YES) +
                          PROMPT('Name pattern')
             PARM       KWD(RCVRNG) TYPE(E2) DFT(*CURRENT) +
                          SNGVAL((*CURRENT) (*CURCHAIN)) EXPR(*YES) +
                          PROMPT('Range of journal receivers')
             PARM       KWD(FROMENTLRG) TYPE(*CHAR) LEN(20) +
                          DFT(*FIRST) SPCVAL((*FIRST)) EXPR(*YES) +
                          PROMPT('Starting large sequence number')
             PARM       KWD(FROMTIME) TYPE(E3) EXPR(*YES) +
                          PROMPT('Starting date and time')
             PARM       KWD(TOENTLRG) TYPE(*CHAR) LEN(20) DFT(*NONE) +
                          SPCVAL((*LAST) (*NONE)) EXPR(*YES) +
                          PROMPT('Ending large sequence number')
             PARM       KWD(TOTIME) TYPE(E4) EXPR(*YES) +
                          PROMPT('Ending date and time')
             PARM       KWD(NBRENT) TYPE(*INT4) DFT(*ALL) REL(*GE 1) +
                          SPCVAL((*ALL 0)) EXPR(*YES) +
                          PROMPT('Number of journal entries')
             PARM       KWD(JRNCDE) TYPE(E5) DFT(*ALL) +
                          SNGVAL((*ALL) (*CTL)) MAX(16) EXPR(*YES) +
                          PROMPT('Journal codes')
             PARM       KWD(ENTTYP) TYPE(*CHAR) LEN(2) DFT(*ALL) +
                          SNGVAL((*ALL '*A') (*RCD '*R')) MAX(300) +
                          EXPR(*YES) +
                          PROMPT('Journal entry types')
             PARM       KWD(JOB) TYPE(Q6) DFT(*ALL) SNGVAL((*ALL) (*)) +
                          EXPR(*YES) PROMPT('Job name')
             PARM       KWD(PGM) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) EXPR(*YES) PROMPT('Program')
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) EXPR(*YES) +
                          PROMPT('User profile')
             PARM       KWD(CCIDLRG) TYPE(*CHAR) LEN(20) DFT(*ALL) +
                          SPCVAL((*ALL)) EXPR(*YES) +
                          PROMPT('Commit cycle large identifier')
             PARM       KWD(DEPENT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *NONE) +
                          PROMPT('Dependent entries')
             PARM       KWD(ENTFMT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*TYPE1) VALUES(*TYPE1 *TYPE2 *TYPE3 +
                          *TYPE4 *TYPEPTR *JRNENTFMT) EXPR(*YES) +
                          PROMPT('Entry format')
             PARM       KWD(FMTMINDTA) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) EXPR(*YES) +
                          PROMPT('Format minimized data')
             PARM       KWD(NULLINDLEN) TYPE(E6) DFT(*ENTFMT) +
                          SNGVAL((*ENTFMT)) +
                          PROMPT('Null value indicators length')
             PARM       KWD(DELAY) TYPE(E7) EXPR(*YES) +
                          PROMPT('Delay time')
             PARM       KWD(BLKLEN) TYPE(*INT4) DFT(*NONE) +
                          RANGE(32 4000) SPCVAL((*NONE -1) (*CALC -2)) +
                          PROMPT('Block length')
             PARM       KWD(JRNENTFMT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(RJNE0100) VALUES(RJNE0100 RJNE0200) +
                          PMTCTL(P1) PROMPT('Journal entry format')
             PARM       KWD(RTNPTR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *SYSMNG *USRMNG) +
                          PMTCTL(P1) PROMPT('Return pointers')
             PARM       KWD(INCENT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*CONFIRMED) VALUES(*CONFIRMED *ALL) +
                          PMTCTL(*PMTRQS) PROMPT('Include entries')
             PARM       KWD(FROMENT) TYPE(*DEC) LEN(15 0) DFT(*FIRST) +
                          RANGE(1 9999999999) SPCVAL((*FIRST 0)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Starting sequence number')
             PARM       KWD(TOENT) TYPE(*DEC) LEN(15 0) DFT(*NONE) +
                          RANGE(1 9999999999) +
                          SPCVAL((*LAST -1) (*NONE -2)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Ending sequence number')
             PARM       KWD(CMTCYCID) TYPE(*DEC) LEN(15 0) DFT(*ALL) +
                          RANGE(1 9999999999) SPCVAL((*ALL 0)) +
                          EXPR(*YES) PMTCTL(*PMTRQS) +
                          PROMPT('Commit cycle identifier')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*ALL)) MIN(1) +
                          CHOICE('Name, *ALLFILE, *ALL')
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(Q3) MIN(1) PROMPT('File')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*ALL) (*NONE)) +
                          PROMPT('Member')
             Q4:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q5:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E2:        ELEM       TYPE(Q4) MIN(1) +
                          PROMPT('Starting journal receiver')
                        ELEM       TYPE(Q5) DFT(*CURRENT) +
                          SNGVAL((*CURRENT)) +
                          PROMPT('Ending journal receiver')
             E3:        ELEM       TYPE(*DATE) MIN(1) PROMPT('Starting date')
                        ELEM       TYPE(*TIME) PROMPT('Starting time')
             E4:        ELEM       TYPE(*DATE) MIN(1) PROMPT('Ending date')
                        ELEM       TYPE(*TIME) PROMPT('Ending time')
             E5:        ELEM       TYPE(*CHAR) LEN(2) RSTD(*YES) +
                          VALUES(A B C D E F J L M P R Q S T U Y) +
                          MIN(1) PROMPT('Journal code value')
                        ELEM       TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALLSLT) +
                          VALUES(*ALLSLT *IGNFILSLT *IGNOBJSLT) +
                          PROMPT('Journal code selection')
             Q6:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) +
                          RANGE(000000 999999) FULL(*YES) +
                          PROMPT('Number')
             E6:        ELEM       TYPE(*INT4) DFT(*VARLEN) RANGE(1 8000) +
                          SPCVAL((*VARLEN -3)) +
                          PROMPT('Field data format')
                        ELEM       TYPE(*INT4) RANGE(1 8000) +
                          PROMPT('Variable length field length')
             E7:        ELEM       TYPE(*INT4) DFT(30) RANGE(1 99999) +
                          SPCVAL((*NEXTENT 0)) +
                          PROMPT('Delay time value')
                        ELEM       TYPE(*INT4) RANGE(1 99999) +
                          SPCVAL((*CLS 0)) +
                          PROMPT('Maximum delay time value')
             P1:        PMTCTL     CTL(ENTFMT) COND((*EQ *JRNENTFMT)) +
                          NBRTRUE(*EQ 1)
             Q7:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*ALL)) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E8:        ELEM       TYPE(Q7) MIN(1) PROMPT('Object')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          VALUES(*FILE *DTAARA *DTAQ *LIB) MIN(1) +
                          PROMPT('Object type')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*ALL) (*NONE)) +
                          PROMPT('Member, if data base file')
             E9:        ELEM       TYPE(*PNAME) LEN(5000) PROMPT('Name')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*INCLUDE) VALUES(*INCLUDE *OMIT) +
                          PROMPT('Include or omit')
             E10:       ELEM       TYPE(*CHAR) LEN(5000) DFT('*') +
                          PROMPT('Pattern')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*INCLUDE) VALUES(*INCLUDE *OMIT) +
                          PROMPT('Include or omit')
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
             DEP        CTL(&DELAY) +
                          PARM((&TOENT *NE -2) (&TOENTLRG *NE *NONE)) +
                          NBRTRUE(*EQ 0) MSGID(CPD7063)
             DEP        CTL(&NULLINDLEN) +
                          PARM((&ENTFMT *NE *TYPE1) (&ENTFMT *NE *TYPE2)) +
                          NBRTRUE(*EQ 2) MSGID(CPD7064)
             DEP        CTL(*ALWAYS) PARM((&ENTFMT *EQ *TYPEPTR) +
                          (&NULLINDLEN *EQ -3)) NBRTRUE(*LE 1) +
                          MSGID(CPD70F7)
             DEP        CTL(*ALWAYS) PARM((&RTNPTR *NE *NONE) +
                          (&NULLINDLEN *EQ -3)) NBRTRUE(*LE 1) +
                          MSGID(CPD70F7)
             DEP        CTL(*ALWAYS) PARM(&RTNPTR +
                          (&ENTFMT *NE *JRNENTFMT)) NBRTRUE(*LE 1) +
                          MSGID(CPD70FC)
             DEP        CTL(*ALWAYS) PARM(&JRNENTFMT +
                          (&ENTFMT *NE *JRNENTFMT)) NBRTRUE(*LE 1) +
                          MSGID(CPD70FD)
             DEP        CTL(&FILE) PARM(&OBJ &OBJPATH) NBRTRUE(*EQ 0) +
                          MSGID(CPD700D)
             DEP        CTL(&PATTERN) PARM(&OBJPATH) NBRTRUE(*EQ 1) +
                          MSGID(CPD700E)
             DEP        CTL(&SUBTREE) PARM(&OBJPATH) NBRTRUE(*EQ 1) +
                          MSGID(CPD700E)
