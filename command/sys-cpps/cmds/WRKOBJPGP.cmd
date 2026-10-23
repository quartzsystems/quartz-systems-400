/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKOBJPGP: list the objects a group profile is the primary group of. */
             CMD        PROMPT('Work with Objects by Primary Group') +
                          MAXPOS(2) ALLOW(*INTERACT *IPGM *IREXX *EXEC +
                          *IMOD) HLPPNLGRP(QHSYCMD) HLPID(WRKOBJPGP)
             PARM       KWD(PGP) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Primary group')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Object type')
