# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# pwd: the program form, -L and -P.

probe default 'cd data && /usr/bin/pwd'
probe link 'cd data && ln -s sub s && cd s && /usr/bin/pwd -L; /usr/bin/pwd -P; /usr/bin/pwd'
probe stale 'PWD=/tmp /usr/bin/pwd'
probe operand '/usr/bin/pwd x; echo $?'
probe option '/usr/bin/pwd -x; echo $?'
finish
