/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCLACTGRP: delete a named activation group that is no longer on the */
/* call stack, or every eligible one.                                   */
             CMD        PROMPT('Reclaim Activation Group') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(RCLACTGRP)
             PARM       KWD(ACTGRP) TYPE(*NAME) LEN(10) DFT(*ELIGIBLE) +
                          SPCVAL((*ELIGIBLE)) PROMPT('Activation group')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*NORMAL) VALUES(*NORMAL *ABNORMAL) +
                          PROMPT('Reclaim option')
