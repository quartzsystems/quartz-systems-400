/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* OTHERWISE: the choice of a SELECT group taken when no WHEN holds. */
             CMD        PROMPT('Otherwise') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(OTHERWISE)
             PARM       KWD(CMD) TYPE(*CMDSTR) LEN(3000) +
                          PROMPT('Command to run')
