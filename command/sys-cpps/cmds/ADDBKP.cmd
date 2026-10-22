/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDBKP: add a breakpoint at statements of a program under debug. */
             CMD        PROMPT('Add Breakpoint') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHDBGCMD) HLPID(ADDBKP)
             PARM       KWD(STMT) TYPE(*CHAR) LEN(10) MIN(1) MAX(10) +
                          PROMPT('Statement identifier')
             PARM       KWD(PGMVAR) TYPE(E1) MAX(10) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Program variable')
             PARM       KWD(START) TYPE(*INT4) DFT(1) RANGE(1 99999) +
                          PROMPT('Start position')
             PARM       KWD(LEN) TYPE(*INT4) DFT(*DCL) RANGE(1 99999) +
                          SPCVAL((*DCL 0)) PROMPT('Length')
             PARM       KWD(OUTFMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CHAR) VALUES(*CHAR *HEX) +
                          PROMPT('Output format')
             PARM       KWD(PGM) TYPE(Q1) DFT(*DFTPGM) SNGVAL((*DFTPGM)) +
                          PROMPT('Program')
             PARM       KWD(SKIP) TYPE(*INT4) DFT(0) RANGE(0 99999) +
                          PROMPT('Skip count')
             PARM       KWD(BKPCOND) TYPE(E2) PROMPT('Breakpoint condition')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*CHAR) LEN(132) +
                          PROMPT('Variable')
                        ELEM       TYPE(*CHAR) LEN(132) MAX(5) +
                          PROMPT('Basing pointer')
             E2:        ELEM       TYPE(*CHAR) LEN(132) +
                          PROMPT('Variable')
                        ELEM       TYPE(*CHAR) LEN(3) RSTD(*YES) +
                          VALUES(*EQ *NE *GT *LT *GE *LE) +
                          PROMPT('Relational operator')
                        ELEM       TYPE(*CHAR) LEN(132) +
                          PROMPT('Value')
