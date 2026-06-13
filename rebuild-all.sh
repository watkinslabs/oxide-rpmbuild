#!/usr/bin/sh
cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl
PKGS="bzip2 sed grep tar make gawk gzip xz patch diffutils findutils fd bat bottom choose delta dua dust eza grex hexyl hyperfine procs ripgrep sd starship tokei xh zoxide tealdeer yazi"
# regen all specs first (serial, fast) — must not race the builds
for p in $PKGS; do $V spec gen $p >/dev/null 2>&1; done
# parallel build: each job builds one package for both arches
echo "$PKGS" | tr ' ' '\n' | xargs -P 10 -I{} sh -c '
  for a in x86_64 aarch64; do
    ./vendorctl/target/debug/vendorctl build {} --arch $a >/tmp/pr-{}-$a.log 2>&1 \
      && echo "{} $a OK" || echo "{} $a FAIL: $(grep -iE "error:|unpackaged|Bad exit|No such" /tmp/pr-{}-$a.log | tail -1)"
  done'
echo "=== republish ==="
$V publish 2>&1 | grep -E 'repo|signed|error'
