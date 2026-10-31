/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHKOPT: read every file of an optical volume to its end (a QS/400 */
/* command of its own).                                               */
             CMD        PROMPT('Check Optical') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHOPCMD) HLPID(CHKOPT)
             PARM       KWD(VOL) TYPE(*CHAR) LEN(32) MIN(1) +
                          SPCVAL((*MOUNTED)) PROMPT('Volume identifier')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Optical device')
