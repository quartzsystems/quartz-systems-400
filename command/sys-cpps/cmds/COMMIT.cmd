/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* COMMIT: commit the current transaction. */
             CMD        PROMPT('Commit') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(COMMIT)
             PARM       KWD(CMTID) TYPE(*CHAR) LEN(4000) DFT(*NONE) +
                          SPCVAL((*NONE ' ')) PROMPT('Commit identifier')
