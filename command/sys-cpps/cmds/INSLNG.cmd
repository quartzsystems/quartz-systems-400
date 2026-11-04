/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* INSLNG: install a secondary language library (QSYS29xx) from the */
/* national language text the system carries.                       */
             CMD        PROMPT('Install Language') ALLOW(*ALL) +
                          HLPPNLGRP(QHSZCMD) HLPID(INSLNG)
             PARM       KWD(LNG) TYPE(*CHAR) LEN(4) MIN(1) +
                          PROMPT('Language feature')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Replace objects')
