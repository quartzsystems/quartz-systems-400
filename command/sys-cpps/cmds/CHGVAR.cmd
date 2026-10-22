/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGVAR: assign the value of an expression to a CL variable. */
             CMD        PROMPT('Change Variable') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(CHGVAR)
             PARM       KWD(VAR) TYPE(*VARNAME) LEN(11) MIN(1) +
                          PROMPT('CL variable name')
             PARM       KWD(VALUE) TYPE(*CHAR) LEN(5000) MIN(1) +
                          EXPR(*YES) PROMPT('New value')
