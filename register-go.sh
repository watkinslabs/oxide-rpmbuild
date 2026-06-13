#!/usr/bin/sh
set -e; cd "$(dirname "$0")"
V=./vendorctl/target/debug/vendorctl
L="MIT"; GH=https://github.com
reg() { # key ver repo tag license summary buildargs bin
  $V pkg add "$1" 2>/dev/null || true
  $V ver add --package "$1" --version "$2" 2>/dev/null || true
  $V meta set "$1" --build-system go --license "$5" --summary "$6" --build-args "$7" >/dev/null
  $V install clear "$1" 2>/dev/null || true
  $V install add "$1" --dest "/usr/bin/$8" --src "$8" --kind bin
  $V src add --package "$1" --version "$2" --url "$GH/$3/archive/refs/tags/$4.tar.gz" --filename "$1-$2.tar.gz" 2>/dev/null || true
}
reg duf     0.8.1   muesli/duf            v0.8.1   "MIT"        "duf: disk usage/free (oxide)"        "."           duf
reg fzf     0.55.0  junegunn/fzf          v0.55.0  "MIT"        "fzf: fuzzy finder (oxide)"           "."           fzf
reg glow    2.0.0   charmbracelet/glow    v2.0.0   "MIT"        "glow: markdown renderer (oxide)"     "."           glow
reg gron    0.7.1   tomnomnom/gron        v0.7.1   "MIT"        "gron: greppable JSON (oxide)"        "."           gron
reg lazygit 0.44.1  jesseduffield/lazygit v0.44.1  "MIT"        "lazygit: git TUI (oxide)"            "."           lazygit
reg micro   2.0.14  zyedidia/micro        v2.0.14  "MIT"        "micro: terminal editor (oxide)"      "./cmd/micro" micro
reg yq      4.44.3  mikefarah/yq          v4.44.3  "MIT"        "yq: YAML processor (oxide)"          "."           yq
echo "registered go cluster"
