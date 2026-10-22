/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTJRN: create a journal attached to a journal receiver. */
             CMD        PROMPT('Create Journal') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(CRTJRN)
             PARM       KWD(JRN) TYPE(Q1) MIN(1) PROMPT('Journal')
             PARM       KWD(JRNRCV) TYPE(Q2) MIN(1) MAX(2) +
                          PROMPT('Journal receiver')
             PARM       KWD(ASP) TYPE(*DEC) LEN(3) DFT(*LIBASP) +
                          RANGE(1 32) SPCVAL((*LIBASP 0)) +
                          PROMPT('Journal ASP')
             PARM       KWD(JRNMSGQ) TYPE(Q2) DFT(QSYSOPR) +
                          PROMPT('Journal message queue')
             PARM       KWD(MNGRCV) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*USER) VALUES(*USER *SYSTEM) +
                          PROMPT('Manage receivers')
             PARM       KWD(DLTRCV) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Delete receivers')
             PARM       KWD(RCVSIZOPT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *RMVINTENT *MINFIXLEN +
                          *MAXOPT1 *MAXOPT2 *MAXOPT3) MAX(4) +
                          PMTCTL(*PMTRQS) PROMPT('Receiver size options')
             PARM       KWD(MINENTDTA) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *FILE *DTAARA *FLDBDY) +
                          MAX(3) PMTCTL(*PMTRQS) +
                          PROMPT('Minimize entry specific data')
             PARM       KWD(FIXLENDTA) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *USR *PGM *PGMLIB +
                          *SYSSEQ *RMTADR *THD *LUW *XID) MAX(9) +
                          PMTCTL(*PMTRQS) PROMPT('Fixed length data')
             PARM       KWD(JRNOBJLMT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*MAX250K) VALUES(*MAX250K *MAX10M) +
                          PMTCTL(*PMTRQS) PROMPT('Journal object limit')
             PARM       KWD(JRNCACHE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Journal caching')
             PARM       KWD(JRNSTATE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ACTIVE) VALUES(*ACTIVE *STANDBY) +
                          PMTCTL(*PMTRQS) PROMPT('Journal state')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Public authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
