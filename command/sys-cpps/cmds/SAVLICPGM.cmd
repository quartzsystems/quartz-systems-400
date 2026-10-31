/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SAVLICPGM: save a licensed program for installing elsewhere. */
             CMD        PROMPT('Save Licensed Program') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSZCMD) HLPID(SAVLICPGM)
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) MIN(1) +
                          PROMPT('Product')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) MAX(4) +
                          SNGVAL((*SAVF)) PROMPT('Device')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(5) DFT(*BASE) +
                          SPCVAL((*BASE) (*ALL)) +
                          PROMPT('Optional part to be saved')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *PGM *LNG) +
                          PROMPT('Object type')
             PARM       KWD(LNG) TYPE(*CHAR) LEN(8) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Language for licensed program')
             PARM       KWD(RLS) TYPE(*CHAR) LEN(6) DFT(*ONLY) +
                          SPCVAL((*ONLY)) PROMPT('Release')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of media option')
             PARM       KWD(SAVF) TYPE(Q1) PMTCTL(P1) PROMPT('Save file')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL *AFTER *REPLACE) +
                          PROMPT('Clear')
             PARM       KWD(DTACPR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*DEV) VALUES(*DEV *NO *YES *LOW *MEDIUM +
                          *HIGH) PMTCTL(*PMTRQS) PROMPT('Data compression')
             PARM       KWD(TGTRLS) TYPE(*CHAR) LEN(8) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*PRV)) PROMPT('Target release')
             P1:        PMTCTL     CTL(DEV) COND((*EQ *SAVF))
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
