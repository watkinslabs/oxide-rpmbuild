#!/bin/sh
# Native in-VM build toolchain (self-hosting foundation).
# Goal: compile/assemble/link IN the running oxide VM, not just cross from the host.
# musl-dev + binutils are the foundation; gcc/g++ + autotools glue layer on top.
set -e; cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl

# --- musl-dev: headers (musl + kernel UAPI) + static libs + crt, staged from the
# cross-toolchain sysroot. build-system 'stage' = no source/compile, just a file copy
# with $TCSYS (per-arch toolchain sysroot) exposed by vendorctl. ---
$V pkg add musl-dev 2>/dev/null || true
$V ver add --package musl-dev --version 1.2.5 2>/dev/null || true
$V meta set musl-dev --build-system stage --license MIT --summary "musl libc headers + static libs + crt (oxide dev)" \
  --install-cmd 'mkdir -p %{buildroot}/usr/include %{buildroot}/usr/lib; cp -a $TCSYS/usr/include/. %{buildroot}/usr/include/; for f in crt1.o crti.o crtn.o Scrt1.o rcrt1.o libc.a libc.so libm.a libpthread.a libdl.a librt.a libcrypt.a libresolv.a libutil.a libxnet.a; do cp -a $TCSYS/lib/$f %{buildroot}/usr/lib/ 2>/dev/null || true; done'

# --- binutils: native as/ld/ar/nm/objcopy/strip/... (--host+--target = oxide target). ---
$V pkg add binutils 2>/dev/null || true
$V ver add --package binutils --version 2.43.1 2>/dev/null || true
$V meta set binutils --build-system autotools --license "GPL-3.0-or-later" --summary "GNU binutils — as/ld/ar (oxide native)" \
  --build-requires 'zlib' \
  --build-args '--prefix=/usr --target=$TRIPLE --disable-nls --disable-werror --disable-multilib --disable-gprofng --enable-deterministic-archives --with-system-zlib'
$V src add --package binutils --version 2.43.1 --url https://mirrors.kernel.org/gnu/binutils/binutils-2.43.1.tar.xz 2>/dev/null || true

# --- autotools glue: m4, bison, flex, pkgconf (run configure scripts in the VM) ---
reg(){ $V pkg add "$1" 2>/dev/null||true; $V ver add --package "$1" --version "$2" 2>/dev/null||true; $V meta set "$1" --build-system autotools --license "$3" --summary "$4"; }
reg m4 1.4.19 "GPL-3.0-or-later" "GNU m4 macro processor (oxide)"
$V meta set m4 --build-args '--prefix=/usr --disable-nls'
$V src add --package m4 --version 1.4.19 --url https://mirrors.kernel.org/gnu/m4/m4-1.4.19.tar.xz 2>/dev/null||true
reg bison 3.8.2 "GPL-3.0-or-later" "GNU bison parser generator (oxide)"
$V meta set bison --build-args '--prefix=/usr --disable-nls'
$V src add --package bison --version 3.8.2 --url https://mirrors.kernel.org/gnu/bison/bison-3.8.2.tar.xz 2>/dev/null||true
# flex: -std=gnu89 so its bundled gnulib malloc.c (K&R decl) survives gcc-14.
reg flex 2.6.4 "BSD-2-Clause" "flex lexical analyzer (oxide)"
$V meta set flex --build-args '--prefix=/usr --disable-nls' --cflags '-std=gnu89'
$V src add --package flex --version 2.6.4 --url https://github.com/westes/flex/releases/download/v2.6.4/flex-2.6.4.tar.gz 2>/dev/null||true
reg pkgconf 2.3.0 "ISC" "pkgconf (pkg-config) (oxide)"
$V meta set pkgconf --build-args '--prefix=/usr'
$V src add --package pkgconf --version 2.3.0 --url https://distfiles.ariadne.space/pkgconf/pkgconf-2.3.0.tar.xz 2>/dev/null||true

