/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* POSDBF: position an open database file at its start or end. */
             CMD        PROMPT('Position Database File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(POSDBF)
             PARM       KWD(OPNID) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Open file identifier')
             PARM       KWD(POSITION) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          MIN(1) VALUES(*START *END) PROMPT('Position')
