/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKCFGSTS: show the status of configuration objects and work with */
/* them.                                                               */
             CMD        PROMPT('Work with Configuration Status') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHDCCMD1) +
                          HLPID(WRKCFGSTS)
             PARM       KWD(CFGTYPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          MIN(1) VALUES(*NWS *NWI *LIN *CTL *DEV) +
                          PROMPT('Type')
             PARM       KWD(CFGD) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*CMN) (*ASYNC) (*BSC) (*DDI) +
                          (*ELAN) (*FAX) (*FR) (*IDLC) (*NET) (*PPP) +
                          (*SDLC) (*TDLC) (*TRLAN) (*WLS) (*X25) (*APPC) +
                          (*DSP) (*PRT) (*VWS) (*LOC)) +
                          PROMPT('Configuration description')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(RMTLOCNAME) TYPE(*GENERIC) LEN(8) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(PLOC) +
                          PROMPT('Remote location')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*PRV) VALUES(*PRV *USRPRF *BASIC *INTERMED +
                          *ADVANCED) PMTCTL(*PMTRQS) +
                          PROMPT('Assistance level')
             PARM       KWD(RANGE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NET) VALUES(*NET *OBJ) PROMPT('Range')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(11) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *ACTIVE *FAILED *VARYOFF +
                          *VARYON *AVAILABLE) PROMPT('Status')
 PLOC:       PMTCTL     CTL(CFGD) COND((*EQ *LOC))