# --- gcc + g++ (native: --host+--target = target) and its math deps gmp/mpfr/mpc ---
# NOTE: gcc must NOT build-require binutils — the BUILD uses the Bootlin cross binutils;
# staging our target binutils would put a target `as` on PATH and break host build tools.
reg gmp 6.3.0 "LGPL-3.0-or-later" "GNU MP bignum library (oxide)"
$V meta set gmp --build-args '--prefix=/usr --enable-cxx'
$V src add --package gmp --version 6.3.0 --url https://mirrors.kernel.org/gnu/gmp/gmp-6.3.0.tar.xz 2>/dev/null||true
reg mpfr 4.2.1 "LGPL-3.0-or-later" "GNU MPFR float library (oxide)"
$V meta set mpfr --build-requires 'gmp' --build-args '--prefix=/usr --with-gmp=$SYS/usr'
$V src add --package mpfr --version 4.2.1 --url https://mirrors.kernel.org/gnu/mpfr/mpfr-4.2.1.tar.xz 2>/dev/null||true
reg mpc 1.3.1 "LGPL-3.0-or-later" "GNU MPC complex math library (oxide)"
$V meta set mpc --build-requires 'gmp mpfr' --build-args '--prefix=/usr --with-gmp=$SYS/usr --with-mpfr=$SYS/usr'
$V src add --package mpc --version 1.3.1 --url https://mirrors.kernel.org/gnu/mpc/mpc-1.3.1.tar.gz 2>/dev/null||true
reg gcc 14.2.0 "GPL-3.0-or-later" "GNU Compiler Collection — gcc/g++ (oxide native)"
$V meta set gcc --build-requires 'gmp mpfr mpc zlib musl-dev' \
  --build-args '--prefix=/usr --target=$TRIPLE --enable-languages=c,c++ --disable-multilib --disable-nls --disable-bootstrap --disable-libsanitizer --disable-libssp --disable-werror --with-gmp=$SYS/usr --with-mpfr=$SYS/usr --with-mpc=$SYS/usr --enable-shared --enable-threads=posix --enable-__cxa_atexit --enable-default-pie MAKEINFO=true'
$V src add --package gcc --version 14.2.0 --url https://mirrors.kernel.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.xz 2>/dev/null||true

echo "registered native toolchain (musl-dev binutils m4 bison flex pkgconf gmp mpfr mpc gcc)"

# ============ in-VM dev languages + autotools suite (self-hosting) ============
# python (cross: host-python for --with-build-python + cross-configure)
$V pkg add python 2>/dev/null||true; $V ver add --package python --version 3.13.1 2>/dev/null||true
$V meta set python --build-system script --license "PSF-2.0" --summary "Python 3.13 (oxide)" \
  --src-subdir Python-3.13.1 --build-requires 'openssl zlib libffi expat bzip2 xz' \
  --build-args 'mkdir -p build-host && ( cd build-host && CC=gcc CXX=g++ ../configure -q >/dev/null && make -s -j$(nproc) python )
HOSTPY=$(pwd)/build-host/python
./configure --build=x86_64-pc-linux-gnu --host=$TRIPLE --with-build-python=$HOSTPY --prefix=/usr --disable-shared --without-ensurepip --with-openssl=$SYS/usr ac_cv_file__dev_ptmx=no ac_cv_file__dev_ptc=no ac_cv_buggy_getaddrinfo=no
make %{?_smp_mflags}' --install-cmd 'make install DESTDIR=%{buildroot}'
$V src add --package python --version 3.13.1 --url https://www.python.org/ftp/python/3.13.1/Python-3.13.1.tar.xz 2>/dev/null||true

# cmake (cross via host cmake + oxide toolchain file)
$V pkg add cmake 2>/dev/null||true; $V ver add --package cmake --version 3.31.6 2>/dev/null||true
$V meta set cmake --build-system script --license "BSD-3-Clause" --summary "CMake build system (oxide)" \
  --src-subdir cmake-3.31.6 --build-requires 'zlib' \
  --build-args 'if [ "%{_target_cpu}" = "aarch64" ]; then CMP=aarch64; else CMP=x86_64; fi
cat > oxide-tc.cmake <<TC
set(CMAKE_SYSTEM_NAME Linux)
set(CMAKE_SYSTEM_PROCESSOR $CMP)
set(CMAKE_C_COMPILER $CC)
set(CMAKE_CXX_COMPILER $CXX)
set(CMAKE_FIND_ROOT_PATH "$SYS;$TCSYS")
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
TC
cmake -S . -B _b -DCMAKE_TOOLCHAIN_FILE=$PWD/oxide-tc.cmake -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr -DCMAKE_USE_OPENSSL=OFF -DBUILD_TESTING=OFF -DBUILD_CursesDialog=OFF
cmake --build _b -j$(nproc)' --install-cmd 'DESTDIR=%{buildroot} cmake --install _b'
$V src add --package cmake --version 3.31.6 --url https://github.com/Kitware/CMake/releases/download/v3.31.6/cmake-3.31.6.tar.gz 2>/dev/null||true

# perl (cross via perl-cross overlay — Source1 multi-source feature)
$V pkg add perl 2>/dev/null||true; $V ver add --package perl --version 5.40.0 2>/dev/null||true
$V meta set perl --build-system script --license "Artistic-1.0-Perl OR GPL-1.0-or-later" --summary "Perl 5 (oxide)" \
  --src-subdir perl-5.40.0 \
  --build-args 'cp -a perl-cross-1.6/* . && ./configure --target=$TRIPLE --prefix=/usr -Dcc="$CC" -Accflags=-D_GNU_SOURCE -Dusethreads && make %{?_smp_mflags}' \
  --install-cmd 'make DESTDIR=%{buildroot} install'
$V src add --package perl --version 5.40.0 --url https://www.cpan.org/src/5.0/perl-5.40.0.tar.gz 2>/dev/null||true
$V src add --package perl --version 5.40.0 --url https://github.com/arsv/perl-cross/releases/download/1.6/perl-cross-1.6.tar.gz 2>/dev/null||true

