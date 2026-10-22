/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WHEN: one choice of a SELECT group, taken when its condition holds. */
             CMD        PROMPT('When') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(WHEN)
             PARM       KWD(COND) TYPE(*CHAR) LEN(5000) MIN(1) EXPR(*YES) +
                          PROMPT('Condition')
             PARM       KWD(THEN) TYPE(*CMDSTR) LEN(3000) +
                          PROMPT('Command to run')
