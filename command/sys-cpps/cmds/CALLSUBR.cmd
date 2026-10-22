/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CALLSUBR: call a subroutine of the procedure. */
             CMD        PROMPT('Call Subroutine') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(CALLSUBR)
             PARM       KWD(SUBR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Subroutine')
             PARM       KWD(RTNVAL) TYPE(*VARNAME) LEN(11) +
                          SPCVAL((*NONE)) PROMPT('CL var for return value')
