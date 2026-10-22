/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DOWHILE: open a group that repeats while a condition holds. */
             CMD        PROMPT('Do While') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(DOWHILE)
             PARM       KWD(COND) TYPE(*CHAR) LEN(5000) MIN(1) EXPR(*YES) +
                          PROMPT('Condition')
