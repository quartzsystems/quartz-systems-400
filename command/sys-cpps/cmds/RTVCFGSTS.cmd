/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVCFGSTS: put the status of a configuration description into CL */
/* variables.                                                          */
             CMD        PROMPT('Retrieve Configuration Status') +
                          MAXPOS(2) ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RTVCFGSTS)
             PARM       KWD(CFGD) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Configuration description')
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) MIN(1) +
                          VALUES(*LIN *CTL *DEV *NWI *NWS) PROMPT('Type')
             PARM       KWD(STSCDE) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for STSCDE (5 0)')
             PARM       KWD(STS) TYPE(*CHAR) LEN(20) RTNVAL(*YES) +
                          PROMPT('CL var for STS (20)')
