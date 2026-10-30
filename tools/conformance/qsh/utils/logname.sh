# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# logname: the user profile name, messages.

probe default 'logname'
probe compare 'test "$(logname)" = "$LOGNAME" && echo same'
probe operand 'logname x; echo $?'
finish
