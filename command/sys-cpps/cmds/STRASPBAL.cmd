/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRASPBAL: spread the data of an ASP over its units. */
             CMD        PROMPT('Start ASP Balance') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(STRASPBAL)
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          VALUES(*CAPACITY *USAGE *HSM *MOVDTA *ENDALC +
                          *RSMALC *MP) PROMPT('Balance type')
             PARM       KWD(ASP) TYPE(*INT4) RANGE(1 32) MAX(32) +
                          SNGVAL((*ALL 0)) PROMPT('ASP number')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) MAX(32) +
                          SNGVAL((*ALLAVL)) PROMPT('ASP device')
             PARM       KWD(UNIT) TYPE(*INT4) MAX(300) +
                          PROMPT('Storage unit')
             PARM       KWD(TIMLMT) TYPE(*INT4) RANGE(1 9999) +
                          SPCVAL((*NOMAX -1)) PROMPT('Time limit')
             PARM       KWD(PRIORITY) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*MEDIUM) VALUES(*LOW *MEDIUM *HIGH) +
                          PROMPT('Balance priority')
             PARM       KWD(SUBTYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CALC) VALUES(*CALC *HDD *SSD) +
                          PROMPT('Subtype')
