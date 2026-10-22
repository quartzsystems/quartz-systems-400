/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ANZPRFACT: schedule the job that disables profiles unused for the */
/* number of days given.                                                */
             CMD        PROMPT('Analyze Profile Activity') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(ANZPRFACT)
             PARM       KWD(DAYS) TYPE(*DEC) LEN(3) MIN(1) RANGE(30 366) +
                          SPCVAL((*NOMAX 0)) PROMPT('Days of inactivity')
