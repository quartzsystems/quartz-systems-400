# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# printenv: all, names, unset names and the status.

probe all 'env -i A=1 B=two /usr/bin/printenv'
probe names 'A=1 B=2 printenv A B; echo $?'
probe unset 'printenv NOSUCH; echo $?'
probe option 'printenv -x; echo $?'
finish
