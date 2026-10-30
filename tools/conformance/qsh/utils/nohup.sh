# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# nohup: statuses, nohup.out when standard output is a terminal (run interactively), messages.

probe runs 'nohup echo hi; echo $?'
probe status 'nohup sh -c '\''exit 3'\''; echo $?'
probe not-found 'nohup nosuchprog; echo $?'
probe not-runnable 'nohup data/fruits.txt; echo $?'
probe usage 'nohup; echo $?'
probe piped 'nohup echo x | cat; ls nohup.out'
finish
