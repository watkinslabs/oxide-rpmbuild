#!/usr/bin/sh
# Populate upstream source URLs so any instance can `vendorctl src fetch <pkg>` +
# build with no local vendor tree. sha256 is trust-on-first-fetch then pinned in the
# sources table (re-export to bake into git). GitHub archive tarballs unpack to
# <repo>-<tag-without-v>, which matches each package's src_subdir.
set -e
cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl
add() { $V src add --package "$1" --version "$2" --url "$3" --filename "$4" 2>/dev/null || \
        $V src update --package "$1" --version "$2" --old-url "$3" --url "$3" --filename "$4" >/dev/null; }

GNU=https://mirrors.kernel.org/gnu
add sed       4.9      $GNU/sed/sed-4.9.tar.xz                 sed-4.9.tar.xz
add grep      3.11     $GNU/grep/grep-3.11.tar.xz             grep-3.11.tar.xz
add tar       1.35     $GNU/tar/tar-1.35.tar.xz               tar-1.35.tar.xz
add make      4.4.1    $GNU/make/make-4.4.1.tar.gz            make-4.4.1.tar.gz
add gawk      5.3.1    $GNU/gawk/gawk-5.3.1.tar.xz            gawk-5.3.1.tar.xz
add gzip      1.13     $GNU/gzip/gzip-1.13.tar.xz             gzip-1.13.tar.xz
add patch     2.7.6    $GNU/patch/patch-2.7.6.tar.xz          patch-2.7.6.tar.xz
add diffutils 3.10     $GNU/diffutils/diffutils-3.10.tar.xz   diffutils-3.10.tar.xz
add findutils 4.10.0   $GNU/findutils/findutils-4.10.0.tar.xz findutils-4.10.0.tar.xz
add bzip2     1.0.8    https://sourceware.org/pub/bzip2/bzip2-1.0.8.tar.gz bzip2-1.0.8.tar.gz
add xz        5.6.3    https://github.com/tukaani-project/xz/releases/download/v5.6.3/xz-5.6.3.tar.xz xz-5.6.3.tar.xz

GH=https://github.com
gh() { add "$1" "$2" "$GH/$3/archive/refs/tags/$4.tar.gz" "$5"; }
gh fd        10.2.0  sharkdp/fd                v10.2.0  fd-10.2.0.tar.gz
gh bat       0.24.0  sharkdp/bat               v0.24.0  bat-0.24.0.tar.gz
gh bottom    0.10.2  ClementTsang/bottom       0.10.2   bottom-0.10.2.tar.gz
gh choose    1.3.6   theryangeary/choose       v1.3.6   choose-1.3.6.tar.gz
gh delta     0.18.2  dandavison/delta          0.18.2   delta-0.18.2.tar.gz
gh dua       2.30.1  Byron/dua-cli             v2.30.1  dua-cli-2.30.1.tar.gz
gh dust      1.1.1   bootandy/dust             v1.1.1   dust-1.1.1.tar.gz
gh eza       0.20.24 eza-community/eza         v0.20.24 eza-0.20.24.tar.gz
gh grex      1.4.5   pemistahl/grex            v1.4.5   grex-1.4.5.tar.gz
gh hexyl     0.15.0  sharkdp/hexyl             v0.15.0  hexyl-0.15.0.tar.gz
gh hyperfine 1.19.0  sharkdp/hyperfine         v1.19.0  hyperfine-1.19.0.tar.gz
gh procs     0.14.10 dalance/procs             v0.14.10 procs-0.14.10.tar.gz
gh ripgrep   14.1.1  BurntSushi/ripgrep        14.1.1   ripgrep-14.1.1.tar.gz
gh sd        1.0.0   chmln/sd                  v1.0.0   sd-1.0.0.tar.gz
gh starship  1.21.1  starship/starship         v1.21.1  starship-1.21.1.tar.gz
gh tokei     12.1.2  XAMPPRocky/tokei          v12.1.2  tokei-12.1.2.tar.gz
gh xh        0.23.0  ducaale/xh                v0.23.0  xh-0.23.0.tar.gz
gh zoxide    0.9.6   ajeetdsouza/zoxide        v0.9.6   zoxide-0.9.6.tar.gz
gh tealdeer  1.7.1   dbrgn/tealdeer            v1.7.1   tealdeer-1.7.1.tar.gz
gh yazi      0.4.2   sxyazi/yazi               v0.4.2   yazi-0.4.2.tar.gz
echo "source URLs registered"
