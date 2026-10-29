# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# env: printing, -i, assignments, running utilities and scripts, statuses.

probe print 'env | sort | cut -d= -f1'
probe empty 'env -i; echo "[$?]"'
probe assign 'env FOO=bar env | grep FOO'
probe only 'env -i A=1 B=2 env'
probe replace 'env HOME=/tmp env | grep "^HOME="'
probe missing 'env nosuchcmd'
probe utility 'env basename /a/b'
probe path 'env -i /usr/bin/basename /x/y'
probe status 'env sh -c "exit 3"; echo $?'
probe script 'printf "echo X=\$X\n" > s.sh; chmod +x s.sh; env X=1 ./s.sh'
probe not-runnable 'echo > s.sh; chmod 644 s.sh; env ./s.sh'
finish
