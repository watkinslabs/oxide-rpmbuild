# oxide rpmbuild — vendor → RPM repo build system

Fedora-grade, **from-source** RPM build for the oxide userspace. Cross-builds
each vendor package static-musl for `x86_64` and `aarch64`, packages it as an
RPM with the `.ox1` dist tag, and (once `createrepo_c` is installed) publishes a
`dnf`-consumable repo.

Sibling repo of `oxide2` (kernel). Sources are *referenced* from
`../oxide2/vendor/` — this tree does not copy the 14 GB of vendor sources.

## Tree (canonical rpm ≥4.20 `%_topdir`)

```
rpmbuild/
├── .rpmmacros-oxide     # %_topdir, %dist .ox1, %vendor_root
├── SPECS/<pkg>.spec     # generated, one per package
├── SOURCES/<pkg>-<ver>.tar.gz   # top dir MUST be <pkg>-<ver>/ (else %setup fails)
├── BUILD/  BUILDROOT/    # rpmbuild scratch (per-pkg root, RPM 4.20 layout)
├── RPMS/{x86_64,aarch64}/   # output (compiled → arch subdir, never noarch)
├── SRPMS/
├── vendorctl/           # SQLite catalog + release-manifest tool (moved here)
└── lib/uapi-stage.sh    # shared Linux-UAPI header staging for musl builds
```

## Build model — from-source

`%prep` unpacks `SOURCES/<pkg>-<ver>.tar.gz`; `%build` cross-compiles on the
host (no container), selecting the toolchain by `%{_target_cpu}`:

| arch | CC |
|---|---|
| `x86_64` | `musl-gcc` (host) |
| `aarch64` | `../oxide2/vendor/cross/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc` |

`%install` stages under `%{buildroot}`; `%files` lists final paths (must match
BUILDROOT exactly). Build: `rpmbuild -ba --target <arch> --define "_topdir …" SPECS/<pkg>.spec`.

### Proven
`bzip2` (plain-make family) builds clean on BOTH arches:
`bzip2-1.0.8-1.ox1.x86_64.rpm` (static x86-64 musl) +
`bzip2-1.0.8-1.ox1.aarch64.rpm` (static-pie ARM aarch64) + `.src.rpm`.

## Build-system families (`%build` templates)

~90 vendor packages span 5 families; `%build` differs per family, not per package:

| Family | Packages (examples) | `%build` |
|---|---|---|
| plain-make | bzip2 | `$CC -c *.c; $CC -static -o` |
| autotools | bash, coreutils, GNU tools, curl | `./configure --host=… ; make` (+ config.cache for cross) |
| meson | iputils, dbus, systemd | meson cross-file per arch |
| cargo | fd, eza, ripgrep, the Rust tools | `cargo build --target <arch>-unknown-linux-musl` |
| go | lazygit, glow | `GOARCH=… CGO_ENABLED=0 go build` |

**Gotcha:** rpm injects Fedora hardening CFLAGS (`redhat-hardened-cc1`, annobin,
`_FORTIFY_SOURCE=3`). Packages that honor `$CFLAGS` (autotools/meson/cmake) must
neutralize them — set `CFLAGS`/`LDFLAGS` explicitly or
`%global __global_compiler_flags %{nil}` — or the musl cross-build breaks.

## Install-path map (`%files`) — heterogeneous shapes

Authoritative source: `../oxide2/tools/xtask/src/rootfs.rs`. Shapes:

- single-binary → `/usr/bin/<name>` (~50 pkgs)
- multi-binary fixed table → util-linux, shadow, procps-ng, iproute2, openssh
- multicall + applet hardlinks → coreutils (~120 applets off `/usr/libexec/coreutils`)
- symlink/hardlink slots → bash→`/bin/sh`, gawk→awk, gzip→gunzip, python3→python3.13
- lib/dev trees (`include`+`lib`) → openssl, ncurses, zlib, pcre2, pam, … (`-devel` style)
- skip (not simple packages) → go, grub, limine, doom, terminfo, systemd

## vendorctl — the orchestrator

SQLite catalog + driver for the tree (`<topdir>/catalog.db`). Build standalone:
`cd vendorctl && cargo build`. Commands:

| Command | Does |
|---|---|
| `pkg add <key>` / `ver add --package <k> --version <v>` | register package + version |
| `meta set <k> --build-system <fam> --summary .. --license .. --url .. --build-args ..` | spec-gen metadata |
| `install add <k> --dest <path> [--src <p>] [--kind bin\|symlink\|hardlink\|tree] [--link-target <t>]` | `%files`/`%install` map |
| `src stage <k>` | tar `vendor/<k>/<k>-<ver>/` → `SOURCES/`, record sha256 |
| `spec gen <k>` | render `SPECS/<k>.spec` (family template + install map) |
| `build <k> [--arch x86_64\|aarch64\|both]` | `rpmbuild -ba` per arch, **sign** RPM+SRPM, record results |
| `publish` | `createrepo_c` each `RPMS/<arch>`, **sign** `repomd.xml` |
| `all <k>` | stage → gen → build |

## GPG signing

Key `rsa4096/2B3D90B0E4C5E7F2` (`oxide package signing <chris@watkinslabs.com>`).
Public key: `RPMS/RPM-GPG-KEY-oxide` — consumers `rpm --import` it.
- RPMs/SRPMs: `rpm --addsign` (needs `rpm-sign` pkg). Verified: `rpm -K` → `signatures OK`.
- `repomd.xml`: detached armored sig (`repomd.xml.asc`). Verified: `gpg --verify` → Good signature.
- `%_gpg_name` / override `OXIDE_GPG_NAME`. **Dev key is passphraseless** — for a real public
  release rotate to a passphrase-protected/offline key and re-sign.

## Status / TODO

- [x] tree + `.ox1` macros + vendorctl + lib
- [x] from-source pipeline proven both arches (bzip2 plain-make, sed autotools)
- [x] `createrepo_c` repo + GPG signing (RPM + SRPM + repomd) verified end-to-end
- [x] vendorctl orchestrator: catalog → stage → spec gen → build → sign → publish
- [ ] populate catalog for remaining ~88 packages (cargo/go/meson `%build` templates need toolchain wiring)
- [ ] coreutils applet hardlinks, lib-tree `-devel` packages
- [ ] migrate multi-binary path tables (util-linux, shadow, procps-ng, iproute2, openssh) from `rootfs.rs`
