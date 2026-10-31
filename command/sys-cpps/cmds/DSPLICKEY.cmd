/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPLICKEY: show the license keys added to the system. */
             CMD        PROMPT('Display License Key Information') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHLZCMD) +
                          HLPID(DSPLICKEY)
             PARM       KWD(PRDID) TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Product identifier')
             PARM       KWD(LICTRM) TYPE(*CHAR) LEN(6) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('License term')
             PARM       KWD(FEATURE) TYPE(*CHAR) LEN(4) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Feature')
             PARM       KWD(SERIAL) TYPE(*CHAR) LEN(8) DFT(*LOCAL) +
                          SPCVAL((*LOCAL) (*ALL)) +
                          PROMPT('System serial number')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT *LICKEYFILE) +
                          PROMPT('Output')
             PARM       KWD(LICKEYFILE) TYPE(*PNAME) LEN(256) +
                          PMTCTL(P1) PROMPT('License key stream file')
             P1:        PMTCTL     CTL(OUTPUT) COND((*EQ *LICKEYFILE))
