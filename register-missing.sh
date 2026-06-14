#!/bin/sh
# Register the vendor packages not covered by the other register-*.sh scripts.
# Shared+dynamic, Bootlin gcc-14/musl-1.2.5 toolchain, features enabled.
# Built from PRISTINE upstream tarballs (vendorctl src fetch verifies sha256) —
# never from the in-place vendor trees, which carry stale build artifacts.
# (doom = DOOM1 shareware WAD data, not a source package; python + systemd
#  are tracked separately as the large remaining tail.)
set -e; cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl

reg() { # key version build-system license summary
  $V pkg add "$1" 2>/dev/null || true
  $V ver add --package "$1" --version "$2" 2>/dev/null || true
  $V meta set "$1" --build-system "$3" --license "$4" --summary "$5"
  $V install clear "$1" 2>/dev/null || true
}
# src URL only (no --filename) → Source0 defaults to <key>-<ver>.tar.gz, which is
# what `src fetch` writes the downloaded content to (rpm %setup content-detects
# the real compression). %setup -n is pinned via --src-subdir when the upstream
# tarball's top dir differs from <key>-<ver>.

# --- zip / unzip (Info-ZIP, bespoke unix/Makefile) ---
# Invoke the zips/unzips target DIRECTLY: the `generic` target runs unix/configure
# which RUNS a probe binary (impossible under cross) and wrongly adds -DNO_DIR.
reg zip 3.0 script "Info-ZIP" "Info-ZIP zip (oxide)"
$V meta set zip --src-subdir zip30 --cflags '-std=gnu89' \
  --build-args 'make -f unix/Makefile zips CC="$CC" AS="$CC -c" LD="$CC" CFLAGS="-O2 -I. -DUNIX -DLARGE_FILE_SUPPORT -DUIDGID_NOT_16BIT -DNO_LCHMOD -std=gnu89"' \
  --install-cmd 'for b in zip zipnote zipcloak zipsplit; do install -Dm755 $b %{buildroot}/usr/bin/$b; done'
$V src add --package zip --version 3.0 --url https://downloads.sourceforge.net/infozip/zip30.tar.gz 2>/dev/null || true

reg unzip 6.0 script "Info-ZIP" "Info-ZIP unzip (oxide)"
$V meta set unzip --src-subdir unzip60 --cflags '-std=gnu89' \
  --build-args 'make -f unix/Makefile unzips CC="$CC" LD="$CC" CFLAGS="-O2 -Wall -DUNIX -DLARGE_FILE_SUPPORT -DNO_LCHMOD -std=gnu89"' \
  --install-cmd 'for b in unzip funzip unzipsfx; do install -Dm755 $b %{buildroot}/usr/bin/$b; done'
$V src add --package unzip --version 6.0 --url https://downloads.sourceforge.net/infozip/unzip60.tar.gz 2>/dev/null || true

# --- entr (custom configure → Makefile) ---
reg entr 5.6 script "ISC" "entr — run commands on file change (oxide)"
$V meta set entr \
  --build-args 'TARGET_OS=Linux ./configure && make CC="$CC" CFLAGS="$CFLAGS" LDFLAGS="$LDFLAGS"' \
  --install-cmd 'make install PREFIX=/usr DESTDIR=%{buildroot}'
$V src add --package entr --version 5.6 --url https://eradman.com/entrproject/code/entr-5.6.tar.gz 2>/dev/null || true

# --- dhcpcd (custom configure, self-contained) ---
reg dhcpcd 10.3.2 script "BSD-2-Clause" "dhcpcd DHCP client (oxide)"
$V meta set dhcpcd \
  --build-args './configure --build=$(gcc -dumpmachine) --host=$TRIPLE --prefix=/usr --sysconfdir=/etc --dbdir=/var/db/dhcpcd --libexecdir=/usr/lib/dhcpcd --without-udev && make %{?_smp_mflags}' \
  --install-cmd 'make install DESTDIR=%{buildroot}'
$V src add --package dhcpcd --version 10.3.2 --url https://github.com/NetworkConfiguration/dhcpcd/releases/download/v10.3.2/dhcpcd-10.3.2.tar.xz 2>/dev/null || true

# --- iproute2 (custom configure + make) ---
# HOSTCC=gcc so netem's distribution-table generators build+run on the HOST.
reg iproute2 6.10.0 script "GPL-2.0-or-later" "iproute2 — ip/tc/bridge (oxide)"
$V meta set iproute2 --cflags '-D_GNU_SOURCE -Wno-implicit-function-declaration -Wno-incompatible-pointer-types' \
  --build-args './configure && make CC="$CC" HOSTCC=gcc %{?_smp_mflags}' \
  --install-cmd 'make install DESTDIR=%{buildroot} HOSTCC=gcc'
$V src add --package iproute2 --version 6.10.0 --url https://www.kernel.org/pub/linux/utils/net/iproute2/iproute2-6.10.0.tar.xz 2>/dev/null || true

# --- dropbear (autotools; --disable-harden: its harden flags clash on gcc-14) ---
reg dropbear 2024.86 autotools "MIT" "Dropbear SSH server+client (oxide)"
$V meta set dropbear --build-requires 'zlib' \
  --build-args '--prefix=/usr --disable-harden --disable-lastlog --disable-wtmp --disable-utmp --disable-wtmpx --disable-utmpx --enable-zlib'
$V src add --package dropbear --version 2024.86 --url https://matt.ucc.asn.au/dropbear/releases/dropbear-2024.86.tar.bz2 --filename dropbear-2024.86.tar.bz2 2>/dev/null || true

# --- iputils (meson) ---
reg iputils 20240117 meson "BSD-3-Clause" "iputils — ping/arping/tracepath (oxide)"
$V meta set iputils --build-requires 'libcap' \
  --build-args '-DBUILD_MANS=false -DBUILD_HTML_MANS=false -DSKIP_TESTS=true -DUSE_CAP=true -DUSE_IDN=false -DINSTALL_SYSTEMD_UNITS=false'
$V src add --package iputils --version 20240117 --url https://github.com/iputils/iputils/archive/refs/tags/20240117.tar.gz 2>/dev/null || true

# --- dbus (autotools; needs expat) ---
reg dbus 1.14.10 autotools "GPL-2.0-or-later OR AFL-2.1" "D-Bus message bus (oxide)"
$V meta set dbus --build-requires 'expat' \
  --build-args '--prefix=/usr --localstatedir=/var --sysconfdir=/etc --runstatedir=/run --disable-systemd --disable-selinux --disable-tests --disable-doxygen-docs --disable-xml-docs --disable-ducktype-docs --without-x'
$V src add --package dbus --version 1.14.10 --url https://dbus.freedesktop.org/releases/dbus/dbus-1.14.10.tar.xz 2>/dev/null || true

echo "registered missing packages (zip unzip entr dhcpcd iproute2 dropbear iputils dbus)"
