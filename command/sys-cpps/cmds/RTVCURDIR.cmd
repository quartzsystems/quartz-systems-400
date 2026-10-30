/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVCURDIR: the job's current directory into CL variables. */
             CMD        PROMPT('Retrieve Current Directory') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHP0LCMD) HLPID(RTVCURDIR)
             PARM       KWD(RTNDIR) TYPE(*CHAR) LEN(5000) RTNVAL(*YES) +
                          MIN(1) PROMPT('CL var for current directory')
             PARM       KWD(DIRNAMLEN) TYPE(*DEC) LEN(7 0) RTNVAL(*YES) +
                          MIN(1) PROMPT('CL var for directory length')
