/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTSQLINF: print the SQL information of a program. */
             CMD        PROMPT('Print SQL Information') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(PRTSQLINF)
             PARM       KWD(OBJ) TYPE(Q1) MIN(1) SNGVAL((*JOB)) +
                          PROMPT('Object')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*PGM) VALUES(*PGM *SQLPKG *SRVPGM) +
                          PROMPT('Object type')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
