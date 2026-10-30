# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# mkfifo: modes with the umask and -m, messages.

probe default 'mkfifo p; ls -l p | cut -c1-10'
probe umask 'umask 027; mkfifo p2; ls -l p2 | cut -c1-10'
probe mode 'mkfifo -m 600 p3; mkfifo -m u=rw,go= p4; ls -l p3 p4 | cut -c1-10'
probe exists 'mkfifo data; echo $?'
probe no-dir 'mkfifo nodir/p; echo $?'
probe bad-mode 'mkfifo -m 999 p5; echo $?'
probe usage 'mkfifo; echo $?'
finish
