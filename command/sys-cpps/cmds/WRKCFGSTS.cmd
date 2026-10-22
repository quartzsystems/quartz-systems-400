/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKCFGSTS: show the status of configuration objects and work with */
/* them.                                                               */
             CMD        PROMPT('Work with Configuration Status') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHCFCMD) +
                          HLPID(WRKCFGSTS)
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          MIN(1) VALUES(*LIN *CTL *DEV *NWI *NWS *ASP) +
                          PROMPT('Type')
             PARM       KWD(CFGD) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Configuration description')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(RMTLOCNAME) TYPE(*NAME) LEN(8) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Remote location')
             PARM       KWD(STSOPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *VRYOFF *VRYON *ACTIVE) +
                          PMTCTL(*PMTRQS) PROMPT('Status filter')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PRV) VALUES(*PRV *BASIC *INTERMED) +
                          PMTCTL(*PMTRQS) PROMPT('Assistance level')
