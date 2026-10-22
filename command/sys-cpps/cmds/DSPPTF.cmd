/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPPTF: show the program temporary fixes of a licensed program. */
             CMD        PROMPT('Display PTF') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(DSPPTF)
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Product')
             PARM       KWD(SELECT) TYPE(*CHAR) LEN(7) MAX(300) +
                          DFT(*ALL) SNGVAL((*ALL)) PROMPT('PTF numbers')
             PARM       KWD(RLS) TYPE(*CHAR) LEN(6) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Release')
             PARM       KWD(COVERONLY) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Cover letter only')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT *OUTFILE) +
                          PROMPT('Output')
             PARM       KWD(OUTFILE) TYPE(Q1) FILE(*OUT) PMTCTL(P1) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E1) PMTCTL(P1) +
                          PROMPT('Output member options')
             P1:        PMTCTL     CTL(OUTPUT) COND((*EQ *OUTFILE))
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
