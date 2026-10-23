/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKDEVD: list device descriptions and work with them. */
             CMD        PROMPT('Work with Device Descriptions') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHCFCMD) HLPID(WRKDEVD)
             PARM       KWD(DEVD) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*APPC) (*ASC) (*BSC) (*CMN) +
                          (*CRP) (*DKT) (*DSP) (*FNC) (*HOST) (*INTRA) +
                          (*LWS) (*MLB) (*NET) (*OPT) (*PRT) (*RTL) +
                          (*SNPT) (*SNUF) (*TAP) (*VWS)) +
                          PROMPT('Device description')
