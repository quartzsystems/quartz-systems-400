# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# sleep: operands and messages.

probe zero 'sleep 0; echo $?'
probe one 'sleep 1; echo $?'
probe bad 'sleep abc'
probe usage 'sleep'
probe negative 'sleep -1'
probe fraction 'sleep 0.5; echo $?'
finish
