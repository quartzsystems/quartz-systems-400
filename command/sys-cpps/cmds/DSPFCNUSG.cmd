/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPFCNUSG: one or more functions and the profiles' usage of them. */
             CMD        PROMPT('Display Function Usage') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(DSPFCNUSG)
             PARM       KWD(FCNID) TYPE(*GENERIC) LEN(30) DFT(*ALL) +
                          SPCVAL((*ALL)) EXPR(*YES) +
                          PROMPT('Function ID')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) EXPR(*YES) +
                          PROMPT('Output')
