**FREE
//; cmd: CRTSQLRPGI OBJ({lib}/{name}) SRCFILE({lib}/QRPGLESRC) COMMIT(*NONE)
//; cmd+: OUTPUT(*PRINT) DBGVIEW(*NONE)
// The machine the timings ran on: model, processor feature, processors.
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s model char(20);
dcl-s feat char(20);
dcl-s cpus int(10);
dcl-s rel char(20);
exec sql select current_character_value into :model from qsys2.system_value_info
  where system_value_name = 'QMODEL';
exec sql select current_character_value into :feat from qsys2.system_value_info
  where system_value_name = 'QPRCFEAT';
exec sql select configured_cpus, os_version concat '.' concat os_release into :cpus, :rel
  from sysibmadm.env_sys_info;
line = 'model ' + model + ' feature ' + feat + ' cpus ' + %char(cpus) + ' os ' + rel;
write QSYSPRT line;
*inlr = *on;
