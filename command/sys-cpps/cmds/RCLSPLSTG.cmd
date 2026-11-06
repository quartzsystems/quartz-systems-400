/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCLSPLSTG: free the spool storage no spooled file uses. */
             CMD        PROMPT('Reclaim Spool Storage') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSPCMD) HLPID(RCLSPLSTG)
             PARM       KWD(DAYS) TYPE(*INT2) MIN(1) RANGE(1 366) +
                          SPCVAL((*NONE 0)) PROMPT('Days')
             PARM       KWD(ASPGRP) TYPE(*NAME) LEN(10) DFT(*SYSBAS) +
                          SPCVAL((*SYSBAS) (*ALL) (*) (*CURASPGRP)) +
                          PROMPT('ASP group')
