# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# pwdx: the shell's and a child's working directory, messages.

probe self 'pwdx $$'
probe child '(cd data && sleep 2) & sleep 1; pwdx $!; wait'
probe missing 'pwdx 999999; echo $?'
probe usage 'pwdx; echo $?; pwdx abc; echo $?; pwdx -x; echo $?'
finish
