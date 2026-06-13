#!/usr/bin/sh
set -e; cd "$(dirname "$0")"; V=./vendorctl/target/debug/vendorctl
at(){ $V pkg add "$1" 2>/dev/null||true; $V ver add --package "$1" --version "$2" 2>/dev/null||true
  $V meta set "$1" --build-system autotools --license "$3" --summary "$4 (oxide)" --cflags "-fPIC" --build-requires "$5" --build-args "$6" >/dev/null
  $V install clear "$1" 2>/dev/null||true; $V src add --package "$1" --version "$2" --url "$7" --filename "$8" 2>/dev/null||true; }
mk(){ $V pkg add "$1" 2>/dev/null||true; $V ver add --package "$1" --version "$2" 2>/dev/null||true
  $V meta set "$1" --build-system script --license "$3" --summary "$4 (oxide)" --build-args "$5" --install-cmd "$6" >/dev/null
  $V install clear "$1" 2>/dev/null||true; $V src add --package "$1" --version "$2" --url "$7" --filename "$8" 2>/dev/null||true; }
at libidn2 2.3.7 "LGPL-3.0-or-later" libidn2 libunistring "--prefix=/usr --enable-shared --disable-static --disable-doc --disable-nls --disable-rpath --with-libunistring-prefix=\$SYS/usr" https://ftp.gnu.org/gnu/libidn/libidn2-2.3.7.tar.gz libidn2-2.3.7.tar.gz
$V meta set libidn2 --ldflags "-lunistring" >/dev/null
at libgcrypt 1.10.3 "LGPL-2.1-or-later" libgcrypt libgpg-error "--prefix=/usr --enable-shared --disable-static --disable-doc --disable-asm --with-libgpg-error-prefix=\$SYS/usr" https://www.gnupg.org/ftp/gcrypt/libgcrypt/libgcrypt-1.10.3.tar.bz2 libgcrypt-1.10.3.tar.bz2
at libevent 2.1.12 "BSD-3-Clause" libevent "" "--prefix=/usr --enable-shared --disable-static --disable-openssl --disable-samples --disable-debug-mode" https://github.com/libevent/libevent/releases/download/release-2.1.12-stable/libevent-2.1.12-stable.tar.gz libevent-2.1.12.tar.gz
$V meta set libevent --src-subdir libevent-2.1.12-stable >/dev/null
mk lz4 1.9.4 "BSD-2-Clause" lz4 'make -C lib CC="$CC" CFLAGS="-O2 -fPIC" %{?_smp_mflags}' 'make -C lib install DESTDIR=%{buildroot} PREFIX=/usr' https://github.com/lz4/lz4/releases/download/v1.9.4/lz4-1.9.4.tar.gz lz4-1.9.4.tar.gz
mk zstd 1.5.6 "BSD-3-Clause" zstd 'make -C lib CC="$CC" CFLAGS="-O2 -fPIC" libzstd %{?_smp_mflags}' 'make -C lib install DESTDIR=%{buildroot} PREFIX=/usr' https://github.com/facebook/zstd/releases/download/v1.5.6/zstd-1.5.6.tar.gz zstd-1.5.6.tar.gz
mk libcap 2.69 "BSD-3-Clause OR GPL-2.0-only" libcap 'make -C libcap CC="$CC" BUILD_CC=gcc AR="${CROSS}ar" RANLIB="${CROSS}ranlib" OBJCOPY="${CROSS}objcopy" SHARED=yes %{?_smp_mflags}' 'make -C libcap install DESTDIR=%{buildroot} prefix=/usr lib=lib SHARED=yes RAISE_SETFCAP=no' https://www.kernel.org/pub/linux/libs/security/linux-privs/libcap2/libcap-2.69.tar.xz libcap-2.69.tar.xz
echo "registered libs2"
