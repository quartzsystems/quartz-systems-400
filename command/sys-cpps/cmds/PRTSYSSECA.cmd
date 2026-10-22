/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTSYSSECA: print the security system values and network attributes */
/* against their recommended settings.                                   */
             CMD        PROMPT('Print System Security Attr') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(PRTSYSSECA)
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *SYSVAL *NETA) +
                          PROMPT('Type of information')
