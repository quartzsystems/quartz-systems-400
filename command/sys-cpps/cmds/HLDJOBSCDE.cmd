/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* HLDJOBSCDE: hold entries of the job schedule. */
             CMD        PROMPT('Hold Job Schedule Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(HLDJOBSCDE)
             PARM       KWD(JOB) TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL)) PROMPT('Job name')
             PARM       KWD(ENTRYNBR) TYPE(*INT4) DFT(*ONLY) +
                          RANGE(1 999999) SPCVAL((*ONLY 0) (*ALL -1)) +
                          PROMPT('Entry number')
