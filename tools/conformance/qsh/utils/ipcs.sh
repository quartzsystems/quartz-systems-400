# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# ipcs: the heading, sections and column sets. Read-only.

probe default 'ipcs | head -12'
probe q 'ipcs -q | head -5'
probe m 'ipcs -m | head -5'
probe s 'ipcs -s | head -5'
probe all 'ipcs -a | head -12'
probe columns 'for o in b c o p t; do ipcs -q$o | head -3; done'
probe usage 'ipcs x; echo $?; ipcs -z; echo $?'
finish
