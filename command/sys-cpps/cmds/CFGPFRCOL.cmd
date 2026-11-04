/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CFGPFRCOL: configure Collection Services. The prompt shows the */
/* current configuration (QSPMTOVR).                              */
             CMD        PROMPT('Configure Perf Collection') MAXPOS(0) +
                          ALLOW(*ALL) PMTOVRPGM(QSYS/QYPSCFGO) +
                          HLPPNLGRP(QHWMCMD) HLPID(CFGPFRCOL)
             PARM       KWD(INTERVAL) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME)) +
                          CHOICE('*SAME, .25, .50, 1.0, 5.0...') +
                          PROMPT('Default interval')
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Collection library')
             PARM       KWD(DFTCOLPRF) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MINIMUM *STANDARD +
                          *STANDARDP *ENHCPCPLN *CUSTOM) +
                          CHOICE('*SAME, *MINIMUM, *STANDARD...') +
                          PROMPT('Default collection profile')
             PARM       KWD(CYCTIME) TYPE(*CHAR) LEN(6) DFT(*SAME) +
                          SPCVAL((*SAME)) CHOICE('Time, *SAME') +
                          PROMPT('Cycle time')
             PARM       KWD(CYCITV) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME)) CHOICE('*SAME, 1-24 hours') +
                          PROMPT('Cycle interval')
             PARM       KWD(RETPERIOD) TYPE(E1) +
                          PROMPT('Collection retention period')
             PARM       KWD(CRTDBF) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Create database files')
             PARM       KWD(CRTPFRSUM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PMTCTL(*PMTRQS) +
                          PROMPT('Create performance summary')
 E1:         ELEM       TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*PERM)) +
                          CHOICE('*SAME, 1-720, *PERM') +
                          PROMPT('Number of units')
             ELEM       TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*HOURS) +
                          VALUES(*HOURS *DAYS) PROMPT('Unit of time')
