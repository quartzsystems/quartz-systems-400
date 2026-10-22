/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTOVR: delete file overrides at a call level. */
             CMD        PROMPT('Delete Override') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(DLTOVR)
             PARM       KWD(FILE) TYPE(*NAME) LEN(10) MIN(1) MAX(50) +
                          SNGVAL((*ALL)) PROMPT('File being overridden')
             PARM       KWD(LVL) TYPE(*CHAR) LEN(10) DFT(*) +
                          SPCVAL((*) (*ACTGRPDFN) (*JOB)) +
                          PROMPT('Level')
