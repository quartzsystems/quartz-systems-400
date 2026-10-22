/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTUSRPRF: print a report on the user profiles selected. */
             CMD        PROMPT('Print User Profile') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(PRTUSRPRF)
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*AUTINFO) VALUES(*AUTINFO *ENVINFO +
                          *PWDINFO *PWDLVL) PROMPT('Type of information')
             PARM       KWD(SELECT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *USRCLS *SPCAUT +
                          *MISMATCH) PROMPT('Select by')
             PARM       KWD(USRCLS) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          MAX(5) DFT(*USER) VALUES(*USER *SYSOPR *PGMR +
                          *SECADM *SECOFR) PMTCTL(P1) +
                          PROMPT('User classes')
             PARM       KWD(SPCAUT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          MAX(8) DFT(*ALLOBJ) VALUES(*ALLOBJ *AUDIT +
                          *IOSYSCFG *JOBCTL *SAVSYS *SECADM *SERVICE +
                          *SPLCTL) PMTCTL(P2) PROMPT('Special authorities')
             P1:        PMTCTL     CTL(SELECT) COND((*EQ *USRCLS))
             P2:        PMTCTL     CTL(SELECT) COND((*EQ *SPCAUT))
