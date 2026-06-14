// Orchestration: drive the canonical rpmbuild tree from the catalog.
// stage source -> generate spec (by build-system family) -> rpmbuild -> createrepo.
use crate::db::now_ts;
use crate::tree;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// Group enabled packages into dependency WAVES (Kahn's, level by level): wave[0] = packages
// with no build_requires, wave[n] = packages whose deps are all in earlier waves. Within a
// wave packages are independent → safe to build in parallel. The Fedora BuildRequires-graph
// approach. Errors on a dependency cycle.
fn all_enabled(conn: &Connection) -> Result<Vec<String>, String> {
    let mut st = conn.prepare("SELECT key FROM packages WHERE enabled=1 ORDER BY key")
        .map_err(|e| format!("vendorctl: list packages: {e}"))?;
    let rows = st.query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("vendorctl: query packages: {e}"))?;
    let mut v = Vec::new();
    for r in rows { v.push(r.map_err(|e| format!("vendorctl: row: {e}"))?); }
    Ok(v)
}

// Transitive build_requires closure of `targets` (targets + everything they need).
fn closure(conn: &Connection, targets: &[String]) -> Result<Vec<String>, String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack = targets.to_vec();
    while let Some(k) = stack.pop() {
        if !seen.insert(k.clone()) { continue; }
        let m = resolve(conn, &k)?;
        for d in m.build_requires.split_whitespace() {
            if !seen.contains(d) { stack.push(d.to_string()); }
        }
    }
    let mut v: Vec<String> = seen.into_iter().collect();
    v.sort();
    Ok(v)
}

// targets empty => all enabled; else the dependency closure of the targets.
pub(crate) fn topo_waves(conn: &Connection, targets: &[String]) -> Result<Vec<Vec<String>>, String> {
    let keys: Vec<String> = if targets.is_empty() { all_enabled(conn)? } else { closure(conn, targets)? };
    let kset: HashSet<&str> = keys.iter().map(|s| s.as_str()).collect();
    let mut indeg: HashMap<String, usize> = keys.iter().map(|k| (k.clone(), 0)).collect();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    for k in &keys {
        let m = resolve(conn, k)?;
        let ds: Vec<String> = m.build_requires.split_whitespace()
            .filter(|d| kset.contains(d)).map(|s| s.to_string()).collect();
        *indeg.get_mut(k).unwrap() = ds.len();
        for d in ds { dependents.entry(d).or_default().push(k.clone()); }
    }
    let mut waves: Vec<Vec<String>> = Vec::new();
    let mut cur: Vec<String> = keys.iter().filter(|k| indeg[*k] == 0).cloned().collect();
    cur.sort();
    let mut done = 0usize;
    while !cur.is_empty() {
        waves.push(cur.clone());
        done += cur.len();
        let mut next: Vec<String> = Vec::new();
        for k in &cur {
            if let Some(deps_on) = dependents.get(k) {
                for p in deps_on {
                    let e = indeg.get_mut(p).unwrap();
                    *e -= 1;
                    if *e == 0 { next.push(p.clone()); }
                }
            }
        }
        next.sort(); next.dedup();
        cur = next;
    }
    if done != keys.len() {
        let stuck: Vec<&String> = keys.iter().filter(|k| indeg[*k] > 0).collect();
        return Err(format!("vendorctl: dependency cycle / missing dep among: {stuck:?}"));
    }
    Ok(waves)
}

pub(crate) fn topo_order(conn: &Connection, targets: &[String]) -> Result<Vec<String>, String> {
    Ok(topo_waves(conn, targets)?.into_iter().flatten().collect())
}

// `plan [pkg...]`: dependency-ordered build plan (the scan matrix) for the given packages +
// their deps, or all packages if none given.
pub(crate) fn plan(conn: &Connection, targets: &[String]) -> Result<(), String> {
    let order = topo_order(conn, targets)?;
    for (i, k) in order.iter().enumerate() {
        let m = resolve(conn, k)?;
        let br = if m.build_requires.is_empty() { "-".to_string() } else { m.build_requires.clone() };
        println!("{:>3}  {:<14} [{:<10}] tc:{:<16} needs: {br}", i + 1, k, m.build_system, toolchains_of(&m));
    }
    println!("-- {} packages, dependency-ordered --", order.len());
    Ok(())
}

// `toolchains [pkg...]`: per-package compiler/toolchain requirements + a distinct summary.
pub(crate) fn toolchains(conn: &Connection, targets: &[String]) -> Result<(), String> {
    let keys: Vec<String> = if targets.is_empty() { all_enabled(conn)? } else { closure(conn, targets)? };
    let mut all: HashMap<String, usize> = HashMap::new();
    for k in &keys {
        let m = resolve(conn, k)?;
        let tc = toolchains_of(&m);
        println!("{:<14} {tc}", k);
        for t in tc.split_whitespace() { *all.entry(t.to_string()).or_default() += 1; }
    }
    let mut summary: Vec<(String, usize)> = all.into_iter().collect();
    summary.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let parts: Vec<String> = summary.iter().map(|(t, n)| format!("{t}={n}")).collect();
    println!("-- toolchains needed across {} pkgs: {} --", keys.len(), parts.join(" "));
    Ok(())
}

