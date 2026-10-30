/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVPCLTBLE: remove a protocol from the protocol table. */
             CMD        PROMPT('Remove Protocol Table Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) HLPID(RMVPCLTBLE)
             PARM       KWD(PROTOCOL) TYPE(*CHAR) LEN(32) MIN(1) CASE(*MIXED) +
                          PROMPT('Protocol')
