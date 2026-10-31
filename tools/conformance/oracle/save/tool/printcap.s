case 30-dspsfwrsc DSPSFWRSC in print form
splf dspsfwrsc DSPSFWRSC OUTPUT(*PRINT)
case 31-dspptf DSPPTF in print form
splf dspptf-all DSPPTF OUTPUT(*PRINT)
splf dspptf-ss1 DSPPTF LICPGM(5770SS1) OUTPUT(*PRINT)
splf dspptf-lic DSPPTF LICPGM(5770999) OUTPUT(*PRINT)
case 32-ptfgrp PTF groups
splf wrkptfgrp WRKPTFGRP OUTPUT(*PRINT)
sql QSYS2.GROUP_PTF_INFO
SELECT * FROM QSYS2.GROUP_PTF_INFO ORDER BY PTF_GROUP_NAME
;;
case 33-ipla IPL attributes, service attributes
splf dspipla DSPIPLA
splf dspsrva DSPSRVA
splf dspsrvsts DSPSRVSTS
splf dspbckup DSPBCKUP
case 34-devices Tape, optical and media library devices, image catalogs
splf wrkdevd-tap WRKDEVD DEVD(*TAP) OUTPUT(*PRINT)
splf wrkdevd-opt WRKDEVD DEVD(*OPT) OUTPUT(*PRINT)
splf wrkdevd-tapmlb WRKDEVD DEVD(*TAPMLB) OUTPUT(*PRINT)
splf wrkdevd-optmlb WRKDEVD DEVD(*OPTMLB) OUTPUT(*PRINT)
splf wrkcfgsts-dev WRKCFGSTS CFGTYPE(*DEV) OUTPUT(*PRINT)
sql image catalogs in QUSRSYS
SELECT OBJNAME, OBJTYPE, OBJOWNER, OBJTEXT FROM TABLE(QSYS2.OBJECT_STATISTICS('QUSRSYS', '*IMGCLG')) X
;;
splf wrkhdwrsc-stg WRKHDWRSC TYPE(*STG) OUTPUT(*PRINT)
case 35-problems Problems, licenses, contact
splf wrkprb WRKPRB OUTPUT(*PRINT)
splf wrklicinf WRKLICINF OUTPUT(*PRINT)
splf dsplickey DSPLICKEY OUTPUT(*PRINT)
splf dspsysval-lic DSPSYSVAL SYSVAL(QSRLNBR) OUTPUT(*PRINT)
