/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVRDBDIRE: remove entries from the relational database directory. */
             CMD        PROMPT('Remove RDB Directory Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHRWCMD) HLPID(RMVRDBDIRE)
             PARM       KWD(RDB) TYPE(*CHAR) LEN(18) MIN(1) +
                          SPCVAL((*ALL) (*ALLRMT)) PROMPT('Entry')
