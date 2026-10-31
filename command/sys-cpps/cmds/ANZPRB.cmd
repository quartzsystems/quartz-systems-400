/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ANZPRB: guided analysis of a problem the user saw; the questions */
/* are panels, so a display is needed.                              */
             CMD        PROMPT('Analyze Problem') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHSXCMD) HLPID(ANZPRB)
             PARM       KWD(ANZTYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*MENU) VALUES(*MENU) PROMPT('Type of analysis')
