/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDMFS: mount a file system over a directory (MOUNT's twin).      */
/* TYPE(*UDFS) only; the network file systems are not supported.        */
             CMD        PROMPT('Add Mounted FS') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(ADDMFS)
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) MIN(1) +
                          VALUES(*NFS *UDFS *NETWORK) +
                          PROMPT('Type of file system')
             PARM       KWD(MFS) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('File system to mount')
             PARM       KWD(MNTOVRDIR) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Directory to mount over')
             PARM       KWD(OPTIONS) TYPE(*CHAR) LEN(2048) +
                          DFT('rw,suid,retry=5,rsize=32768,wsize=32768,timeo=20,retrans=5,acregmin=30,acregmax=60,acdirmin=30,acdirmax=60,hard') +
                          PROMPT('Mount options')
             PARM       KWD(CCSID) TYPE(*CHAR) LEN(16) DFT(*BINARY) +
                          PMTCTL(*PMTRQS) PROMPT('Coded character set ID')
             PARM       KWD(CODEPAGE) TYPE(*CHAR) LEN(16) DFT(*BINARY) +
                          PMTCTL(*PMTRQS) PROMPT('Code page')
