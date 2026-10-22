/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CLOF: close a file opened by OPNQRYF or OPNDBF. */
             CMD        PROMPT('Close File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CLOF)
             PARM       KWD(OPNID) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Open file identifier')
