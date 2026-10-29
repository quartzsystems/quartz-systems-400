# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# rmdir: empty directories, -p, errors.

probe empty 'mkdir d; rmdir d; ls -d d'
probe not-empty 'rmdir data/sub'
probe missing 'rmdir nosuch'
probe file 'rmdir data/fruits.txt'
probe parents 'mkdir -p x/y/z; rmdir -p x/y/z; ls -d x'
probe parents-stop 'mkdir -p x/y/z x/k; rmdir -p x/y/z; ls x'
probe several 'mkdir a b; rmdir a b; ls -d a b'
probe usage 'rmdir'
finish
