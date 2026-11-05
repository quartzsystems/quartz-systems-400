/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDPFCST: add a unique, primary key, referential or check constraint */
/* to a physical file.                                                  */
             CMD        PROMPT('Add Physical File Constraint') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHDBCMD) +
                          HLPID(ADDPFCST)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) MIN(1) +
                          VALUES(*REFCST *UNQCST *PRIKEY *CHKCST) +
                          PROMPT('Constraint type')
             PARM       KWD(CST) TYPE(*CHAR) LEN(258) DFT(*GEN) +
                          SPCVAL((*GEN)) PROMPT('Constraint name')
             PARM       KWD(KEY) TYPE(*NAME) LEN(10) MAX(120) +
                          PMTCTL(K1) PROMPT('Key')
             PARM       KWD(PRNFILE) TYPE(Q2) PMTCTL(R1) +
                          PROMPT('Parent file')
             PARM       KWD(PRNKEY) TYPE(*NAME) LEN(10) DFT(*PRNFILE) +
                          SPCVAL((*PRNFILE)) MAX(120) PMTCTL(R1) +
                          PROMPT('Parent key')
             PARM       KWD(DLTRULE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*NOACTION) VALUES(*NOACTION *CASCADE +
                          *SETNULL *SETDFT *RESTRICT) PMTCTL(R1) +
                          PROMPT('Delete rule')
             PARM       KWD(UPDRULE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*NOACTION) VALUES(*NOACTION *RESTRICT) +
                          PMTCTL(R1) PROMPT('Update rule')
             PARM       KWD(CHKCST) TYPE(*CHAR) LEN(2000) +
                          PMTCTL(C1) PROMPT('Check constraint')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             K1:        PMTCTL     CTL(TYPE) COND((*NE *CHKCST))
             R1:        PMTCTL     CTL(TYPE) COND((*EQ *REFCST))
             C1:        PMTCTL     CTL(TYPE) COND((*EQ *CHKCST))
