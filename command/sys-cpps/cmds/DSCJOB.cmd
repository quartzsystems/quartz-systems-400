/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSCJOB: disconnect the interactive job from its device, keeping */
/* it for a later sign-on.                                          */
             CMD        PROMPT('Disconnect Job') MAXPOS(*NONE) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHWMCMD) HLPID(DSCJOB)
             PARM       KWD(JOBLOG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LIST) VALUES(*LIST *NOLIST) +
                          PROMPT('Job log')
             PARM       KWD(DROP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*DEVD) VALUES(*DEVD *YES *NO) +
                          PROMPT('Drop line')
             PARM       KWD(ENDCNN) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('End connection')
