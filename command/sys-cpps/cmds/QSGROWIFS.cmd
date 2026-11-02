/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSGROWIFS: give the integrated file system more of ASP 1 now. */
             CMD        PROMPT('Grow the IFS') ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(QSGROWIFS)
             PARM       KWD(SIZE) TYPE(*INT4) DFT(*STEP) +
                          RANGE(64 16777216) SPCVAL((*STEP 0)) +
                          PROMPT('Storage to add (MB)')
