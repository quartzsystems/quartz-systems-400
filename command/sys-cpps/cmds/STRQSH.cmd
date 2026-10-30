/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRQSH: run a Qshell command, or start the QSH terminal session. */
             CMD        PROMPT('Start QSH') ALLOW(*ALL) +
                          HLPPNLGRP(QHZSHCMD) HLPID(STRQSH)
             PARM       KWD(CMD) TYPE(*CHAR) LEN(5000) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Command')
