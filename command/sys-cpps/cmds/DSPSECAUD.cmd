/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPSECAUD: show the auditing system values and the audit journal. */
             CMD        PROMPT('Display Security Auditing') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(DSPSECAUD)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
