/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSPMTOVR: a command's current values, for its prompt override. */
             CMD        PROMPT('Prompt Override Values') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(QSPMTOVR)
             PARM       KWD(CMD) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Command')
             PARM       KWD(KEY) TYPE(*CHAR) LEN(256) DFT(*NONE) +
                          PROMPT('Key parameters')
