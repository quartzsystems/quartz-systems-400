/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DCL: declare a CL variable. */
             CMD        PROMPT('Declare CL Variable') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(DCL)
             PARM       KWD(VAR) TYPE(*VARNAME) LEN(11) MIN(1) +
                          PROMPT('CL variable name')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) MIN(1) +
                          VALUES(*CHAR *DEC *LGL *INT *UINT *PTR) +
                          PROMPT('Type')
             PARM       KWD(STG) TYPE(*CHAR) LEN(8) RSTD(*YES) DFT(*AUTO) +
                          VALUES(*AUTO *DEFINED *BASED) PROMPT('Storage')
             PARM       KWD(LEN) TYPE(E1) PROMPT('Length')
             PARM       KWD(VALUE) TYPE(*X) LEN(5000) +
                          PROMPT('Initial value')
             PARM       KWD(DEFVAR) TYPE(E2) PROMPT('Defined on variable')
             PARM       KWD(BASPTR) TYPE(*VARNAME) LEN(11) +
                          PROMPT('Basing pointer')
             E1:        ELEM       TYPE(*INT4) RANGE(1 32767) +
                          PROMPT('Length')
                        ELEM       TYPE(*INT2) DFT(0) RANGE(0 9) +
                          PROMPT('Decimal positions')
             E2:        ELEM       TYPE(*VARNAME) LEN(11) +
                          PROMPT('Variable')
                        ELEM       TYPE(*INT4) DFT(1) RANGE(1 32767) +
                          PROMPT('Position')
