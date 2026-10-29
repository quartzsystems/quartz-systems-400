# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# dirname: the POSIX algorithm, messages.

probe path 'dirname /usr/bin/ls; dirname file; dirname /usr/lib/'
probe root 'dirname /; dirname /usr; dirname //a//b//'
probe relative 'dirname a/b/c'
probe usage 'dirname; dirname a b'
finish
