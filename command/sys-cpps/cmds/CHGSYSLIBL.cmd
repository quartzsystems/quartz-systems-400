/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSYSLIBL: add a library to, or remove one from, the system part */
/* of the library list of the job.                                    */
             CMD        PROMPT('Change System Library List') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGSYSLIBL)
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ADD) VALUES(*ADD *REMOVE) PROMPT('Option')
