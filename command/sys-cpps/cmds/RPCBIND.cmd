/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RPCBIND: start the RPC binder daemon (QNFSRPCD) alone.              */
             CMD        PROMPT('Start RPC Binder Daemon') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(RPCBIND)
             PARM       KWD(RTVRPCREG) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*NO) SPCVAL((*NO N) (*YES Y)) +
                          PROMPT('Retrieve RPC registration')
