# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# liblist: the display form, adding, removing, the current library,
# messages. Changes stay in the probe's job.

probe show 'liblist'
probe add 'liblist -a QTEMP; echo $?; liblist -a QSYS2; echo $?'
probe add-new 'liblist -a QGPL; liblist -d QGPL; echo $?; liblist -a QGPL; echo $?; liblist'
probe last 'liblist -d QGPL; liblist -l QGPL; liblist | tail -2'
probe remove-missing 'liblist -d NOSUCHLIB; echo $?'
probe current 'liblist -c QGPL; echo $?; liblist | grep -i cur'
probe bad-name 'liblist -a 1BAD; echo $?'
probe usage 'liblist -z; echo $?; liblist QGPL; echo $?'
finish
