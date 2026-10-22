/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKLIB: the Work with Libraries display. */
             CMD        PROMPT('Work with Libraries') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(WRKLIB)
             PARM       KWD(LIB) TYPE(*GENERIC) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*USRLIBL) (*ALL) (*ALLUSR) +
                          (*CURLIB)) PROMPT('Library')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALLAVL) (*CURASPGRP) (*SYSBAS)) +
                          PMTCTL(*PMTRQS) PROMPT('ASP device')
