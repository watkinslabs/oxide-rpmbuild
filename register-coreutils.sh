#!/usr/bin/sh
set -e; cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl
$V pkg add coreutils 2>/dev/null || true
$V ver add --package coreutils --version 8.32 2>/dev/null || true
$V meta set coreutils --build-system autotools --license "GPL-3.0-or-later" --summary "GNU coreutils (static-musl, oxide)" \
  --cflags "-DO_BINARY=0 -DO_TEXT=0 -DS_IXUGO='(S_IXUSR|S_IXGRP|S_IXOTH)' -DS_IRWXUGO='(S_IRWXU|S_IRWXG|S_IRWXO)' -DSYS_getdents=SYS_getdents64" \
  --config-cache "$(printf 'ac_cv_header_error_h=no\nac_cv_have_decl_error=no\nac_cv_have_decl_error_at_line=no\nac_cv_func_error=no')" \
  --build-args "--enable-single-binary=symlinks --enable-no-install-program=stdbuf,arch,hostname --disable-nls --disable-libsmack --disable-libcap --disable-acl --disable-xattr --without-selinux --without-openssl --prefix=/usr" >/dev/null
$V install clear coreutils 2>/dev/null || true
$V src add --package coreutils --version 8.32 --url https://ftp.gnu.org/gnu/coreutils/coreutils-8.32.tar.xz --filename coreutils-8.32.tar.xz 2>/dev/null || true
echo "registered coreutils"
