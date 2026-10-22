/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGPWD: change the password of the signed-on user; the old and new */
/* passwords are entered on a screen, so the command has no parameters. */
             CMD        PROMPT('Change Password') MAXPOS(0) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHSYCMD) HLPID(CHGPWD)
