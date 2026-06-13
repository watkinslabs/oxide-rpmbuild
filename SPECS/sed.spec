# oxide vendor package — autotools family.
# From-source: %build configure --host=<arch>-linux-musl + make, CC by %%{_target_cpu}.
%global debug_package %{nil}
# neutralize rpm's injected Fedora hardening CFLAGS — they break musl cross.
%global __global_compiler_flags %{nil}
%global cross_arm /home/nd/oxide/oxide2/vendor/cross/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc

Name:           sed
Version:        4.9
Release:        1%{?dist}
Summary:        GNU stream editor (static-musl, oxide)
License:        GPL-3.0-or-later
URL:            https://www.gnu.org/software/sed/
Source0:        %{name}-%{version}.tar.gz

%description
GNU sed 4.9 built static against musl for the oxide userspace.

%prep
%setup -q

%build
. /home/nd/oxide/rpmbuild/lib/uapi-stage.sh
if [ "%{_target_cpu}" = "aarch64" ]; then
    CC=%{cross_arm}; UAPI="$(uapi_cflags aarch64)"
else
    CC=musl-gcc;     UAPI="$(uapi_cflags x86_64)"
fi
CC="$CC" CC_FOR_BUILD=gcc \
CFLAGS_FOR_BUILD="-D_GNU_SOURCE -Wno-implicit-function-declaration -Wno-incompatible-pointer-types" \
LDFLAGS_FOR_BUILD="" \
CFLAGS="-Os -D_GNU_SOURCE -Wno-implicit-function-declaration -Wno-incompatible-pointer-types $UAPI" \
LDFLAGS="-static" \
./configure --host=%{_target_cpu}-linux-musl \
    --disable-nls --disable-acl --disable-i18n --without-selinux --prefix=/usr
make %{?_smp_mflags}

%install
mkdir -p %{buildroot}%{_bindir}
install -m0755 sed/sed %{buildroot}%{_bindir}/sed

%files
%{_bindir}/sed

%changelog
* Sat Jun 13 2026 Chris Watkins <chris@watkinslabs.com> - 4.9-1
- Initial oxide from-source spec (autotools family).
