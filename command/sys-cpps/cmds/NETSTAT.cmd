/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* NETSTAT: the network status: interfaces, routes, connections. */
             CMD        PROMPT('Work with TCP/IP Network Sts') +
                          ALLOW(*ALL) HLPPNLGRP(QHTONCMD) HLPID(NETSTAT)
             PARM       KWD(OPTION) TYPE(*INT4) RSTD(*YES) DFT(*SELECT) +
                          SPCVAL((*SELECT 0) (*IFC 1) (*RTE 2) (*CNN 3) +
                          (*IFC6 4) (*RTE6 5) (*CNN6 6) (*STATUS 10)) +
                          PROMPT('Option')
