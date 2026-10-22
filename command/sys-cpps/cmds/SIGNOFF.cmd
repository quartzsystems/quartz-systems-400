/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SIGNOFF: end the interactive job and show the sign-on display. */
             CMD        PROMPT('Sign Off') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHWMCMD) HLPID(SIGNOFF)
             PARM       KWD(LOG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NOLIST) VALUES(*NOLIST *LIST) +
                          PROMPT('Job log')
             PARM       KWD(DROP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*DEVD) VALUES(*DEVD *YES *NO) +
                          PROMPT('Drop line')
             PARM       KWD(ENDCNN) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('End connection')
