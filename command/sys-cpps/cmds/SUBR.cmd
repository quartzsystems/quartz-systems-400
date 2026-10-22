/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SUBR: open a subroutine of the procedure. */
             CMD        PROMPT('Subroutine') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(SUBR)
             PARM       KWD(SUBR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Subroutine')
