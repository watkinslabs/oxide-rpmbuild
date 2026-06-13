#!/usr/bin/sh
set -e; cd "$(dirname "$0")"; V=./vendorctl/target/debug/vendorctl
$V pkg add curl 2>/dev/null||true; $V ver add --package curl --version 8.11.0 2>/dev/null||true
$V meta set curl --build-system autotools --license curl --summary "curl (oxide)" --build-requires "openssl zlib" \
  --build-args '--with-openssl=$SYS/usr --with-zlib=$SYS/usr --without-libpsl --without-brotli --without-zstd --without-nghttp2 --without-nghttp3 --without-ngtcp2 --without-libssh2 --without-libidn2 --without-librtmp --disable-ldap --disable-ldaps --disable-docs --prefix=/usr' >/dev/null
$V install clear curl 2>/dev/null||true
$V src add --package curl --version 8.11.0 --url https://curl.se/download/curl-8.11.0.tar.gz --filename curl-8.11.0.tar.gz 2>/dev/null||true
echo "registered curl"
