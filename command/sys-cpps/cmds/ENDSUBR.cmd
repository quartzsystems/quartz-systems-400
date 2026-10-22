/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDSUBR: close a subroutine and return a value to CALLSUBR. */
             CMD        PROMPT('End Subroutine') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(ENDSUBR)
             PARM       KWD(RTNVAL) TYPE(*INT4) DFT(0) +
                          PROMPT('Return value')
