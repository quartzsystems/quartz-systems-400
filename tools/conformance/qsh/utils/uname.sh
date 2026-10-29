# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# uname: each field and their order, messages.

probe default 'uname'
probe fields 'uname -s; uname -n; uname -r; uname -v; uname -m'
probe all 'uname -a'
probe pair 'uname -sn; uname -rv'
probe operand 'uname x'
probe bad-option 'uname -p'
finish
