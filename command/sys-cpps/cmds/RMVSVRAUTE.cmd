/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVSVRAUTE: remove a profile's entry for a remote server. */
             CMD        PROMPT('Remove Server Auth Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSECCMD) HLPID(RMVSVRAUTE)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*CURRENT)) PROMPT('User profile')
             PARM       KWD(SERVER) TYPE(*CHAR) LEN(200) MIN(1) +
                          CASE(*MIXED) INLPMTLEN(32) PROMPT('Server')
