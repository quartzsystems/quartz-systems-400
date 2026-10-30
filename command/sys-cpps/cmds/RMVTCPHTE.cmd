/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVTCPHTE: remove an address from the host table. */
             CMD        PROMPT('Remove TCP/IP Host Table Entry') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(RMVTCPHTE)
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(45) MIN(1) +
                          PROMPT('Internet address')
