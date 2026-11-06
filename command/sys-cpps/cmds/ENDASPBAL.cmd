/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDASPBAL: end the balance running for an ASP. */
             CMD        PROMPT('End ASP Balance') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHLICMD) HLPID(ENDASPBAL)
             PARM       KWD(ASP) TYPE(*INT4) RANGE(1 32) MAX(32) +
                          SNGVAL((*ALL 0)) PROMPT('ASP number')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) MAX(32) +
                          SNGVAL((*ALLAVL)) PROMPT('ASP device')
