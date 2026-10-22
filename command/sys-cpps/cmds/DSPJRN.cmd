/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPJRN: display, print or write to a file the entries of a journal. */
             CMD        PROMPT('Display Journal') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(DSPJRN)
             PARM       KWD(JRN) TYPE(Q1) MIN(1) PROMPT('Journal')
             PARM       KWD(FILE) TYPE(E1) MAX(300) DFT(*ALLFILE) +
                          SNGVAL((*ALLFILE)) PROMPT('Journaled file')
             PARM       KWD(RCVRNG) TYPE(E2) DFT(*CURRENT) +
                          SNGVAL((*CURRENT) (*CURCHAIN)) +
                          PROMPT('Range of journal receivers')
             PARM       KWD(FROMENT) TYPE(*DEC) LEN(20) DFT(*FIRST) +
                          RANGE(1 18446744073709551600) +
                          SPCVAL((*FIRST 0)) PROMPT('Starting sequence number')
             PARM       KWD(TOENT) TYPE(*DEC) LEN(20) DFT(*LAST) +
                          RANGE(1 18446744073709551600) +
                          SPCVAL((*LAST 0)) PROMPT('Ending sequence number')
             PARM       KWD(NBRENT) TYPE(*DEC) LEN(10) DFT(*ALL) +
                          RANGE(1 2147483647) SPCVAL((*ALL 0)) +
                          PROMPT('Number of journal entries')
             PARM       KWD(JRNCDE) TYPE(E3) MAX(16) DFT(*ALL) +
                          SNGVAL((*ALL) (*CTL)) PROMPT('Journal codes')
             PARM       KWD(ENTTYP) TYPE(*CHAR) LEN(2) MAX(50) DFT(*ALL) +
                          SNGVAL((*ALL) (*RCD)) PROMPT('Journal entry types')
             PARM       KWD(JOB) TYPE(Q2) DFT(*ALL) SNGVAL((*ALL) (*)) +
                          PROMPT('Job')
             PARM       KWD(PGM) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Program')
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User profile')
             PARM       KWD(CMTCYCID) TYPE(*DEC) LEN(20) DFT(*ALL) +
                          RANGE(1 18446744073709551600) SPCVAL((*ALL 0)) +
                          PROMPT('Commit cycle identifier')
             PARM       KWD(DEPENT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *NONE) +
                          PROMPT('Dependent entries')
             PARM       KWD(FROMTIME) TYPE(E4) PROMPT('Starting date and time')
             PARM       KWD(TOTIME) TYPE(E4) PROMPT('Ending date and time')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT *OUTFILE) +
                          PROMPT('Output')
             PARM       KWD(OUTFILFMT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*TYPE1) VALUES(*TYPE1 *TYPE2 *TYPE3 *TYPE4 +
                          *TYPE5) PMTCTL(*PMTRQS) +
                          PROMPT('Output file format')
             PARM       KWD(OUTFILE) TYPE(Q3) PMTCTL(*PMTRQS) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E5) PMTCTL(*PMTRQS) +
                          PROMPT('Output member options')
             PARM       KWD(ENTDTALEN) TYPE(*DEC) LEN(5) DFT(*OUTFILFMT) +
                          RANGE(1 32766) SPCVAL((*OUTFILFMT 0) (*CALC -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Entry data length')
             PARM       KWD(NULLINDLEN) TYPE(*DEC) LEN(5) DFT(*OUTFILFMT) +
                          RANGE(1 8000) SPCVAL((*OUTFILFMT 0) (*CALC -1) +
                          (*VARLEN -2)) PMTCTL(*PMTRQS) +
                          PROMPT('Null value indicator length')
             PARM       KWD(INCENT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*CONFIRMED) VALUES(*CONFIRMED *ALL) +
                          PMTCTL(*PMTRQS) PROMPT('Include entries')
             PARM       KWD(FMTMINDTA) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Format minimized data')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Job name')
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) PROMPT('Number')
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q4:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(Q4) MIN(1) PROMPT('File')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*ALL)) PROMPT('Member')
             E2:        ELEM       TYPE(Q4) MIN(1) PROMPT('Starting receiver')
                        ELEM       TYPE(Q4) DFT(*CURRENT) +
                          SNGVAL((*CURRENT)) PROMPT('Ending receiver')
             E3:        ELEM       TYPE(*CHAR) LEN(1) MIN(1) +
                          PROMPT('Journal code')
                        ELEM       TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALLSLT) VALUES(*ALLSLT *IGNFILSLT) +
                          PROMPT('Selection')
             E4:        ELEM       TYPE(*DATE) MIN(1) PROMPT('Date')
                        ELEM       TYPE(*TIME) DFT(000000) PROMPT('Time')
             E5:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
