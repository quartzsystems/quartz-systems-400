/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSYSVAL: change a system value. */
             CMD        PROMPT('Change System Value') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(CHGSYSVAL)
             PARM       KWD(SYSVAL) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('System value')
             PARM       KWD(VALUE) TYPE(*CHAR) LEN(1024) MIN(1) +
                          CASE(*MIXED) PROMPT('New value')
