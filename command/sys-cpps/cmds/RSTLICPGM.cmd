/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RSTLICPGM: install a licensed program from media or a save file. */
             CMD        PROMPT('Restore Licensed Program') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSZCMD) HLPID(RSTLICPGM)
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) MIN(1) +
                          PROMPT('Product')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) MAX(4) +
                          SNGVAL((*SAVF)) PROMPT('Device')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(5) DFT(*BASE) +
                          SPCVAL((*BASE)) PROMPT('Optional part to be restored')
             PARM       KWD(RSTOBJ) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *PGM *LNG) +
                          PROMPT('Type of object to be restored')
             PARM       KWD(LNG) TYPE(*CHAR) LEN(8) DFT(*PRIMARY) +
                          SPCVAL((*PRIMARY)) PROMPT('Language for licensed program')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *PRINT) PROMPT('Output')
             PARM       KWD(REPLACERLS) TYPE(*CHAR) LEN(6) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*NO)) PMTCTL(*PMTRQS) +
                          PROMPT('Release to be replaced')
             PARM       KWD(RLS) TYPE(*CHAR) LEN(6) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Release')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of media option')
             PARM       KWD(SAVF) TYPE(Q1) PMTCTL(P1) PROMPT('Save file')
             P1:        PMTCTL     CTL(DEV) COND((*EQ *SAVF))
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
