# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.
#
# mkdir: modes, -p, -m, errors.

probe one 'mkdir d; ls -ld d'
probe several 'mkdir a b c; ls -d a b c'
probe umask 'umask 077; mkdir d; ls -ld d'
probe mode 'mkdir -m 700 d; ls -ld d'
probe mode-symbolic 'mkdir -m u=rwx,go=rx d; ls -ld d'
probe mode-bad 'mkdir -m zz d'
probe exists 'mkdir data'
probe no-parent 'mkdir x/y'
probe parents 'mkdir -p x/y/z; ls -R x'
probe parents-exist 'mkdir -p data/sub; echo $?'
probe through-file 'mkdir -p data/fruits.txt/x'
probe usage 'mkdir'
probe parents-mode 'mkdir -p -m 700 x/y; ls -ld x x/y'
finish
