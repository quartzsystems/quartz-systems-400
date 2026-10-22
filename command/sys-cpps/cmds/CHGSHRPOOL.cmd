/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSHRPOOL: change the size, activity level and tuning of a shared */
/* storage pool.                                                        */
             CMD        PROMPT('Change Shared Storage Pool') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGSHRPOOL)
             PARM       KWD(POOL) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*MACHINE) (*BASE) (*INTERACT) (*SPOOL)) +
                          PROMPT('Shared pool')
             PARM       KWD(SIZE) TYPE(*INT4) DFT(*SAME) +
                          RANGE(32 2147483647) SPCVAL((*SAME -1)) +
                          PROMPT('Storage size')
             PARM       KWD(ACTLVL) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -1)) +
                          PROMPT('Activity level')
             PARM       KWD(PAGING) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *FIXED *CALC) +
                          PROMPT('Paging option')
             PARM       KWD(PTY) TYPE(*INT2) DFT(*SAME) RANGE(1 14) +
                          SPCVAL((*SAME -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Priority')
             PARM       KWD(MINPCT) TYPE(*DEC) LEN(5 2) DFT(*SAME) +
                          RANGE(1 100) SPCVAL((*SAME -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Minimum size percent')
             PARM       KWD(MAXPCT) TYPE(*DEC) LEN(5 2) DFT(*SAME) +
                          RANGE(1 100) SPCVAL((*SAME -1)) PMTCTL(*PMTRQS) +
                          PROMPT('Maximum size percent')
             PARM       KWD(MINFAULT) TYPE(*DEC) LEN(6 2) DFT(*SAME) +
                          RANGE(0 9999.99) SPCVAL((*SAME -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Minimum faults')
             PARM       KWD(PERFAULT) TYPE(*DEC) LEN(6 2) DFT(*SAME) +
                          RANGE(0 9999.99) SPCVAL((*SAME -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Per-thread faults')
             PARM       KWD(MAXFAULT) TYPE(*DEC) LEN(6 2) DFT(*SAME) +
                          RANGE(0 9999.99) SPCVAL((*SAME -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum faults')
