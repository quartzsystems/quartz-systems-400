/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CLRLIB: delete every object in a library, keeping the library. */
             CMD        PROMPT('Clear Library') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(CLRLIB)
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALLAVL) (*CURASPGRP) (*SYSBAS)) +
                          PMTCTL(*PMTRQS) PROMPT('ASP device')
