/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ELSE: run a command when the preceding IF condition did not hold. */
             CMD        PROMPT('Else') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(ELSE)
             PARM       KWD(CMD) TYPE(*CMDSTR) LEN(3000) +
                          PROMPT('Command to run')
