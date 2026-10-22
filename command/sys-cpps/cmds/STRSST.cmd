/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRSST: start the system service tools; the service tools sign-on is */
/* a screen, so the command has no parameters.                            */
             CMD        PROMPT('Start System Service Tools') MAXPOS(0) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHSVCMD) HLPID(STRSST)
