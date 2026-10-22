/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPLIB: show the objects of one or more libraries. */
             CMD        PROMPT('Display Library') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(DSPLIB)
             PARM       KWD(LIB) TYPE(*GENERIC) LEN(10) MAX(300) +
                          DFT(*LIBL) SNGVAL((*LIBL) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) SPCVAL((*CURLIB)) PROMPT('Library')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALLAVL) (*CURASPGRP) (*SYSBAS)) +
                          PMTCTL(*PMTRQS) PROMPT('ASP device')
