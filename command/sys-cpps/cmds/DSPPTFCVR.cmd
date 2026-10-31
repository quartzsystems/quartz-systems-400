/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPPTFCVR: show the cover letters of program temporary fixes. */
             CMD        PROMPT('Display PTF Cover Letter') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(DSPPTFCVR)
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Product')
             PARM       KWD(SELECT) TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          MAX(300) SNGVAL((*ALL)) PROMPT('PTF numbers to select')
             PARM       KWD(CVRATR) TYPE(*CHAR) LEN(8) DFT(*ALL) +
                          MAX(4) SNGVAL((*ALL)) RSTD(*YES) +
                          VALUES(*SPCINST *DFT *NONE) +
                          PROMPT('Cover letter attribute')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(8) DFT(*ALL) +
                          RSTD(*YES) VALUES(*ALL *NOTAPY *APYPERM *APYTEMP) +
                          PROMPT('PTF status')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
