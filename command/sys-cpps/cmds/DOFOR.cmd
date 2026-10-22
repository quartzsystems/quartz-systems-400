/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DOFOR: open a group that repeats while an integer variable counts */
/* from one value to another.                                          */
             CMD        PROMPT('Do For') MAXPOS(4) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(DOFOR)
             PARM       KWD(VAR) TYPE(*VARNAME) LEN(11) MIN(1) +
                          PROMPT('CL variable name')
             PARM       KWD(FROM) TYPE(*CHAR) LEN(5000) MIN(1) EXPR(*YES) +
                          PROMPT('From value')
             PARM       KWD(TO) TYPE(*CHAR) LEN(5000) MIN(1) EXPR(*YES) +
                          PROMPT('To value')
             PARM       KWD(BY) TYPE(*INT4) DFT(1) PROMPT('Increment')
