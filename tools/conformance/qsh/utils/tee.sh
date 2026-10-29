# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# tee: files, -a, -i, errors.

probe files 'echo hello | tee t1 t2; cat t1 t2'
probe append 'echo one > t; echo two | tee -a t; cat t'
probe none 'echo just | tee'
probe missing-dir 'echo x | tee nodir/f'
probe pipe 'printf "a\nb\n" | tee t | wc -l; cat t'
probe interrupts 'echo i | tee -i t; cat t'
finish
