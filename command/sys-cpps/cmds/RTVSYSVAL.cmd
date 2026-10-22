/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVSYSVAL: put a system value into a CL variable. */
             CMD        PROMPT('Retrieve System Value') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RTVSYSVAL)
             PARM       KWD(SYSVAL) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('System value')
             PARM       KWD(RTNVAR) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          MIN(0) PROMPT('CL var for returned value')
