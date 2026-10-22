/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ANZDFTPWD: report the profiles whose password is the profile name, */
/* and disable or expire them when asked.                               */
             CMD        PROMPT('Analyze Default Passwords') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(ANZDFTPWD)
             PARM       KWD(ACTION) TYPE(*CHAR) LEN(8) RSTD(*YES) MAX(2) +
                          DFT(*NONE) SNGVAL((*NONE)) +
                          VALUES(*DISABLE *PWDEXP) PROMPT('Action taken')
