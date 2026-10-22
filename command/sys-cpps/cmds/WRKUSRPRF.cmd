/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKUSRPRF: list user profiles and work with them. */
             CMD        PROMPT('Work with User Profiles') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(WRKUSRPRF)
             PARM       KWD(USRPRF) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User profile')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PRV) VALUES(*PRV *BASIC *INTERMED) +
                          PMTCTL(*PMTRQS) PROMPT('Assistance level')
