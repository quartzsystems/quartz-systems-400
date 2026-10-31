/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RSTAUT: grant the private authorities restored with the profiles. */
             CMD        PROMPT('Restore Authority') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSRCMD) HLPID(RSTAUT)
             PARM       KWD(USRPRF) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          MAX(300) SNGVAL((*ALL)) PROMPT('User profile')
             PARM       KWD(SAVASPDEV) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*SYSBAS)) PMTCTL(*PMTRQS) +
                          PROMPT('Saved ASP device')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*SAVASPDEV) +
                          SPCVAL((*SAVASPDEV) (*SYSBAS)) PMTCTL(*PMTRQS) +
                          PROMPT('Restore to ASP device')
