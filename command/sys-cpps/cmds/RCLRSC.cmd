/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCLRSC: free the resources (open files, storage) left behind by */
/* programs that have ended.                                         */
             CMD        PROMPT('Reclaim Resources') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(RCLRSC)
             PARM       KWD(LVL) TYPE(*CHAR) LEN(7) RSTD(*YES) DFT(*) +
                          VALUES(* *CALLER) PROMPT('Level')
