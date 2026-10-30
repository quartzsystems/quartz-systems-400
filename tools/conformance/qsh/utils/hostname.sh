# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# hostname: the TCP/IP host name, -s, setting refused, messages.

probe default 'hostname'
probe short 'hostname -s'
probe env 'echo "$HOSTNAME"; hostname'
probe operand 'hostname newname; echo $?'
probe bad-option 'hostname -x; echo $?'
finish
