/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKWTR: list writers and work with them. */
             CMD        PROMPT('Work with Writers') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(WRKWTR)
             PARM       KWD(WTR) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*PRT) (*RMT)) PROMPT('Writer')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PRV) VALUES(*PRV *BASIC *INTERMED) +
                          PMTCTL(*PMTRQS) PROMPT('Assistance level')
