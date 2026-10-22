/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCLSTG: reclaim storage; the system must be in the restricted state. */
             CMD        PROMPT('Reclaim Storage') MAXPOS(*NONE) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHLICMD) HLPID(RCLSTG)
             PARM       KWD(SELECT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *DBXREF *DIR) +
                          PROMPT('Select')
             PARM       KWD(OMIT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *DBXREF *DIR) +
                          PROMPT('Omit')
             PARM       KWD(ASPDEV) TYPE(*NAME) LEN(10) DFT(*SYSBAS) +
                          SPCVAL((*SYSBAS) (*ALL)) PROMPT('ASP device')
             PARM       KWD(ESTIMATE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Estimate only')
