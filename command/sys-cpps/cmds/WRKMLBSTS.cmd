/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKMLBSTS: work with media libraries and their drives.             */
             CMD        PROMPT('Work with Media Library Status') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHSVCMD) +
                          HLPID(WRKMLBSTS)
             PARM       KWD(MLB) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Media library')
