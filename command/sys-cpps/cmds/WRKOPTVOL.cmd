/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKOPTVOL: work with the optical volumes the system knows.         */
             CMD        PROMPT('Work with Optical Volumes') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(WRKOPTVOL)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Optical device')
             PARM       KWD(VOL) TYPE(*GENERIC) LEN(32) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Volume identifier')
             PARM       KWD(CSI) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*NONE)) PROMPT('Side information')
             PARM       KWD(RTVEXTINFO) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Retrieve extended information')
