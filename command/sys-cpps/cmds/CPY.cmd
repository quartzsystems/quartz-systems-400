/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPY: copy objects, a directory's contents with SUBTREE(*ALL). */
             CMD        PROMPT('Copy Object') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(CPY)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(TODIR) TYPE(*PNAME) LEN(5000) DFT('.') +
                          PROMPT('To directory')
             PARM       KWD(TOOBJ) TYPE(*PNAME) LEN(5000) +
                          PROMPT('To object')
             PARM       KWD(SYMLNK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Symbolic link')
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
             PARM       KWD(SUBTREE) TYPE(*INT4) RSTD(*YES) +
                          DFT(*NODIR) SPCVAL((*NODIR 0) (*NONE 1) +
                          (*ALL 2)) PROMPT('Directory subtree')
             PARM       KWD(REPLACE) TYPE(*INT4) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1)) +
                          PROMPT('Replace object')
             PARM       KWD(OWNER) TYPE(*INT4) RSTD(*YES) DFT(*NEW) +
                          SPCVAL((*NEW 0) (*KEEP 1)) PROMPT('Owner')
             PARM       KWD(AUT) TYPE(*INT4) RSTD(*YES) DFT(*OBJ) +
                          SPCVAL((*OBJ 1) (*INDIR 2) (*INDIROBJ 3)) +
                          PROMPT('Authority')
             PARM       KWD(FROMCODPAG) TYPE(*INT4) DFT(*OBJ) +
                          RANGE(1 32767) SPCVAL((*OBJ 0) (*PCASCII -1)) +
                          PMTCTL(*PMTRQS) PROMPT('From code page')
             PARM       KWD(TOCODEPAGE) TYPE(*INT4) DFT(*OBJ) +
                          RANGE(1 32767) SPCVAL((*OBJ 0) (*CALC -1) +
                          (*STDASCII -2) (*PCASCII -3)) PMTCTL(*PMTRQS) +
                          PROMPT('To code page')
