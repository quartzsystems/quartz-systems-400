/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGJRN: attach a new receiver to a journal or change its attributes. */
             CMD        PROMPT('Change Journal') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHJRCMD) HLPID(CHGJRN)
             PARM       KWD(JRN) TYPE(Q1) MIN(1) PROMPT('Journal')
             PARM       KWD(JRNRCV) TYPE(Q1) MAX(2) DFT(*SAME) +
                          SNGVAL((*SAME) (*GEN)) PROMPT('Journal receiver')
             PARM       KWD(SEQOPT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*CONT) VALUES(*CONT *RESET) +
                          PROMPT('Sequence option')
             PARM       KWD(JRNMSGQ) TYPE(Q2) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Journal message queue')
             PARM       KWD(MNGRCV) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *USER *SYSTEM) +
                          PROMPT('Manage receivers')
             PARM       KWD(DLTRCV) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Delete receivers')
             PARM       KWD(RCVSIZOPT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*NONE *RMVINTENT *MINFIXLEN +
                          *MAXOPT1 *MAXOPT2 *MAXOPT3) MAX(4) +
                          SNGVAL((*SAME)) PMTCTL(*PMTRQS) +
                          PROMPT('Receiver size options')
             PARM       KWD(MINENTDTA) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*NONE *FILE *DTAARA *FLDBDY) +
                          MAX(3) SNGVAL((*SAME)) PMTCTL(*PMTRQS) +
                          PROMPT('Minimize entry specific data')
             PARM       KWD(FIXLENDTA) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*JOB *USR *PGM *PGMLIB +
                          *SYSSEQ *RMTADR *THD *LUW *XID) MAX(9) +
                          SNGVAL((*SAME)) PMTCTL(*PMTRQS) +
                          PROMPT('Fixed length data')
             PARM       KWD(JRNOBJLMT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MAX250K *MAX10M) +
                          PMTCTL(*PMTRQS) PROMPT('Journal object limit')
             PARM       KWD(JRNCACHE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Journal caching')
             PARM       KWD(JRNSTATE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ACTIVE *STANDBY) +
                          PMTCTL(*PMTRQS) PROMPT('Journal state')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
