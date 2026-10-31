/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSRVPVD: the service providers this system reports to. */
             CMD        PROMPT('Work with Service Providers') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSXCMD) HLPID(WRKSRVPVD)
             PARM       KWD(CNNTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ANY) VALUES(*ANY *DIRECT *IPCNN *NONE) +
                          PROMPT('Connection type')
