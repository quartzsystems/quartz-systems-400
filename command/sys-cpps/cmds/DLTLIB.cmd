/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTLIB: delete a library and everything in it. */
             CMD        PROMPT('Delete Library') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(DLTLIB)
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*) (*ALLAVL) (*CURASPGRP) (*SYSBAS)) +
                          PMTCTL(*PMTRQS) PROMPT('ASP device')
