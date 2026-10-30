# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# dspmsg: catalog lookup (path, NLSPATH, cwd), sets, substitution, defaults, statuses.

probe message 'printf '\''1 Hello\\n\n2 File %%s\\n\n3 %%2$s %%1$s\\n\n'\'' > m.msg; gencat m.cat m.msg; dspmsg ./m.cat 1; dspmsg ./m.cat 2 d x; dspmsg ./m.cat 3 d a b'
probe cwd-name 'printf '\''1 Hello\\n\n'\'' > m.msg; gencat m.cat m.msg; dspmsg m.cat 1; echo $?'
probe nlspath 'mkdir -p nls/en_US; printf '\''1 nls\\n\n'\'' > m.msg; gencat nls/en_US/app.cat m.msg; NLSPATH=$PWD/nls/%L/%N LANG=en_US dspmsg app.cat 1'
probe set 'printf '\''$set 2\n1 two\\n\n'\'' > m.msg; gencat s.cat m.msg; dspmsg -s 2 ./s.cat 1'
probe default 'dspmsg ./nosuch.cat 1 '\''Default %s\n'\'' arg; echo $?'
probe missing 'dspmsg ./nosuch.cat 1; echo $?'
probe percent 'dspmsg ./nosuch.cat 1 '\''100%% %d\n'\'''
probe newline 'dspmsg ./nosuch.cat 1 '\''no newline'\''; echo '\''|'\'''
probe usage 'dspmsg; dspmsg x.cat; dspmsg -s 0 x.cat 1; dspmsg x.cat y'
finish
