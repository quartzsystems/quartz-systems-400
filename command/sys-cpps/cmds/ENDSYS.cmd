/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDSYS: end every subsystem but the controlling one, which leaves */
/* only the console job: the restricted state.                        */
             CMD        PROMPT('End System') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(ENDSYS)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*CNTRLD) VALUES(*CNTRLD *IMMED) +
                          PROMPT('How to end')
             PARM       KWD(DELAY) TYPE(*INT4) DFT(*NOLIMIT) +
                          RANGE(1 99999) SPCVAL((*NOLIMIT 0)) +
                          PROMPT('Controlled end delay time')
             PARM       KWD(ENDSBSOPT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          MAX(3) DFT(*DFT) VALUES(*NOJOBLOG *CHGPTY +
                          *CHGTSL) SNGVAL((*DFT)) +
                          PROMPT('End subsystem option')
             PARM       KWD(BCHTIMLMT) TYPE(*INT4) DFT(*NONE) +
                          RANGE(1 99999) SPCVAL((*NONE 0)) +
                          PROMPT('Batch time limit')