// `graph [pkg...]`: show the dependency graph — a tree per package that HAS build deps, plus a
// flat list of the leaves (no build deps). Scoped to the given packages' closure, or all.
pub(crate) fn graph(conn: &Connection, targets: &[String]) -> Result<(), String> {
    let keys: Vec<String> = if targets.is_empty() { all_enabled(conn)? } else { closure(conn, targets)? };
    let kset: HashSet<&str> = keys.iter().map(|s| s.as_str()).collect();
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    let mut leaves: Vec<String> = Vec::new();
    for k in &keys {
        let m = resolve(conn, k)?;
        let ds: Vec<String> = m.build_requires.split_whitespace()
            .filter(|d| kset.contains(d)).map(|s| s.to_string()).collect();
        if ds.is_empty() { leaves.push(k.clone()); } else { deps.insert(k.clone(), ds); }
    }
    fn print_tree(k: &str, deps: &HashMap<String, Vec<String>>, depth: usize, seen: &mut HashSet<String>) {
        let indent = "  ".repeat(depth);
        let arrow = if depth == 0 { "" } else { "└─ " };
        println!("{indent}{arrow}{k}");
        if !seen.insert(k.to_string()) { return; }
        if let Some(ds) = deps.get(k) { for d in ds { print_tree(d, deps, depth + 1, seen); } }
    }
    println!("== dependency graph ({} pkgs) ==", keys.len());
    let mut roots: Vec<&String> = deps.keys().collect();
    roots.sort();
    for r in roots { let mut seen = HashSet::new(); print_tree(r, &deps, 0, &mut seen); }
    leaves.sort();
    println!("\n-- no build deps ({}) --\n{}", leaves.len(), leaves.join(" "));
    Ok(())
}

// `build-all`: dependency waves, parallel within each wave. fetch+gen run sequentially (fast),
// then each wave's packages build concurrently (capped) by shelling to `vendorctl build` —
// every child opens its own WAL connection and builds both arches sequentially (so the
// per-package rpmbuild BUILD dir never collides). Deps are in earlier waves, so each
// package's sysroot is populated when it builds.
pub(crate) fn build_all(conn: &Connection, arches: &[String], targets: &[String]) -> Result<(), String> {
    let waves = topo_waves(conn, targets)?;
    let total: usize = waves.iter().map(|w| w.len()).sum();
    println!("== build-all: {total} packages in {} dependency waves ==", waves.len());
    // fetch (if missing) + gen spec — sequential, cheap
    for key in waves.iter().flatten() {
        let m = resolve(conn, key)?;
        if let Some(s) = source_for(conn, m.id)? {
            let fname = if s.filename.is_empty() { format!("{key}-{}.tar.gz", m.version) } else { s.filename };
            if !s.url.is_empty() && !tree::sources().join(&fname).is_file() {
                if let Err(e) = fetch(conn, key) { eprintln!("fetch {key}: {e}"); }
            }
        }
        if let Err(e) = gen_spec(conn, key) { eprintln!("gen {key}: {e}"); }
    }
    let exe = std::env::current_exe().map_err(|e| format!("vendorctl: current_exe: {e}"))?;
    let single_arch: Option<&str> = if arches.len() == 1 { Some(arches[0].as_str()) } else { None };
    let cap = 10usize;
    let mut failed: Vec<String> = Vec::new();
    for (wi, wave) in waves.iter().enumerate() {
        println!("-- wave {}/{}: {} pkgs: {wave:?} --", wi + 1, waves.len(), wave.len());
        for chunk in wave.chunks(cap) {
            let mut kids: Vec<(String, std::process::Child)> = Vec::new();
            for key in chunk {
                let log = std::fs::File::create(format!("/tmp/ba-{key}.log"))
                    .map_err(|e| format!("vendorctl: log {key}: {e}"))?;
                let err = log.try_clone().map_err(|e| format!("vendorctl: log dup: {e}"))?;
                let mut c = Command::new(&exe);
                c.arg("build").arg(key);
                if let Some(a) = single_arch { c.arg("--arch").arg(a); }
                c.stdout(std::process::Stdio::from(log)).stderr(std::process::Stdio::from(err));
                match c.spawn() {
                    Ok(ch) => kids.push((key.clone(), ch)),
                    Err(e) => { eprintln!("spawn {key}: {e}"); failed.push(key.clone()); }
                }
            }
            for (key, mut ch) in kids {
                match ch.wait() {
                    Ok(s) if s.success() => println!("  OK   {key}"),
                    _ => { println!("  FAIL {key}  (/tmp/ba-{key}.log)"); failed.push(key); }
                }
            }
        }
    }
    if failed.is_empty() { println!("== all {total} built =="); Ok(()) }
    else { Err(format!("vendorctl: {} failed: {failed:?}", failed.len())) }
}

pub(crate) struct VerMeta {
    pub id: i64,
    pub version: String,
    pub build_system: String,
    pub summary: String,
    pub license: String,
    pub upstream_url: String,
    pub src_subdir: String,
    pub build_args: String,
    pub cflags: String,
    pub config_cache: String,
    pub ldflags: String,
    pub install_cmd: String,
    pub build_requires: String,
    pub toolchains: String,
}

// Toolchains a package needs — explicit if set, else derived from its build-system family.
pub(crate) fn toolchains_of(m: &VerMeta) -> String {
    if !m.toolchains.is_empty() { return m.toolchains.clone(); }
    match m.build_system.as_str() {
        "cargo" => "rust c".into(),       // rust + cross cc for C deps (onig/jemalloc)
        "go" => "go".into(),
        "meson" => "c c++ meson python".into(),
        _ => "c".into(),                  // autotools / script / plain-make
    }
}

pub(crate) struct Install {
    pub src: String,
    pub dest: String,
    pub kind: String,
    pub link_target: String,
    pub mode: String,
}

