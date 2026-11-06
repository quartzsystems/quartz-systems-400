/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKTIMZON: the time zone descriptions of QSYS, to change, delete, */
/* display, print or make the system's (QTIMZON).                    */
             CMD        PROMPT('Work with Time Zone Desc') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWCCMD) +
                          HLPID(WRKTIMZON)
             PARM       KWD(TIMZON) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Time zone description')
