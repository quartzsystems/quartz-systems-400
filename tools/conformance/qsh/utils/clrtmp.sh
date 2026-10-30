# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# clrtmp: messages only. Clearing /tmp is never run on a shared system.

probe bad-option 'clrtmp -x; echo $?'
probe operand 'clrtmp /tmp/none.$$; echo $?'
finish
