#!/usr/bin/sh
# Register the cargo (Rust) single-binary tool cluster. build-system=cargo;
# %build cross-compiles static-musl (RUSTFLAGS=+crt-static), binary lands at
# target/<triple>/release/<bin> — install src uses %{_target_cpu} macro.
set -e
cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl
L="MIT OR Apache-2.0"
T='target/%{_target_cpu}-unknown-linux-musl/release'

reg() { # key version license summary [src_subdir]
    $V pkg add "$1" 2>/dev/null || true
    $V ver add --package "$1" --version "$2" 2>/dev/null || true
    if [ -n "$5" ]; then sub="--src-subdir $5"; else sub=""; fi
    $V meta set "$1" --build-system cargo --license "$3" --summary "$4" --build-args "" $sub
    $V install clear "$1" 2>/dev/null || true
}
bin() { $V install add "$1" --dest "/usr/bin/$2" --src "$T/$2" --kind bin; }

reg bat       0.24.0  "$L" "bat: cat clone with syntax highlighting (oxide)"; bin bat bat
reg bottom    0.10.2  "$L" "bottom: system monitor (oxide)";                  bin bottom btm
reg choose    1.3.6   "$L" "choose: human cut/awk alternative (oxide)";       bin choose choose
reg delta     0.18.2  "MIT" "delta: syntax-highlighting pager for git (oxide)"; bin delta delta
reg dua       2.30.1  "MIT" "dua: disk usage analyzer (oxide)" dua-cli-2.30.1; bin dua dua
reg dust      1.1.1   "MIT" "dust: du alternative (oxide)";                    bin dust dust
reg eza       0.20.24 "MIT" "eza: modern ls (oxide)";                          bin eza eza
reg grex      1.4.5   "$L" "grex: regex generator (oxide)";                    bin grex grex
reg hexyl     0.15.0  "$L" "hexyl: hex viewer (oxide)";                        bin hexyl hexyl
reg hyperfine 1.19.0  "$L" "hyperfine: benchmarking tool (oxide)";             bin hyperfine hyperfine
reg procs     0.14.10 "MIT" "procs: ps alternative (oxide)";                   bin procs procs
reg ripgrep   14.1.1  "MIT OR Unlicense" "ripgrep: recursive grep (oxide)";    bin ripgrep rg
reg sd        1.0.0   "MIT" "sd: sed alternative (oxide)";                     bin sd sd
reg starship  1.21.1  "ISC" "starship: shell prompt (oxide)";                  bin starship starship
reg tokei     12.1.2  "$L" "tokei: code statistics (oxide)";                   bin tokei tokei
reg xh        0.23.0  "MIT" "xh: HTTP client (oxide)";                         bin xh xh
reg zoxide    0.9.6   "MIT" "zoxide: smarter cd (oxide)";                      bin zoxide zoxide
reg tealdeer  1.7.1   "MIT OR Apache-2.0" "tealdeer: tldr client (oxide)";     bin tealdeer tldr
reg yazi      0.4.2   "MIT" "yazi: terminal file manager (oxide)";             bin yazi yazi; bin yazi ya

# onig_sys (oniguruma C dep) has K&R protos that modern gcc's C23 default rejects;
# pin gnu11 for packages that pull it (bat syntax set, delta/xh/yazi).
for p in bat delta xh yazi; do $V meta set "$p" --cflags "-std=gnu11" >/dev/null; done

echo "registered cargo cluster"
