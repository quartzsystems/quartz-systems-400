/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKLICINF: work with the license information of products. */
             CMD        PROMPT('Work with License Information') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHLZCMD) +
                          HLPID(WRKLICINF)
             PARM       KWD(PRDID) TYPE(*CHAR) LEN(10) DFT(*INSTALLED) +
                          SPCVAL((*INSTALLED) (*ALL)) +
                          PROMPT('Product identifier')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
