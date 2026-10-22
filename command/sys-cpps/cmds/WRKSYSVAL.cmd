/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSYSVAL: work with the system values, all or one group. */
             CMD        PROMPT('Work with System Values') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(WRKSYSVAL)
             PARM       KWD(SYSVAL) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*ALC) (*DATTIM) (*EDT) (*LIBL) +
                          (*MSG) (*SEC) (*STG) (*SYSCTL)) +
                          PROMPT('System value')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
