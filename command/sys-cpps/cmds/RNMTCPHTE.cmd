/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RNMTCPHTE: move a host table entry to a new address. */
             CMD        PROMPT('Rename TCP/IP Host Table Entry') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(RNMTCPHTE)
             PARM       KWD(INTNETADR) TYPE(*CHAR) LEN(45) MIN(1) +
                          PROMPT('Internet address')
             PARM       KWD(NEWINTNETA) TYPE(*CHAR) LEN(45) MIN(1) +
                          PROMPT('New internet address')
