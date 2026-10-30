/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CD: change the job's current directory (CHGCURDIR's alias). */
             CMD        PROMPT('Change Current Directory') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(CD)
             PARM       KWD(DIR) TYPE(*PNAME) LEN(5000) MIN(1) +
                          EXPR(*YES) PROMPT('Directory')
