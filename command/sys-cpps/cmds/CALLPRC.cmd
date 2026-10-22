/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CALLPRC: call a bound procedure with parameters passed by reference */
/* or by value.                                                          */
             CMD        PROMPT('Call Procedure') MAXPOS(2) +
                          ALLOW(*BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(CALLPRC)
             PARM       KWD(PRC) TYPE(*CHAR) LEN(256) MIN(1) +
                          CASE(*MIXED) PROMPT('Procedure')
             PARM       KWD(PARM) TYPE(E1) MAX(300) PROMPT('Parameters')
             PARM       KWD(RTNVAL) TYPE(*VARNAME) LEN(11) +
                          SPCVAL((*NONE)) PROMPT('CL var for return value')
             E1:        ELEM       TYPE(*X) LEN(5000) EXPR(*YES) +
                          PROMPT('Value')
                        ELEM       TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*BYREF) VALUES(*BYREF *BYVAL) +
                          PROMPT('Passing method')
