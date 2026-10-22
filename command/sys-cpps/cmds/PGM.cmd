/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PGM: open a CL procedure and name its parameters. */
             CMD        PROMPT('Program') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(PGM)
             PARM       KWD(PARM) TYPE(*VARNAME) LEN(11) MAX(255) +
                          PROMPT('Parameters')
