/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTDEVD: delete a device description. */
             CMD        PROMPT('Delete Device Description') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHCFCMD) HLPID(DLTDEVD)
             PARM       KWD(DEVD) TYPE(*GENERIC) LEN(10) MIN(1) +
                          PROMPT('Device description')
