/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTADPOBJ: print the programs that adopt the authority of a user. */
             CMD        PROMPT('Print Adopting Objects') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(PRTADPOBJ)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*ALL)) PROMPT('User profile')
             PARM       KWD(CHGRPTONLY) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Changed report only')
