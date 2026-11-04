**FREE
ctl-opt dftactgrp(*no);
/copy QOAR/QRPGLESRC,QRNOPENACC
/copy QOAR/QRPGLESRC,QRNDTAINTO
/copy QOAR/QRPGLESRC,QRNDTAGEN
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-ds oa likeds(QrnOpenAccess_T);
dcl-ds nv likeds(QrnNameValue_T);
dcl-ds nvs likeds(QrnNamesValues_T) based(p);
dcl-ds rl likeds(QrnRecordLevels_T) based(p);
dcl-ds pc likeds(QrnPrtctl_T);
dcl-ds di likeds(QrnDiParm_T);
dcl-ds de likeds(QrnDiEnv) based(p);
dcl-ds dg likeds(QrnDgParm_T);
dcl-ds ge likeds(QrnDgEnv) based(p);
dcl-s p pointer;
dcl-s b pointer;
line = 'sizes ' + %char(%size(oa)) + ' ' + %char(%size(nv)) + ' ' + %char(%size(nvs))
  + ' ' + %char(%size(rl)) + ' ' + %char(%size(pc)) + ' ' + %char(%size(di))
  + ' ' + %char(%size(de)) + ' ' + %char(%size(dg)) + ' ' + %char(%size(ge))
  + ' ' + %char(%size(QrnDgDs_T)) + ' ' + %char(%size(QrnDgArray_T))
  + ' ' + %char(%size(QrnDgScalar_T)) + ' ' + %char(%size(QrnObject_T));
write QSYSPRT line;
b = %addr(oa);
line = 'oa ' + %char(%addr(oa.userArea) - b) + ' ' + %char(%addr(oa.namesValues) - b)
  + ' ' + %char(%addr(oa.reserved4) - b) + ' ' + %char(%addr(oa.externalFile) - b)
  + ' ' + %char(%addr(oa.recordName) - b) + ' ' + %char(%addr(oa.rpgOperation) - b)
  + ' ' + %char(%addr(oa.alphaCcsids) - b) + ' ' + %char(%addr(oa.functionKey) - b)
  + ' ' + %char(%addr(oa.useNamesValues) - b) + ' ' + %char(%addr(oa.rpgDevice) - b)
  + ' ' + %char(%addr(oa.reserved10) - b);
write QSYSPRT line;
b = %addr(nv);
line = 'nv ' + %char(%addr(nv.dataType) - b) + ' ' + %char(%addr(nv.hasNullValue) - b)
  + ' ' + %char(%addr(nv.valueLenBytes) - b) + ' ' + %char(%addr(nv.value) - b)
  + ' ' + %char(%addr(nv.reserved3) - b);
write QSYSPRT line;
p = %alloc(1000);
b = %addr(di);
line = 'di ' + %char(%addr(di.env) - b) + ' ' + %char(%addr(di.dataLen) - b)
  + ' ' + %char(%addr(di.userParmIsNullTermString) - b);
write QSYSPRT line;
b = %addr(de);
line = 'de ' + %char(%addr(de.reportName) - b) + ' ' + %char(%addr(de.trace) - b)
  + ' ' + %char(%addr(de.reportAttrCcsid) - b);
write QSYSPRT line;
b = %addr(dg);
line = 'dg ' + %char(%addr(dg.userParm) - b) + ' ' + %char(%addr(dg.userParmSize) - b)
  + ' ' + %char(%addr(dg.userParmType) - b) + ' ' + %char(%addr(dg.isPartOfSequence) - b)
  + ' ' + %char(%addr(dg.name) - b) + ' ' + %char(%addr(dg.event) - b)
  + ' ' + %char(%addr(dg.ds) - b) + ' ' + %char(%addr(dg.scalar) - b);
write QSYSPRT line;
line = 'dg2 ' + %char(%addr(dg.ds.extFile) - b) + ' ' + %char(%addr(dg.ds.recordLevelId) - b)
  + ' ' + %char(%addr(dg.scalar.dataType) - b) + ' ' + %char(%addr(dg.scalar.valueLenBytes) - b)
  + ' ' + %char(%addr(dg.scalar.value) - b) + ' ' + %char(%addr(dg.array.subfieldNumber) - b);
write QSYSPRT line;
b = %addr(ge);
line = 'ge ' + %char(%addr(ge.addTextString) - b) + ' ' + %char(%addr(ge.getSubfieldName) - b);
write QSYSPRT line;
*inlr = *on;
