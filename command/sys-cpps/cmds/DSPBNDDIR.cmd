/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPBNDDIR: list the entries of a binding directory. */
             CMD        PROMPT('Display Binding Directory') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(DSPBNDDIR)
             PARM       KWD(BNDDIR) TYPE(Q1) MIN(1) +
                          PROMPT('Binding directory')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT *OUTFILE) PROMPT('Output')
             PARM       KWD(OUTFILE) TYPE(Q2) PMTCTL(P1) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E1) PMTCTL(P1) +
                          PROMPT('Output member options')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL)) +
                          PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             P1:        PMTCTL     CTL(OUTPUT) COND((*EQ *OUTFILE)) +
                          NBRTRUE(*ALL)
