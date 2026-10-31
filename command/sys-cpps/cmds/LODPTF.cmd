/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* LODPTF: load program temporary fixes from save files or media. */
             CMD        PROMPT('Load PTF') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(LODPTF)
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) MIN(1) +
                          PROMPT('Product')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*SAVF) (*SERVICE)) PROMPT('Device')
             PARM       KWD(SELECT) TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          MAX(300) SNGVAL((*ALL)) PROMPT('PTF numbers to select')
             PARM       KWD(OMIT) TYPE(*CHAR) LEN(7) MAX(300) +
                          PROMPT('PTF numbers to omit')
             PARM       KWD(SPRPTF) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*APYPERM) VALUES(*APYPERM *NOAPY) +
                          PMTCTL(*PMTRQS) PROMPT('Superseded PTFs')
             PARM       KWD(RLS) TYPE(*CHAR) LEN(6) DFT(*ONLY) +
                          SPCVAL((*ONLY)) PROMPT('Release')
             PARM       KWD(SAVF) TYPE(Q1) DFT(*DFT) SNGVAL((*DFT)) +
                          PMTCTL(*PMTRQS) PROMPT('Save file')
             PARM       KWD(COVER) TYPE(Q1) DFT(*DFT) SNGVAL((*DFT)) +
                          PMTCTL(*PMTRQS) PROMPT('Cover letter file')
             PARM       KWD(PATHID) TYPE(*INT4) DFT(*FIRST) +
                          RANGE(1 9999) SPCVAL((*FIRST 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Path identifier')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
