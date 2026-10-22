/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* APYJRNCHG: apply journaled changes to files from a range of entries. */
             CMD        PROMPT('Apply Journaled Changes') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(APYJRNCHG)
             PARM       KWD(JRN) TYPE(Q1) MIN(1) PROMPT('Journal')
             PARM       KWD(FILE) TYPE(E1) MAX(300) DFT(*ALLFILE) +
                          SNGVAL((*ALLFILE)) PROMPT('Journaled file')
             PARM       KWD(RCVRNG) TYPE(E2) DFT(*LASTSAVE) +
                          SNGVAL((*LASTSAVE) (*CURRENT) (*CURCHAIN)) +
                          PROMPT('Range of journal receivers')
             PARM       KWD(FROMENT) TYPE(*DEC) LEN(20) DFT(*LASTSAVE) +
                          RANGE(1 18446744073709551600) +
                          SPCVAL((*LASTSAVE 0) (*FIRST -1)) +
                          PROMPT('Starting sequence number')
             PARM       KWD(TOENT) TYPE(*DEC) LEN(20) DFT(*LAST) +
                          RANGE(1 18446744073709551600) +
                          SPCVAL((*LAST 0)) PROMPT('Ending sequence number')
             PARM       KWD(TOJOBO) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('End at job open')
             PARM       KWD(TOJOBC) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('End at job close')
             PARM       KWD(FROMTIME) TYPE(E3) PMTCTL(*PMTRQS) +
                          PROMPT('Starting date and time')
             PARM       KWD(TOTIME) TYPE(E3) PMTCTL(*PMTRQS) +
                          PROMPT('Ending date and time')
             PARM       KWD(CMTBDY) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Stop at commitment boundary')
             PARM       KWD(OBJERROPT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*CONTINUE) VALUES(*CONTINUE *END) +
                          PMTCTL(*PMTRQS) PROMPT('On object error')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *OUTFILE) +
                          PMTCTL(*PMTRQS) PROMPT('Output')
             PARM       KWD(OUTFILE) TYPE(Q3) PMTCTL(*PMTRQS) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E4) PMTCTL(*PMTRQS) +
                          PROMPT('Output member options')
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
             E1:        ELEM       TYPE(Q1) MIN(1) PROMPT('File')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*ALL)) PROMPT('Member')
             E2:        ELEM       TYPE(Q1) MIN(1) PROMPT('Starting receiver')
                        ELEM       TYPE(Q1) DFT(*CURRENT) +
                          SNGVAL((*CURRENT)) PROMPT('Ending receiver')
             E3:        ELEM       TYPE(*DATE) MIN(1) PROMPT('Date')
                        ELEM       TYPE(*TIME) DFT(000000) PROMPT('Time')
             E4:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
