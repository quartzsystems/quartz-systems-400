/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKIMGCLGE: work with the entries of an image catalog.             */
             CMD        PROMPT('Work with Image Catalog Entries') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(WRKIMGCLGE)
             PARM       KWD(IMGCLG) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Image catalog')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
