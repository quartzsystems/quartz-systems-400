/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                        */
/*                                                                        */
/* After hlinv.sql, one command a line (Command Entry, or the client's    */
/* Run Command): what SQL cannot say. See README.md.                      */

/* HLTEST.NOJRN is not journaled: SQL7008 under commitment control.       */
ENDJRNPF FILE(HLTEST/NOJRN)

/* The profiles the checklist signs on as (change the passwords).         */
CRTUSRPRF USRPRF(BOB) PASSWORD('Secret1') PWDEXP(*NO) TEXT('QAP checklist')
CRTUSRPRF USRPRF(ANN) PASSWORD('Annpw01') PWDEXP(*NO) TEXT('QAP checklist')

/* HLPAY.SALARIES is ANN's alone: SQL0551 (NotAuthorized) for BOB.        */
GRTOBJAUT OBJ(HLPAY/SALARIES) OBJTYPE(*FILE) USER(*PUBLIC) AUT(*EXCLUDE)
GRTOBJAUT OBJ(HLPAY/SALARIES) OBJTYPE(*FILE) USER(ANN) AUT(*ALL)
