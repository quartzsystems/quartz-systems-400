/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKHDWRSC: list the hardware resources of one kind and work with them. */
             CMD        PROMPT('Work with Hardware Resources') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHSVCMD) HLPID(WRKHDWRSC)
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) MIN(1) +
                          VALUES(*CMN *CRP *CSA *LWS *PRC *STG) +
                          PROMPT('Type')
