/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* MOV: move objects to another directory or name. */
             CMD        PROMPT('Move Object') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(MOV)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(TODIR) TYPE(*PNAME) LEN(5000) DFT('.') +
                          PROMPT('To directory')
             PARM       KWD(TOOBJ) TYPE(*PNAME) LEN(5000) +
                          PROMPT('To object')
             PARM       KWD(FROMCCSID) TYPE(*INT4) DFT(*OBJ) +
                          RANGE(1 65533) SPCVAL((*OBJ 0) (*PCASCII -1) +
                          (*JOBCCSID -2)) PROMPT('From CCSID')
             PARM       KWD(TOCCSID) TYPE(*INT4) DFT(*OBJ) +
                          RANGE(1 65533) SPCVAL((*OBJ 0) (*CALC -1) +
                          (*STDASCII -2) (*PCASCII -3) (*JOBCCSID -4)) +
                          PROMPT('To CCSID')
             PARM       KWD(DTAFMT) TYPE(*INT4) RSTD(*YES) +
                          DFT(*BINARY) SPCVAL((*BINARY 0) (*TEXT 1)) +
                          PROMPT('Data format')
             PARM       KWD(FROMCODPAG) TYPE(*INT4) DFT(*OBJ) +
                          RANGE(1 32767) SPCVAL((*OBJ 0) (*PCASCII -1)) +
                          PMTCTL(*PMTRQS) PROMPT('From code page')
             PARM       KWD(TOCODEPAGE) TYPE(*INT4) DFT(*OBJ) +
                          RANGE(1 32767) SPCVAL((*OBJ 0) (*CALC -1) +
                          (*STDASCII -2) (*PCASCII -3)) PMTCTL(*PMTRQS) +
                          PROMPT('To code page')
