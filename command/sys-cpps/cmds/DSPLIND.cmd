/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPLIND: show a line description. */
             CMD        PROMPT('Display Line Description') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHDCLIN) HLPID(DSPLIND)
             PARM       KWD(LIND) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Line description')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *BASIC *SWTNWILST *CTL +
                          *SWTCTLLST *ACTSWTCTL *SSAP *GRPADR *FCNADR) +
                          PROMPT('Option')
