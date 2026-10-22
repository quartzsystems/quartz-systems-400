/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* GOTO: continue at a labelled command. */
             CMD        PROMPT('Go To') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(GOTO)
             PARM       KWD(CMDLBL) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Command label')
