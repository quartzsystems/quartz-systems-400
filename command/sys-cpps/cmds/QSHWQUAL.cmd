/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSHWQUAL: check this machine's hardware for QS/400 and grade each   */
/* resource pass, warn or fail; shown, printed, or also written to a   */
/* FAT USB stick.                                                       */
             CMD        PROMPT('Qualify Hardware') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(QSHWQUAL)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(TODEV) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *USB) +
                          PROMPT('Also write to device')
