/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DMPCLPGM: print the variables and messages of the running CL */
/* procedure.                                                       */
             CMD        PROMPT('Dump CL Program') +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(DMPCLPGM)