// Resolve the build-target version (most recently added) for a package.
pub(crate) fn resolve(conn: &Connection, key: &str) -> Result<VerMeta, String> {
    conn.query_row(
        "SELECT id, version, build_system, summary, license, upstream_url, src_subdir, build_args, cflags, config_cache, ldflags, install_cmd, build_requires, toolchains \
         FROM package_versions WHERE package_key=?1 ORDER BY id DESC LIMIT 1",
        params![key],
        |r| Ok(VerMeta {
            id: r.get(0)?, version: r.get(1)?, build_system: r.get(2)?, summary: r.get(3)?,
            license: r.get(4)?, upstream_url: r.get(5)?, src_subdir: r.get(6)?, build_args: r.get(7)?,
            cflags: r.get(8)?, config_cache: r.get(9)?, ldflags: r.get(10)?,
            install_cmd: r.get(11)?, build_requires: r.get(12)?, toolchains: r.get(13)?,
        }),
    )
    .optional()
    .map_err(|e| format!("vendorctl: query {key}: {e}"))?
    .ok_or_else(|| format!("vendorctl: no version for package `{key}` (add with `ver add`)"))
}

pub(crate) fn installs(conn: &Connection, ver_id: i64) -> Result<Vec<Install>, String> {
    let mut st = conn
        .prepare("SELECT src, dest, kind, link_target, mode FROM install_map WHERE package_version_id=?1 ORDER BY id")
        .map_err(|e| format!("vendorctl: prepare install_map: {e}"))?;
    let rows = st
        .query_map(params![ver_id], |r| Ok(Install {
            src: r.get(0)?, dest: r.get(1)?, kind: r.get(2)?, link_target: r.get(3)?, mode: r.get(4)?,
        }))
        .map_err(|e| format!("vendorctl: query install_map: {e}"))?;
    let mut v = Vec::new();
    for r in rows { v.push(r.map_err(|e| format!("vendorctl: row: {e}"))?); }
    Ok(v)
}

fn src_subdir(m: &VerMeta, key: &str) -> String {
    if m.src_subdir.is_empty() { format!("{key}-{}", m.version) } else { m.src_subdir.clone() }
}

// `src stage`: produce SOURCES/<key>-<ver>.tar.gz with top dir <subdir>, record sha256.
pub(crate) fn stage(conn: &Connection, key: &str) -> Result<(), String> {
    let m = resolve(conn, key)?;
    let sub = src_subdir(&m, key);
    let vroot = tree::vendor_root().join(key);
    if !vroot.join(&sub).is_dir() {
        return Err(format!("vendorctl: source dir {}/{sub} not found", vroot.display()));
    }
    fs::create_dir_all(tree::sources()).map_err(|e| format!("vendorctl: mkdir SOURCES: {e}"))?;
    let tarball = tree::sources().join(format!("{key}-{}.tar.gz", m.version));
    // exclude build artifacts that dirty vendor trees carry (cargo target/ is GB-scale).
    let st = Command::new("tar")
        .args(["czf", tarball.to_str().unwrap(),
               &format!("--exclude={sub}/target"), &format!("--exclude={sub}/.git"),
               "-C", vroot.to_str().unwrap(), &sub])
        .status().map_err(|e| format!("vendorctl: tar: {e}"))?;
    if !st.success() { return Err(format!("vendorctl: tar failed for {key}")); }
    let out = Command::new("sha256sum").arg(&tarball).output()
        .map_err(|e| format!("vendorctl: sha256sum: {e}"))?;
    let sha = String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or("").to_string();
    conn.execute("UPDATE package_versions SET integrity_hash=?1 WHERE id=?2", params![sha, m.id])
        .map_err(|e| format!("vendorctl: record hash: {e}"))?;
    println!("staged\t{}\tsha256:{sha}", tarball.display());
    Ok(())
}

pub(crate) struct Src { pub url: String, pub filename: String, pub checksum: String }

pub(crate) fn source_for(conn: &Connection, ver_id: i64) -> Result<Option<Src>, String> {
    conn.query_row(
        "SELECT canonical_url, filename, checksum_value FROM sources WHERE package_version_id=?1 ORDER BY id LIMIT 1",
        params![ver_id],
        |r| Ok(Src { url: r.get(0)?, filename: r.get(1)?, checksum: r.get(2)? }),
    ).optional().map_err(|e| format!("vendorctl: query source: {e}"))
}

// All sources for a version, ordered (id). [0] = Source0 (the main tarball, %setup'd);
// [1..] = Source1.. = secondary tarballs (e.g. perl-cross), extracted in %prep over the tree.
pub(crate) fn sources_for(conn: &Connection, ver_id: i64) -> Result<Vec<Src>, String> {
    let mut st = conn.prepare("SELECT canonical_url, filename, checksum_value FROM sources WHERE package_version_id=?1 ORDER BY id")
        .map_err(|e| format!("vendorctl: prepare sources: {e}"))?;
    let rows = st.query_map(params![ver_id], |r| Ok(Src { url: r.get(0)?, filename: r.get(1)?, checksum: r.get(2)? }))
        .map_err(|e| format!("vendorctl: query sources: {e}"))?;
    let mut v = Vec::new();
    for r in rows { v.push(r.map_err(|e| format!("vendorctl: source row: {e}"))?); }
    Ok(v)
}

