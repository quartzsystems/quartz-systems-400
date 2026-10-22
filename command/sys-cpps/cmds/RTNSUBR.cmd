/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTNSUBR: return from a subroutine before its ENDSUBR. */
             CMD        PROMPT('Return from Subroutine') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RTNSUBR)
             PARM       KWD(RTNVAL) TYPE(*INT4) DFT(0) +
                          PROMPT('Return value')