# autoconf / automake / libtool (need perl + m4)
$V pkg add autoconf 2>/dev/null||true; $V ver add --package autoconf --version 2.72 2>/dev/null||true
$V meta set autoconf --build-system autotools --license "GPL-3.0-or-later" --summary "GNU autoconf (oxide)" --build-requires 'perl m4' --build-args '--prefix=/usr'
$V src add --package autoconf --version 2.72 --url https://mirrors.kernel.org/gnu/autoconf/autoconf-2.72.tar.xz 2>/dev/null||true
$V pkg add automake 2>/dev/null||true; $V ver add --package automake --version 1.17 2>/dev/null||true
$V meta set automake --build-system autotools --license "GPL-2.0-or-later" --summary "GNU automake (oxide)" --build-requires 'perl autoconf' --build-args '--prefix=/usr'
$V src add --package automake --version 1.17 --url https://mirrors.kernel.org/gnu/automake/automake-1.17.tar.xz 2>/dev/null||true
$V pkg add libtool 2>/dev/null||true; $V ver add --package libtool --version 2.5.4 2>/dev/null||true
$V meta set libtool --build-system autotools --license "GPL-3.0-or-later" --summary "GNU libtool (oxide)" --build-requires 'm4' --build-args '--prefix=/usr'
$V src add --package libtool --version 2.5.4 --url https://mirrors.kernel.org/gnu/libtool/libtool-2.5.4.tar.xz 2>/dev/null||true

# go (native: host-go bootstrap + cross-install target toolchain)
$V pkg add go 2>/dev/null||true; $V ver add --package go --version 1.23.4 2>/dev/null||true
$V meta set go --build-system script --license "BSD-3-Clause" --summary "Go toolchain (oxide native)" --src-subdir go \
  --build-args 'if [ "%{_target_cpu}" = "aarch64" ]; then GA=arm64; else GA=amd64; fi
cd src && GOROOT_BOOTSTRAP=/home/nd/oxide/oxide2/vendor/go CGO_ENABLED=0 GOROOT_FINAL=/usr/lib/go ./make.bash && cd .. && GOOS=linux GOARCH=$GA CGO_ENABLED=0 ./bin/go install -a std cmd' \
  --install-cmd 'if [ "%{_target_cpu}" = "aarch64" ]; then GA=arm64; else GA=amd64; fi
mkdir -p %{buildroot}/usr/lib/go %{buildroot}/usr/bin
cp -a bin pkg src lib api go.env VERSION %{buildroot}/usr/lib/go/ 2>/dev/null
if [ -d %{buildroot}/usr/lib/go/bin/linux_$GA ]; then rm -f %{buildroot}/usr/lib/go/bin/go %{buildroot}/usr/lib/go/bin/gofmt; mv %{buildroot}/usr/lib/go/bin/linux_$GA/* %{buildroot}/usr/lib/go/bin/; rmdir %{buildroot}/usr/lib/go/bin/linux_$GA; fi
ln -sf ../lib/go/bin/go %{buildroot}/usr/bin/go; ln -sf ../lib/go/bin/gofmt %{buildroot}/usr/bin/gofmt'
$V src add --package go --version 1.23.4 --url https://go.dev/dl/go1.23.4.src.tar.gz 2>/dev/null||true

# rust (cross-bootstrap: rustc+cargo that RUN on musl target; crt-static=false for proc-macros)
$V pkg add rust 2>/dev/null||true; $V ver add --package rust --version 1.84.0 2>/dev/null||true
$V meta set rust --build-system script --license "MIT OR Apache-2.0" --summary "Rust toolchain (oxide native)" \
  --src-subdir rustc-1.84.0-src \
  --build-args 'if [ "%{_target_cpu}" = "aarch64" ]; then RT=aarch64-unknown-linux-musl; else RT=x86_64-unknown-linux-musl; fi
cat > config.toml <<EOF2
[llvm]
download-ci-llvm = false
[build]
build = "x86_64-unknown-linux-gnu"
host = ["$RT"]
target = ["$RT"]
docs = false
extended = true
tools = ["cargo"]
[install]
prefix = "/usr"
[rust]
channel = "stable"
download-rustc = false
[target.$RT]
cc = "$CC"
cxx = "$CXX"
linker = "$CC"
ar = "${CROSS}ar"
ranlib = "${CROSS}ranlib"
musl-root = "$TCSYS"
crt-static = false
[target.x86_64-unknown-linux-gnu]
cc = "gcc"
cxx = "g++"
EOF2
python3 x.py build --stage 2' --install-cmd 'DESTDIR=%{buildroot} python3 x.py install'
$V src add --package rust --version 1.84.0 --url https://static.rust-lang.org/dist/rustc-1.84.0-src.tar.gz 2>/dev/null||true

echo "registered full dev system (+ python cmake perl autoconf automake libtool go rust)"
