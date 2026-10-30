# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# rexec: options and messages only; no login is tried on the system.

probe usage 'rexec; echo $?'
probe no-command 'rexec localhost; echo $?'
probe bad-option 'rexec -z localhost ls; echo $?'
probe no-password 'HOME=$PWD rexec -u nobody localhost ls </dev/null; echo $?'
probe refused 'rexec -u nobody -p x -P 1 127.0.0.1 ls; echo $?'
finish
