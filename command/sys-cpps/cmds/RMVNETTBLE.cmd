/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVNETTBLE: remove a network from the network table. */
             CMD        PROMPT('Remove Network Table Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) HLPID(RMVNETTBLE)
             PARM       KWD(NETWORK) TYPE(*CHAR) LEN(255) MIN(1) CASE(*MIXED) +
                          PROMPT('Network')