fn sha256_of(path: &Path) -> Result<String, String> {
    let out = Command::new("sha256sum").arg(path).output().map_err(|e| format!("vendorctl: sha256sum: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or("").to_string())
}

// `src fetch`: download canonical_url -> SOURCES/<filename>, verify (or record) sha256.
// Enables distributed builds — a fresh instance needs only the repo + network, no local vendor tree.
pub(crate) fn fetch(conn: &Connection, key: &str) -> Result<(), String> {
    let m = resolve(conn, key)?;
    let srcs: Vec<Src> = sources_for(conn, m.id)?.into_iter().filter(|s| !s.url.is_empty()).collect();
    if srcs.is_empty() { return Err(format!("vendorctl: no source URL for {key} (add with `src add --url ...`)")); }
    fs::create_dir_all(tree::sources()).map_err(|e| format!("vendorctl: mkdir SOURCES: {e}"))?;
    for (i, s) in srcs.iter().enumerate() {
        let fname = if s.filename.is_empty() {
            if i == 0 { format!("{key}-{}.tar.gz", m.version) } else { s.url.rsplit('/').next().unwrap_or("source").to_string() }
        } else { s.filename.clone() };
        let out = tree::sources().join(&fname);
        let st = Command::new("curl").args(["-fsSL", "--retry", "3", "-o", out.to_str().unwrap(), &s.url])
            .status().map_err(|e| format!("vendorctl: curl: {e}"))?;
        if !st.success() { return Err(format!("vendorctl: download failed: {}", s.url)); }
        let sha = sha256_of(&out)?;
        if s.checksum.is_empty() {
            conn.execute("UPDATE sources SET checksum_value=?1, checksum_type='sha256' WHERE package_version_id=?2 AND canonical_url=?3",
                params![sha, m.id, s.url]).map_err(|e| format!("vendorctl: record checksum: {e}"))?;
            println!("fetched\t{}\tsha256:{sha} (recorded)", out.display());
        } else if s.checksum != sha {
            return Err(format!("vendorctl: CHECKSUM MISMATCH for {key}: expected {} got {sha}", s.checksum));
        } else {
            println!("fetched\t{}\tsha256 verified", out.display());
        }
    }
    Ok(())
}


// GPG identity used to sign RPMs + repo metadata. Override with OXIDE_GPG_NAME.
fn gpg_name() -> String {
    // key-id is stable across UID relabels; override with OXIDE_GPG_NAME.
    std::env::var("OXIDE_GPG_NAME").unwrap_or_else(|_| "2B3D90B0E4C5E7F2".to_string())
}

// rpm --addsign <path>. Non-interactive with a passphraseless key via gpg-agent.
fn sign_rpm(path: &Path) -> Result<(), String> {
    if !path.is_file() { return Ok(()); }
    let st = Command::new("rpm")
        .args(["--define", &format!("_gpg_name {}", gpg_name())])
        .arg("--addsign").arg(path.to_str().unwrap())
        .status().map_err(|e| format!("vendorctl: rpm --addsign: {e}"))?;
    if !st.success() { return Err(format!("vendorctl: signing failed: {}", path.display())); }
    println!("signed\t{}", path.display());
    Ok(())
}

// Toolchain env for %install — install steps may relink/recompile (libtool, make-based
// `make install`), so they need CC/CROSS + toolchain on PATH, and rpm's injected CFLAGS
// neutralized (else host gcc gets target hardening flags). %install does NOT run the
// %build preamble, so set it here too.
//
// THREE TOOL DOMAINS — never let them mix (HARD RULE; the cause of nasty host/target bugs):
//   1. host build tools (run during build): host gcc/as/ld/make — from the inherited $PATH.
//   2. cross toolchain (run on host, emit target): $TCBIN (Bootlin $TRIPLE-* + oxide-gcc).
//   3. target sysroot $SYS: headers (-I) + libs (-L) + .pc ONLY — its /usr/bin holds TARGET
//      (musl) binaries that CANNOT run on the host. So PATH = $TCBIN:$PATH:$SYS/usr/bin —
//      cross + host always win; $SYS/usr/bin is LAST, reachable only for arch-independent
//      *-config shell scripts (gpgrt-config, …). A target ELF must never shadow a host tool.
fn tc_path_export() -> String {
    let (x86bin, armbin) = (tc_bin(ARCH_X86), tc_bin(ARCH_ARM));
    format!(
        "SYS={topdir}/sysroot/%{{name}}/%{{_target_cpu}}\n\
         if [ \"%{{_target_cpu}}\" = \"aarch64\" ]; then CC={armbin}/{TRIPLE_ARM}-oxide-gcc; CROSS={armbin}/{TRIPLE_ARM}-; TCBIN={armbin}; TCSYS={armsys}; \
         else CC={x86bin}/{TRIPLE_X86}-oxide-gcc; CROSS={x86bin}/{TRIPLE_X86}-; TCBIN={x86bin}; TCSYS={x86sys}; fi\n\
         export CC CROSS TCSYS PATH=\"$TCBIN:$PATH:$SYS/usr/bin\"\n\
         unset CFLAGS CXXFLAGS CPPFLAGS LDFLAGS\n",
        topdir = tree::topdir().display(),
        armbin = armbin.display(), x86bin = x86bin.display(),
        armsys = tc_sysroot(ARCH_ARM, TRIPLE_ARM).display(), x86sys = tc_sysroot(ARCH_X86, TRIPLE_X86).display())
}

// Toolchain sysroot (musl headers + libc.a/.so + crt*.o + kernel UAPI headers) for a triple.
fn tc_sysroot(tcdir: &str, triple: &str) -> PathBuf {
    tree::vendor_root().join("cross").join(tcdir).join(triple).join("sysroot")
}

// Vendored Bootlin cross-toolchains (gcc 14.3 / musl 1.2.5 with statx). Self-contained:
// own musl libc + UAPI headers. CC is the `-oxide-` re-exec wrapper (poison check bypassed).
const ARCH_X86: &str = "x86-64--musl--stable-2025.08-1";
const ARCH_ARM: &str = "aarch64--musl--stable-2025.08-1";
const TRIPLE_X86: &str = "x86_64-buildroot-linux-musl";
const TRIPLE_ARM: &str = "aarch64-buildroot-linux-musl";
fn tc_bin(tcdir: &str) -> PathBuf { tree::vendor_root().join("cross").join(tcdir).join("bin") }

// %build block per build-system family.
fn build_block(m: &VerMeta) -> Result<String, String> {
    // HOST-INDEPENDENT: both arches use the self-contained vendored cross toolchains, which
    // bundle their own musl libc + UAPI headers. No host musl-gcc, no host /usr/include copy.
    let (x86bin, armbin) = (tc_bin(ARCH_X86), tc_bin(ARCH_ARM));
    // CC = oxide re-exec wrapper (buildroot poison check bypassed); CROSS/TRIPLE = real triple.
    let cc_x86 = format!("{}/{TRIPLE_X86}-oxide-gcc", x86bin.display());
    let cc_arm = format!("{}/{TRIPLE_ARM}-oxide-gcc", armbin.display());
    // SYS = per-arch sysroot where build_requires deps are installed; build against it.
    let preamble = format!(
        "SYS={topdir}/sysroot/%{{name}}/%{{_target_cpu}}\n\
         if [ \"%{{_target_cpu}}\" = \"aarch64\" ]; then CC={ccarm}; CXX={cxxarm}; CROSS={armbin}/{TRIPLE_ARM}-; TCBIN={armbin}; TRIPLE={TRIPLE_ARM}; TCSYS={armsys}; \
         else CC={ccx86}; CXX={cxxx86}; CROSS={x86bin}/{TRIPLE_X86}-; TCBIN={x86bin}; TRIPLE={TRIPLE_X86}; TCSYS={x86sys}; fi\n\
         UAPI=\"\"\n\
         export TCSYS PATH=\"$TCBIN:$PATH:$SYS/usr/bin\"\n\
         export CC_FOR_BUILD=gcc BUILD_CC=gcc CXX\n\
         unset CFLAGS CXXFLAGS CPPFLAGS LDFLAGS\n",
        topdir = tree::topdir().display(),
        ccarm = cc_arm, cxxarm = format!("{}/{TRIPLE_ARM}-oxide-g++", armbin.display()),
        ccx86 = cc_x86, cxxx86 = format!("{}/{TRIPLE_X86}-oxide-g++", x86bin.display()),
        armbin = armbin.display(), x86bin = x86bin.display(),
        armsys = tc_sysroot(ARCH_ARM, TRIPLE_ARM).display(), x86sys = tc_sysroot(ARCH_X86, TRIPLE_X86).display());
    let b = &m.build_args;
    let cf = &m.cflags; // extra per-package CFLAGS (e.g. -std=gnu89)
    let lf = &m.ldflags; // extra per-package LDFLAGS (e.g. -L<dep>/lib for shared deps)
    Ok(match m.build_system.as_str() {
        // file-staging package: no compile; %install (gen_spec) does the file copy.
        "stage" => String::new(),
        "plain-make" => format!("{preamble}export CC UAPI\nOXIDE_CFLAGS=\"{cf}\"; export OXIDE_CFLAGS\n{b}\n"),
        "autotools" => {
            // cross config.cache: answers for configure tests that must RUN a target binary.
            let (cache_write, cache_flag) = if m.config_cache.is_empty() {
                (String::new(), "")
            } else {
                (format!("cat > config.cache <<'OXEOF'\n{}\nOXEOF\n", m.config_cache), "--cache-file=config.cache ")
            };
            format!(
            "{preamble}\
             [ -f Makefile ] && make distclean >/dev/null 2>&1 || true\n\
             {cache_write}\
             CC=\"$CC\" CC_FOR_BUILD=gcc LDFLAGS_FOR_BUILD=\"\" \\\n\
             CFLAGS_FOR_BUILD=\"-D_GNU_SOURCE {cf} -Wno-implicit-function-declaration -Wno-incompatible-pointer-types -Wno-int-conversion\" \\\n\
             CFLAGS=\"-Os -D_GNU_SOURCE {cf} -I$SYS/usr/include -Wno-poison-system-directories -Wno-implicit-function-declaration -Wno-incompatible-pointer-types -Wno-int-conversion $UAPI\" \\\n\
             LDFLAGS=\"-Wl,-rpath-link,$SYS/usr/lib -L$SYS/usr/lib {lf}\" \\\n\
             PKG_CONFIG_LIBDIR=\"$SYS/usr/lib/pkgconfig\" PKG_CONFIG_SYSROOT_DIR=\"$SYS\" PKG_CONFIG_PATH=\"\" \\\n\
             ./configure --build=x86_64-pc-linux-gnu --host=$TRIPLE {cache_flag}{b}\n\
             make %{{?_smp_mflags}}\n")
        },
        // custom build systems (zlib ./configure, openssl ./Configure): build_args is the full
        // configure+make snippet; CC/CROSS/SYS/UAPI exported. %install uses install_cmd.
        "script" => format!(
            "{preamble}\
             [ -f Makefile ] && make distclean >/dev/null 2>&1 || true\n\
             export CC CROSS UAPI\n\
             export CFLAGS=\"-Os -fPIC -D_GNU_SOURCE {cf} -I$SYS/usr/include -Wno-poison-system-directories $UAPI\"\n\
             export LDFLAGS=\"-Wl,-rpath-link,$SYS/usr/lib -L$SYS/usr/lib {lf}\"\n\
             unset PKG_CONFIG_PATH\n\
             export PKG_CONFIG_LIBDIR=\"$SYS/usr/lib/pkgconfig\" PKG_CONFIG_SYSROOT_DIR=\"$SYS\"\n\
             {b}\n"),
        "cargo" => format!(
            // host-independent: vendored cross gcc for the Rust link AND cc-rs C deps (onig/jemalloc),
            // both arches. unset rpm's injected CC/CFLAGS first.
            "unset CC CXX CPP CFLAGS CXXFLAGS CPPFLAGS LDFLAGS\n\
             {cf_export}\
             if [ \"%{{_target_cpu}}\" = \"aarch64\" ]; then TGT=aarch64-unknown-linux-musl; G={ccarm}; \
             else TGT=x86_64-unknown-linux-musl; G={ccx86}; fi\n\
             export PATH=\"$(dirname $G):$PATH\"\n\
             V=$(echo $TGT | tr 'a-z-' 'A-Z_'); export CARGO_TARGET_${{V}}_LINKER=$G\n\
             export CC_$(echo $TGT | tr - _)=$G\n\
             rustup target add $TGT >/dev/null 2>&1 || true\n\
             RUSTFLAGS=\"-C target-feature=+crt-static\" cargo build --release --target $TGT {b}\n",
            cf_export = if cf.is_empty() { String::new() } else { format!("export CFLAGS=\"{cf}\" CXXFLAGS=\"{cf}\"\n") },
            ccarm = cc_arm, ccx86 = cc_x86, b = b),
        // meson: generate a cross-file from the vendored toolchain + sysroot, then meson+ninja.
        "meson" => format!(
            "{preamble}\
             unset PKG_CONFIG_PATH\n\
             export PKG_CONFIG_LIBDIR=\"$SYS/usr/lib/pkgconfig\" PKG_CONFIG_SYSROOT_DIR=\"$SYS\"\n\
             cat > oxide-cross.ini <<OXEOF\n\
             [binaries]\n\
             c = '$CC'\n\
             ar = '${{CROSS}}ar'\n\
             strip = '${{CROSS}}strip'\n\
             pkg-config = 'pkg-config'\n\
             [host_machine]\n\
             system = 'linux'\n\
             cpu_family = '%{{_target_cpu}}'\n\
             cpu = '%{{_target_cpu}}'\n\
             endian = 'little'\n\
             [built-in options]\n\
             c_args = ['-I$SYS/usr/include', '-Wno-poison-system-directories']\n\
             c_link_args = ['-L$SYS/usr/lib', '-Wl,-rpath-link,$SYS/usr/lib']\n\
             [properties]\n\
             sys_root = '$SYS'\n\
             pkg_config_libdir = ['$SYS/usr/lib/pkgconfig']\n\
             OXEOF\n\
             rm -rf _b\n\
             meson setup _b --cross-file oxide-cross.ini --prefix=/usr --libdir=lib {b}\n\
             ninja -C _b\n"),
        "go" => format!(
            // Go cross-compiles natively via GOOS/GOARCH (no cross toolchain); CGO off = static.
            // -o %{{name}} fixes the output path; build_args = the package path (. or ./cmd/x).
            "export PATH={gobin}:$PATH\n\
             if [ \"%{{_target_cpu}}\" = \"aarch64\" ]; then GOARCH=arm64; else GOARCH=amd64; fi\n\
             CGO_ENABLED=0 GOOS=linux GOARCH=$GOARCH go build -ldflags='-s -w' -o %{{name}} {b}\n",
            gobin = tree::vendor_root().join("go/bin").display(), b = b),
        other => return Err(format!("vendorctl: build_system `{other}` not yet templated (plain-make|autotools|cargo|go)")),
    })
}

fn install_block(items: &[Install]) -> String {
    let mut s = String::from("rm -rf %{buildroot}\n");
    for it in items {
        match it.kind.as_str() {
            "bin" | "file" => {
                let dir = Path::new(&it.dest).parent().map(|p| p.display().to_string()).unwrap_or_default();
                s.push_str(&format!("mkdir -p %{{buildroot}}{dir}\n"));
                s.push_str(&format!("install -m{} {} %{{buildroot}}{}\n", it.mode, it.src, it.dest));
            }
            "symlink" => {
                let dir = Path::new(&it.dest).parent().map(|p| p.display().to_string()).unwrap_or_default();
                s.push_str(&format!("mkdir -p %{{buildroot}}{dir}\n"));
                s.push_str(&format!("ln -s {} %{{buildroot}}{}\n", it.link_target, it.dest));
            }
            "hardlink" => {
                let dir = Path::new(&it.dest).parent().map(|p| p.display().to_string()).unwrap_or_default();
                s.push_str(&format!("mkdir -p %{{buildroot}}{dir}\n"));
                s.push_str(&format!("ln %{{buildroot}}{} %{{buildroot}}{}\n", it.link_target, it.dest));
            }
            "tree" => {
                s.push_str(&format!("mkdir -p %{{buildroot}}{}\n", it.dest));
                s.push_str(&format!("cp -a {}/. %{{buildroot}}{}/\n", it.src, it.dest));
            }
            _ => {}
        }
    }
    s
}

// Best-effort capture of cargo-generated man pages + shell completions from the build
// OUT_DIR (build.rs emits them there). Globbed because crate naming varies.
const CARGO_EXTRAS: &str = "\
B=target/%{_target_cpu}-unknown-linux-musl/release/build\n\
find $B -path '*/out/*' -name '*.1' 2>/dev/null | while read f; do install -Dm644 \"$f\" %{buildroot}%{_mandir}/man1/\"$(basename \"$f\")\"; done\n\
find $B -path '*/out/*' \\( -name '*.bash' -o -name '*.bash-completion' \\) 2>/dev/null | while read f; do n=$(basename \"$f\"); n=${n%.bash}; n=${n%.bash-completion}; install -Dm644 \"$f\" %{buildroot}%{_datadir}/bash-completion/completions/\"$n\"; done\n\
find $B -path '*/out/*' -name '_*' 2>/dev/null | while read f; do install -Dm644 \"$f\" %{buildroot}%{_datadir}/zsh/site-functions/\"$(basename \"$f\")\"; done\n\
find $B -path '*/out/*' -name '*.fish' 2>/dev/null | while read f; do install -Dm644 \"$f\" %{buildroot}%{_datadir}/fish/vendor_completions.d/\"$(basename \"$f\")\"; done\n";

// `spec gen`: render SPECS/<key>.spec.
pub(crate) fn gen_spec(conn: &Connection, key: &str) -> Result<(), String> {
    let m = resolve(conn, key)?;
    let items = installs(conn, m.id)?;
    // autotools: run upstream `make install` — captures bin + man + info + locale + all
    // links/extra binaries (gawk->awk, gzip->gunzip/zcat, coreutils applets, …). %files is
    // auto-generated from whatever actually landed, so nothing upstream ships is dropped.
    // cargo/plain-make: explicit install_map (cargo man/completions are a per-pkg follow-up).
    let (install, files_section) = if m.build_system == "stage" {
        // file-staging package: no source/compile — install_cmd copies a declared fileset
        // (headers/libc/crt from $TCSYS, etc.) into %{buildroot}. tc_path_export exposes
        // SYS/CC/CROSS/TCSYS. Auto-filelist packages exactly what landed.
        if m.install_cmd.is_empty() { return Err(format!("vendorctl: stage pkg {key} needs --install-cmd")); }
        let inst = format!(
            "{pathexp}{cmd}\n\
             ( cd %{{buildroot}} && find . -type f -o -type l ) | sed 's#^\\.\\(.*\\)#\"\\1\"#' | LC_ALL=C sort > %{{_builddir}}/{key}.files\n",
            pathexp = tc_path_export(), cmd = m.install_cmd);
        (inst, format!("%files -f %{{_builddir}}/{key}.files"))
    } else if m.build_system == "autotools" || m.build_system == "script" || m.build_system == "meson" {
        let default_install = if m.build_system == "meson" { "DESTDIR=%{buildroot} ninja -C _b install" } else { "make install DESTDIR=%{buildroot} INSTALL='install -p'" };
        let cmd = if m.install_cmd.is_empty() { default_install.to_string() } else { m.install_cmd.clone() };
        let pathexp = tc_path_export();
        let inst = format!(
            "{pathexp}{cmd}\n\
             rm -f %{{buildroot}}%{{_infodir}}/dir\n\
             find %{{buildroot}} -name '*.la' -delete 2>/dev/null || true\n\
             ( cd %{{buildroot}} && find . -type f -o -type l ) | sed 's#^\\.\\(.*\\)#\"\\1\"#' | LC_ALL=C sort > %{{_builddir}}/{key}.files\n");
        (inst, format!("%files -f %{{_builddir}}/{key}.files"))
    } else {
        if items.is_empty() { return Err(format!("vendorctl: no install_map for {key} (add with `install add`)")); }
        let mut inst = install_block(&items);
        // cargo: rust tools emit man pages + shell completions into the build OUT_DIR.
        // Names/locations vary per crate, so glob best-effort and place at standard paths.
        if m.build_system == "cargo" { inst.push_str(CARGO_EXTRAS); }
        // auto-filelist: package exactly what landed in buildroot (bins, links, man, completions).
        inst.push_str(&format!(
            "( cd %{{buildroot}} && find . -type f -o -type l ) | sed 's#^\\.\\(.*\\)#\"\\1\"#' | LC_ALL=C sort > %{{_builddir}}/{key}.files\n"));
        (inst, format!("%files -f %{{_builddir}}/{key}.files"))
    };
    // stage family has no upstream source. Otherwise Source0 = registered tarball filename
    // (rpm %setup handles any compression) or the local-stage default <key>-<ver>.tar.gz.
    let stage = m.build_system == "stage";
    // Source0 = main tarball (%setup'd); Source1.. = secondary tarballs (e.g. perl-cross),
    // extracted over the unpacked tree in %prep so build_args can overlay/use them.
    let srcs = sources_for(conn, m.id)?;
    let src_name = |i: usize, s: &Src| -> String {
        if !s.filename.is_empty() { s.filename.clone() }
        else if i == 0 { format!("{key}-{}.tar.gz", m.version) }
        else { s.url.rsplit('/').next().unwrap_or("source").to_string() }
    };
    let summary = if m.summary.is_empty() { format!("{key} (static-musl, oxide)") } else { m.summary.clone() };
    let license = if m.license.is_empty() { "Unknown".into() } else { m.license.clone() };
    let url = if m.upstream_url.is_empty() { String::new() } else { format!("URL:            {}\n", m.upstream_url) };
    // -n names the unpacked dir explicitly (handles src dirs != %{name}-%{version}, e.g. dua-cli-*).
    let (source_line, prep) = if stage {
        (String::new(), String::new())
    } else {
        let src0 = if srcs.is_empty() { format!("{key}-{}.tar.gz", m.version) } else { src_name(0, &srcs[0]) };
        let mut sl = format!("Source0:        {src0}\n");
        let mut pr = format!("%setup -q -n {}\n", src_subdir(&m, key));
        for (i, s) in srcs.iter().enumerate().skip(1) {
            let fn_ = src_name(i, s);
            sl.push_str(&format!("Source{i}:        {fn_}\n"));
            // extract secondary tarball into the unpacked tree (CWD after %setup)
            pr.push_str(&format!("tar -xf %{{_sourcedir}}/{fn_}\n"));
        }
        (sl, pr)
    };
    let spec = format!(
        "# Generated by vendorctl. build-system: {bs}. Do not hand-edit; edit catalog + regen.\n\
         %global debug_package %{{nil}}\n\
         %global __global_compiler_flags %{{nil}}\n\n\
         Name:           {key}\n\
         Version:        {ver}\n\
         Release:        1%{{?dist}}\n\
         Summary:        {summary}\n\
         License:        {license}\n\
         {url}\
         {source_line}\n\
         %description\n{summary}\n\n\
         %prep\n{prep}\n\n\
         %build\n{build}\n\
         %install\n{install}\n\
         {files_section}\n\
         %changelog\n\
         * Sat Jun 13 2026 Chris Watkins <chris@watkinslabs.com> - {ver}-1\n\
         - Generated oxide spec ({bs} family).\n",
        bs = m.build_system, key = key, ver = m.version, summary = summary, license = license,
        url = url, source_line = source_line, prep = prep, build = build_block(&m)?, install = install, files_section = files_section);
    fs::create_dir_all(tree::specs()).map_err(|e| format!("vendorctl: mkdir SPECS: {e}"))?;
    let path = tree::specs().join(format!("{key}.spec"));
    fs::write(&path, spec).map_err(|e| format!("vendorctl: write {}: {e}", path.display()))?;
    println!("generated\t{}", path.display());
    Ok(())
}

// `build`: rpmbuild -ba per arch, record build_results.
pub(crate) fn build(conn: &Connection, key: &str, arches: &[String]) -> Result<(), String> {
    let m = resolve(conn, key)?;
    let spec = tree::specs().join(format!("{key}.spec"));
    if !spec.is_file() { return Err(format!("vendorctl: {} missing (run `spec gen {key}`)", spec.display())); }
    let topdir = tree::topdir();
    for arch in arches {
        // populate the per-arch sysroot with build_requires deps (Fedora: dnf-install
        // BuildRequires into the mock chroot). Each dep's built RPM is unpacked into the
        // sysroot so this package's %build finds its headers/libs at $SYS/usr/{include,lib}.
        // per-package sysroot: topdir/sysroot/<key>/<arch> — isolated so concurrent builds
        // sharing a dep (e.g. ncurses) don't clobber each other's staging. Cleared fresh.
        let sys = tree::topdir().join("sysroot").join(key).join(arch); // matches spec SYS=.../sysroot/%{name}/%{_target_cpu}
        if !m.build_requires.is_empty() {
            let _ = fs::remove_dir_all(&sys);
            fs::create_dir_all(&sys).map_err(|e| format!("vendorctl: mkdir sysroot: {e}"))?;
        }
        for dep in m.build_requires.split_whitespace() {
            let dm = resolve(conn, dep)?;
            let dep_rpm = tree::rpms().join(arch).join(format!("{dep}-{}-1.ox1.{arch}.rpm", dm.version));
            if !dep_rpm.is_file() {
                return Err(format!("vendorctl: build-dep {dep} not built for {arch} (build it first)"));
            }
            let st = Command::new("sh").arg("-c")
                .arg(format!("rpm2cpio '{}' | cpio -idmu --quiet -D '{}'", dep_rpm.display(), sys.display()))
                .status().map_err(|e| format!("vendorctl: sysroot install {dep}: {e}"))?;
            if !st.success() { return Err(format!("vendorctl: failed to stage {dep} into {arch} sysroot")); }
            println!("sysroot\t{arch}\t<- {dep} {}", dm.version);
        }
        // _topdir MUST be a CLI --define (highest precedence, applied before rpm derives
        // _sourcedir/_builddir). --load is too late for build-path macros — sources would
        // resolve under the default ~/rpmbuild. orch.rs is the single source for these.
        let st = Command::new("rpmbuild")
            .args(["-ba", "--target", arch])
            .args(["--define", &format!("_topdir {}", topdir.display())])
            .args(["--define", "dist .ox1"])
            .args(["--define", "__os_install_post %{nil}"])
            .args(["--define", "_build_id_links none"])
            .arg(spec.to_str().unwrap())
            .status().map_err(|e| format!("vendorctl: rpmbuild: {e}"))?;
        let ok = st.success();
        let rpm = tree::rpms().join(arch).join(format!("{key}-{}-1.ox1.{arch}.rpm", m.version));
        let rpm_s = if ok && rpm.is_file() { rpm.display().to_string() } else { String::new() };
        conn.execute(
            "INSERT INTO build_results(package_version_id,arch,rpm_path,status,built_at) VALUES(?1,?2,?3,?4,?5) \
             ON CONFLICT(package_version_id,arch) DO UPDATE SET rpm_path=?3,status=?4,built_at=?5",
            params![m.id, arch, rpm_s, if ok {"ok"} else {"fail"}, now_ts()],
        ).map_err(|e| format!("vendorctl: record build: {e}"))?;
        if ok {
            println!("built\t{arch}\t{rpm_s}");
            sign_rpm(&rpm)?;
        } else { return Err(format!("vendorctl: build failed: {key} {arch}")); }
    }
    // SRPM is arch-independent — sign once.
    sign_rpm(&topdir.join("SRPMS").join(format!("{key}-{}-1.ox1.src.rpm", m.version)))?;
    Ok(())
}

// `repo create`: createrepo_c over each RPMS/<arch>.
pub(crate) fn create_repo() -> Result<(), String> {
    for arch in tree::ARCHES {
        let dir = tree::rpms().join(arch);
        if !dir.is_dir() { continue; }
        let st = Command::new("createrepo_c").arg("--update").arg(dir.to_str().unwrap())
            .status().map_err(|e| format!("vendorctl: createrepo_c: {e}"))?;
        if !st.success() { return Err(format!("vendorctl: createrepo_c failed for {arch}")); }
        println!("repo\t{}", dir.display());
        // detached, armored signature over repomd.xml (consumers verify with the pubkey).
        let repomd = dir.join("repodata/repomd.xml");
        if repomd.is_file() {
            let _ = fs::remove_file(dir.join("repodata/repomd.xml.asc"));
            let st = Command::new("gpg")
                .args(["--batch", "--yes", "--local-user", &gpg_name(), "--detach-sign", "--armor"])
                .arg(repomd.to_str().unwrap())
                .status().map_err(|e| format!("vendorctl: gpg sign repomd: {e}"))?;
            if !st.success() { return Err(format!("vendorctl: repomd sign failed for {arch}")); }
            println!("signed\t{}", dir.join("repodata/repomd.xml.asc").display());
        }
    }
    Ok(())
}
