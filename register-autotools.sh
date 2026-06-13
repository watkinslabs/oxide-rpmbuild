#!/usr/bin/sh
# Register the autotools single-binary GNU tools cluster into the vendorctl
# catalog. Idempotent-ish: re-running re-adds (pkg add will error if exists —
# ignore). build-args = configure flags (template supplies --host).
set -e
cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl
GPL="GPL-3.0-or-later"

reg() { # key version license summary build_args
    $V pkg add "$1" 2>/dev/null || true
    $V ver add --package "$1" --version "$2" 2>/dev/null || true
    $V meta set "$1" --build-system autotools --license "$3" --summary "$4" --build-args "$5"
    $V install clear "$1" 2>/dev/null || true
}

reg grep      3.11    "$GPL" "GNU grep (static-musl, oxide)" \
    "--disable-nls --disable-perl-regexp --without-libsigsegv-prefix --with-included-regex --prefix=/usr"
$V install add grep --dest /usr/bin/grep --src src/grep --kind bin

reg tar       1.35    "$GPL" "GNU tar (static-musl, oxide)" \
    "--disable-nls --disable-acl --disable-xattr --without-selinux --without-posix-acls --prefix=/usr"
$V install add tar --dest /usr/bin/tar --src src/tar --kind bin

reg make      4.4.1   "$GPL" "GNU make (static-musl, oxide)" \
    "--disable-nls --without-guile --without-libsigsegv --prefix=/usr"
$V install add make --dest /usr/bin/make --src make --kind bin

reg gawk      5.3.1   "$GPL" "GNU awk (static-musl, oxide)" \
    "--disable-nls --without-mpfr --without-readline --disable-extensions --prefix=/usr"
$V install add gawk --dest /usr/bin/gawk --src gawk --kind bin
$V install add gawk --dest /usr/bin/awk --kind hardlink --link-target /usr/bin/gawk

reg gzip      1.13    "$GPL" "GNU gzip (static-musl, oxide)" \
    "--disable-nls --prefix=/usr"
$V install add gzip --dest /usr/bin/gzip --src gzip --kind bin
$V install add gzip --dest /usr/bin/gunzip --kind hardlink --link-target /usr/bin/gzip

reg xz        5.6.3   "0BSD AND GPL-2.0-or-later" "XZ Utils (static-musl, oxide)" \
    "--disable-nls --disable-shared --enable-static --disable-doc --disable-scripts --disable-lzma-links --prefix=/usr"
$V install add xz --dest /usr/bin/xz --src src/xz/xz --kind bin

reg patch     2.7.6   "$GPL" "GNU patch (static-musl, oxide)" \
    "--disable-nls --prefix=/usr"
$V install add patch --dest /usr/bin/patch --src src/patch --kind bin

reg diffutils 3.10    "$GPL" "GNU diffutils (static-musl, oxide)" \
    "--disable-nls --without-selinux --prefix=/usr"
$V install add diffutils --dest /usr/bin/diff --src src/diff --kind bin
$V install add diffutils --dest /usr/bin/cmp  --src src/cmp  --kind bin

reg findutils 4.10.0  "$GPL" "GNU findutils (static-musl, oxide)" \
    "--disable-nls --without-selinux --prefix=/usr"
$V install add findutils --dest /usr/bin/find  --src find/find   --kind bin
$V install add findutils --dest /usr/bin/xargs --src xargs/xargs --kind bin

echo "registered autotools cluster"
