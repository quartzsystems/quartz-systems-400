/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPSRVPGM: show the attributes, modules, exports and signatures of */
/* a service program.                                                   */
             CMD        PROMPT('Display Service Program') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(DSPSRVPGM)
             PARM       KWD(SRVPGM) TYPE(Q1) MIN(1) PROMPT('Service program')
             PARM       KWD(DETAIL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALL) MAX(9) VALUES(*ALL *BASIC *SIZE +
                          *MODULE *SRVPGM *PROCEXP *DTAEXP *ACTGRPEXP +
                          *COPYRIGHT *SIGNATURE) SNGVAL((*ALL)) +
                          PROMPT('Detail')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
