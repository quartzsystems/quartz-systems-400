/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKFCNUSG: the function usage registry, options 2 and 5. */
             CMD        PROMPT('Work with Function Usage') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHSYCMD) HLPID(WRKFCNUSG)
             PARM       KWD(FCNID) TYPE(*GENERIC) LEN(30) DFT(*ALL) +
                          SPCVAL((*ALL)) EXPR(*YES) +
                          PROMPT('Function ID')
