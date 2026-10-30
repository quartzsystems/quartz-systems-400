# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# id: the default form (gid without a group profile), -u -g -G -n, other
# users, messages.

probe default 'id'
probe u 'id -u; id -un'
probe g 'id -g; id -gn; echo $?'
probe G 'id -G; id -Gn'
probe qsys 'id QSYS; id -u qsys'
probe unknown 'id nosuchusr; echo $?'
probe usage 'id -n; echo $?; id -u -g; echo $?; id -x; echo $?'
finish
