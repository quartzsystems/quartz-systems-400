/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCLTMPSTG: free the temporary storage no job holds any more. */
             CMD        PROMPT('Reclaim Temporary Storage') +
                          MAXPOS(*NONE) ALLOW(*ALL) +
                          HLPPNLGRP(QHLICMD) HLPID(RCLTMPSTG)
