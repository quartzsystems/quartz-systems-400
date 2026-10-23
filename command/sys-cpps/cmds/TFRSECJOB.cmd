/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* TFRSECJOB: move the display to the secondary job, signing one */
/* on when there is none.                                          */
             CMD        PROMPT('Transfer Secondary Job') +
                          ALLOW(*INTERACT *IPGM *IMOD *IREXX *EXEC) +
                          HLPPNLGRP(QHWCCMD) HLPID(TFRSECJOB)
