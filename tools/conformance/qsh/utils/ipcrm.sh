# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# ipcrm: messages for objects that do not exist; nothing is removed.

probe missing 'ipcrm -q 999999; echo $?; ipcrm -m 999999; echo $?; ipcrm -s 999999; echo $?'
probe missing-key 'ipcrm -Q 0x7fff1234; echo $?; ipcrm -M 0; echo $?'
probe bad-number 'ipcrm -q abc; echo $?'
probe usage 'ipcrm; echo $?; ipcrm 5; echo $?'
finish
