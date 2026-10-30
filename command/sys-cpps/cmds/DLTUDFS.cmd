/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTUDFS: delete a user-defined file system that is not mounted. */
             CMD        PROMPT('Delete User-Defined FS') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(DLTUDFS)
             PARM       KWD(UDFS) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('User-defined file system')
