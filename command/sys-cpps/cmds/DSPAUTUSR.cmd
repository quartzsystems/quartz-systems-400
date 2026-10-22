/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPAUTUSR: list every user profile with its group and password state. */
             CMD        PROMPT('Display Authorized Users') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(DSPAUTUSR)
             PARM       KWD(SEQ) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*USRPRF) VALUES(*USRPRF *GRPPRF) +
                          PROMPT('Sequence')
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
