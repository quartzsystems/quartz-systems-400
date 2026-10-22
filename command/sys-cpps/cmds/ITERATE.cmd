/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ITERATE: skip to the next pass of a DOWHILE, DOUNTIL or DOFOR group. */
             CMD        PROMPT('Iterate') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(ITERATE)
             PARM       KWD(CMDLBL) TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT)) PROMPT('Command label')
