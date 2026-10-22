/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DOUNTIL: open a group that repeats until a condition holds, tested */
/* at the end of each pass.                                             */
             CMD        PROMPT('Do Until') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(DOUNTIL)
             PARM       KWD(COND) TYPE(*CHAR) LEN(5000) MIN(1) EXPR(*YES) +
                          PROMPT('Condition')
