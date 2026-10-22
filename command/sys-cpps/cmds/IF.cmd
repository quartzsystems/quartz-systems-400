/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* IF: run a command when a condition holds. */
             CMD        PROMPT('If') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(IF)
             PARM       KWD(COND) TYPE(*CHAR) LEN(5000) MIN(1) EXPR(*YES) +
                          PROMPT('Condition')
             PARM       KWD(THEN) TYPE(*CMDSTR) LEN(3000) +
                          PROMPT('Command to run')
