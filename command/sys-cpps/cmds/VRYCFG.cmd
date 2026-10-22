/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* VRYCFG: vary configuration objects on or off. */
             CMD        PROMPT('Vary Configuration') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHCFCMD) HLPID(VRYCFG)
             PARM       KWD(CFGOBJ) TYPE(*GENERIC) LEN(10) MIN(1) +
                          MAX(256) PROMPT('Configuration object')
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          MIN(1) VALUES(*LIN *CTL *DEV *NWS *NWI *ASP) +
                          PROMPT('Type')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          MIN(1) VALUES(*ON *OFF *RESET *ALLOCATE +
                          *DEALLOCATE) PROMPT('Status')
             PARM       KWD(RANGE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NET) VALUES(*NET *OBJ) PROMPT('Range')
             PARM       KWD(VRYWAIT) TYPE(*INT2) DFT(*CFGOBJD) +
                          RANGE(15 180) SPCVAL((*CFGOBJD -1) (*NOWAIT 0)) +
                          PROMPT('Vary on wait')
             PARM       KWD(ASCVRYOFF) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Asynchronous vary off')
             PARM       KWD(FRCVRYOFF) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Forced vary off')
             PARM       KWD(RESETOPT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Reset the resource')
