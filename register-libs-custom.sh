#!/usr/bin/sh
# Custom-build-system libs (script family): zlib (./configure), openssl (./Configure).
set -e; cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl
$V pkg add zlib 2>/dev/null||true; $V ver add --package zlib --version 1.3.1 2>/dev/null||true
$V meta set zlib --build-system script --license Zlib --summary "zlib -devel (oxide)" \
  --build-args 'CC="$CC" ./configure --prefix=/usr && make %{?_smp_mflags}' >/dev/null
$V src add --package zlib --version 1.3.1 --url https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz --filename zlib-1.3.1.tar.gz 2>/dev/null||true
$V pkg add openssl 2>/dev/null||true; $V ver add --package openssl --version 3.0.15 2>/dev/null||true
$V meta set openssl --build-system script --license Apache-2.0 --summary "OpenSSL -devel (oxide)" \
  --build-args 'unset CC; ./Configure linux-%{_target_cpu} --cross-compile-prefix=$CROSS shared no-tests no-module no-legacy --prefix=/usr --libdir=lib && make %{?_smp_mflags}' \
  --install-cmd 'make install_sw DESTDIR=%{buildroot}' >/dev/null
$V src add --package openssl --version 3.0.15 --url https://github.com/openssl/openssl/releases/download/openssl-3.0.15/openssl-3.0.15.tar.gz --filename openssl-3.0.15.tar.gz 2>/dev/null||true
echo "registered custom libs"
