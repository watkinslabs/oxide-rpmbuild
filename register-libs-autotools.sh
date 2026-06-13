#!/usr/bin/sh
set -e; cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl
# key ver license "summary" "COMMON flags" url filename
reg() {
  $V pkg add "$1" 2>/dev/null || true
  $V ver add --package "$1" --version "$2" 2>/dev/null || true
  $V meta set "$1" --build-system autotools --license "$3" --summary "$4" --cflags "-fPIC" \
     --build-args "--prefix=/usr $5" >/dev/null
  $V install clear "$1" 2>/dev/null || true
  $V src add --package "$1" --version "$2" --url "$6" --filename "$7" 2>/dev/null || true
}
reg expat        2.6.2    "MIT"        "expat XML parser -devel (oxide)"   "--enable-shared --disable-static --without-docbook --without-examples --without-tests" https://github.com/libexpat/libexpat/releases/download/R_2_6_2/expat-2.6.2.tar.xz expat-2.6.2.tar.xz
reg pcre2        10.44    "BSD-3-Clause" "PCRE2 regex -devel (oxide)"      "--enable-shared --disable-static --disable-pcre2grep-jit" https://github.com/PCRE2Project/pcre2/releases/download/pcre2-10.44/pcre2-10.44.tar.bz2 pcre2-10.44.tar.bz2
reg libffi       3.4.6    "MIT"        "libffi -devel (oxide)"             "--enable-shared --disable-static" https://github.com/libffi/libffi/releases/download/v3.4.6/libffi-3.4.6.tar.gz libffi-3.4.6.tar.gz
reg libxcrypt    4.4.36   "LGPL-2.1-or-later" "libxcrypt -devel (oxide)"   "--enable-shared --disable-static --disable-werror --enable-hashes=glibc --enable-obsolete-api=glibc" https://github.com/besser82/libxcrypt/releases/download/v4.4.36/libxcrypt-4.4.36.tar.xz libxcrypt-4.4.36.tar.xz
reg libunistring 1.2      "LGPL-3.0-or-later" "libunistring -devel (oxide)" "--enable-shared --disable-static --disable-rpath" https://ftp.gnu.org/gnu/libunistring/libunistring-1.2.tar.gz libunistring-1.2.tar.gz
reg libgpg-error 1.50     "LGPL-2.1-or-later" "libgpg-error -devel (oxide)" "--enable-shared --disable-static --disable-doc --disable-tests" https://www.gnupg.org/ftp/gcrypt/libgpg-error/libgpg-error-1.50.tar.bz2 libgpg-error-1.50.tar.bz2
reg libseccomp   2.5.5    "LGPL-2.1-only" "libseccomp -devel (oxide)"      "--enable-shared --disable-static" https://github.com/seccomp/libseccomp/releases/download/v2.5.5/libseccomp-2.5.5.tar.gz libseccomp-2.5.5.tar.gz
reg acl          2.3.2    "LGPL-2.1-or-later" "acl -devel (oxide)"         "--enable-shared --disable-static --disable-nls --disable-rpath" https://download.savannah.nongnu.org/releases/acl/acl-2.3.2.tar.gz acl-2.3.2.tar.gz
reg attr         2.5.2    "LGPL-2.1-or-later" "attr -devel (oxide)"        "--enable-shared --disable-static --disable-nls --disable-rpath" https://download.savannah.nongnu.org/releases/attr/attr-2.5.2.tar.gz attr-2.5.2.tar.gz
reg kmod         31       "LGPL-2.1-or-later" "kmod -devel (oxide)"        "--enable-shared --disable-static --disable-tools --disable-manpages" https://www.kernel.org/pub/linux/utils/kernel/kmod/kmod-31.tar.xz kmod-31.tar.xz
echo registered

# ncurses: shared (.so) + acl (dynamic-links shared libattr from vendored attr tree)
$V pkg add ncurses 2>/dev/null || true; $V ver add --package ncurses --version 6.5 2>/dev/null || true
$V meta set ncurses --build-system autotools --license "X11" --summary "ncurses shared -devel (oxide)" --cflags "-fPIC" \
  --build-args "--prefix=/usr --with-shared --without-normal --without-debug --without-ada --without-cxx --without-cxx-binding --without-manpages --without-progs --without-tack --without-tests --enable-pc-files=no --disable-db-install --enable-widec --enable-overwrite --with-default-terminfo-dir=/usr/share/terminfo --with-terminfo-dirs=/etc/terminfo:/lib/terminfo:/usr/share/terminfo" >/dev/null
$V install clear ncurses 2>/dev/null || true
$V src add --package ncurses --version 6.5 --url https://ftp.gnu.org/gnu/ncurses/ncurses-6.5.tar.gz --filename ncurses-6.5.tar.gz 2>/dev/null || true
ATTR='/home/nd/oxide/oxide2/vendor/attr/install-%{_target_cpu}'
$V meta set acl --cflags "-fPIC -I$ATTR/include" --ldflags "-L$ATTR/lib" >/dev/null
