/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SAVSAVFDTA: a save file's contents onto tape or optical, as the save */
/* that made it would have written them, so RSTLIB and RSTOBJ restore   */
/* from the media and DSPTAP shows the original save.                   */
             CMD        PROMPT('Save Save File Data') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSRCMD) +
                          HLPID(SAVSAVFDTA)
             PARM       KWD(SAVF) TYPE(Q1) MIN(1) EXPR(*YES) +
                          PROMPT('Save file')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) MAX(4) +
                          EXPR(*YES) PROMPT('Device')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(32) DFT(*MOUNTED) +
                          SNGVAL((*MOUNTED)) MAX(75) EXPR(*YES) +
                          PROMPT('Volume identifier')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(*END) +
                          RANGE(1 16777215) SPCVAL((*END -3)) +
                          EXPR(*YES) PROMPT('Sequence number')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          EXPR(*YES) PROMPT('End of media option')
             PARM       KWD(OPTFILE) TYPE(*PNAME) LEN(256) DFT('*') +
                          PROMPT('Optical file')
             PARM       KWD(USEOPTBLK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Use optimum block')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL *AFTER +
                          *REPLACE) EXPR(*YES) PROMPT('Clear')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*PERM) +
                          SPCVAL((*PERM)) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('File expiration date')
             PARM       KWD(COMPACT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*DEV) VALUES(*DEV *NO) EXPR(*YES) +
                          PMTCTL(*PMTRQS) PROMPT('Data compaction')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *PRINT *OUTFILE) +
                          EXPR(*YES) PMTCTL(*PMTRQS) PROMPT('Output')
             PARM       KWD(OUTFILE) TYPE(Q2) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E1) EXPR(*YES) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Output member options')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) +
                          PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             DEP        CTL((&OUTPUT *EQ *OUTFILE)) PARM(&OUTFILE) +
                          NBRTRUE(*EQ 1) MSGID(CPD371E)
             DEP        CTL(&OUTMBR) PARM(&OUTFILE) NBRTRUE(*EQ 1) +
                          MSGID(CPD9867)
             DEP        CTL(&OUTFILE) PARM((&OUTPUT *EQ *OUTFILE)) +
                          NBRTRUE(*EQ 1) MSGID(CPD371F)
