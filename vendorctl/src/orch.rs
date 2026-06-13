// Orchestration: drive the canonical rpmbuild tree from the catalog.
// stage source -> generate spec (by build-system family) -> rpmbuild -> createrepo.
use crate::db::now_ts;
use crate::tree;
use rusqlite::{params, Connection, OptionalExtension};
use std::fs;
use std::path::Path;
use std::process::Command;

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
        "SELECT id, version, build_system, summary, license, upstream_url, src_subdir, build_args, cflags, config_cache, ldflags, install_cmd, build_requires \
         FROM package_versions WHERE package_key=?1 ORDER BY id DESC LIMIT 1",
        params![key],
        |r| Ok(VerMeta {
            id: r.get(0)?, version: r.get(1)?, build_system: r.get(2)?, summary: r.get(3)?,
            license: r.get(4)?, upstream_url: r.get(5)?, src_subdir: r.get(6)?, build_args: r.get(7)?,
            cflags: r.get(8)?, config_cache: r.get(9)?, ldflags: r.get(10)?,
            install_cmd: r.get(11)?, build_requires: r.get(12)?,
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

fn sha256_of(path: &Path) -> Result<String, String> {
    let out = Command::new("sha256sum").arg(path).output().map_err(|e| format!("vendorctl: sha256sum: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or("").to_string())
}

// `src fetch`: download canonical_url -> SOURCES/<filename>, verify (or record) sha256.
// Enables distributed builds — a fresh instance needs only the repo + network, no local vendor tree.
pub(crate) fn fetch(conn: &Connection, key: &str) -> Result<(), String> {
    let m = resolve(conn, key)?;
    let s = source_for(conn, m.id)?.filter(|s| !s.url.is_empty())
        .ok_or_else(|| format!("vendorctl: no source URL for {key} (add with `src add --url ...`)"))?;
    let fname = if s.filename.is_empty() { format!("{key}-{}.tar.gz", m.version) } else { s.filename.clone() };
    fs::create_dir_all(tree::sources()).map_err(|e| format!("vendorctl: mkdir SOURCES: {e}"))?;
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

// %build block per build-system family.
fn build_block(m: &VerMeta) -> Result<String, String> {
    // HOST-INDEPENDENT: both arches use the self-contained vendored cross toolchains, which
    // bundle their own musl libc + UAPI headers. No host musl-gcc, no host /usr/include copy.
    let vr = tree::vendor_root();
    let cc_x86 = vr.join("cross/x86_64-linux-musl-cross/bin/x86_64-linux-musl-gcc");
    let cc_arm = vr.join("cross/aarch64-linux-musl-cross/bin/aarch64-linux-musl-gcc");
    // SYS = per-arch sysroot where build_requires deps are installed; build against it.
    let preamble = format!(
        "SYS={topdir}/sysroot/%{{_target_cpu}}\n\
         if [ \"%{{_target_cpu}}\" = \"aarch64\" ]; then CC={ccarm}; CROSS={parm}; \
         else CC={ccx86}; CROSS={px86}; fi\n\
         UAPI=\"\"\n\
         export AR=\"${{CROSS}}ar\" RANLIB=\"${{CROSS}}ranlib\" NM=\"${{CROSS}}nm\" STRIP=\"${{CROSS}}strip\"\n\
         export C_INCLUDE_PATH=\"$SYS/usr/include\" CPLUS_INCLUDE_PATH=\"$SYS/usr/include\" LIBRARY_PATH=\"$SYS/usr/lib\"\n",
        topdir = tree::topdir().display(),
        ccarm = cc_arm.display(), parm = cc_arm.display().to_string().trim_end_matches("gcc"),
        ccx86 = cc_x86.display(), px86 = cc_x86.display().to_string().trim_end_matches("gcc"));
    let b = &m.build_args;
    let cf = &m.cflags; // extra per-package CFLAGS (e.g. -std=gnu89)
    let lf = &m.ldflags; // extra per-package LDFLAGS (e.g. -L<dep>/lib for shared deps)
    Ok(match m.build_system.as_str() {
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
             find . \\( -name '*.o' -o -name '*.a' -o -name '*.lo' -o -name '*.la' \\) -delete 2>/dev/null || true\n\
             {cache_write}\
             CC=\"$CC\" CC_FOR_BUILD=gcc LDFLAGS_FOR_BUILD=\"\" \\\n\
             CFLAGS_FOR_BUILD=\"-D_GNU_SOURCE -Wno-implicit-function-declaration -Wno-incompatible-pointer-types -Wno-int-conversion\" \\\n\
             CFLAGS=\"-Os -D_GNU_SOURCE {cf} -I$SYS/usr/include -Wno-implicit-function-declaration -Wno-incompatible-pointer-types -Wno-int-conversion $UAPI\" \\\n\
             LDFLAGS=\"-Wl,-rpath,/usr/lib -L$SYS/usr/lib {lf}\" \\\n\
             PKG_CONFIG_PATH=\"$SYS/usr/lib/pkgconfig\" \\\n\
             LD_LIBRARY_PATH=\"$SYS/usr/lib\" \\\n\
             ./configure --build=x86_64-pc-linux-gnu --host=%{{_target_cpu}}-linux-musl {cache_flag}{b}\n\
             make %{{?_smp_mflags}}\n")
        },
        // custom build systems (zlib ./configure, openssl ./Configure): build_args is the full
        // configure+make snippet; CC/CROSS/SYS/UAPI exported. %install uses install_cmd.
        "script" => format!(
            "{preamble}\
             export CC CROSS UAPI\n\
             export CFLAGS=\"-Os -fPIC {cf} -I$SYS/usr/include $UAPI\"\n\
             export LDFLAGS=\"-Wl,-rpath,/usr/lib -L$SYS/usr/lib {lf}\"\n\
             export PKG_CONFIG_PATH=\"$SYS/usr/lib/pkgconfig\"\n\
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
            ccarm = cc_arm.display(), ccx86 = cc_x86.display(), b = b),
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
    let (install, files_section) = if m.build_system == "autotools" || m.build_system == "script" {
        let cmd = if m.install_cmd.is_empty() { "make install DESTDIR=%{buildroot} INSTALL='install -p'".to_string() } else { m.install_cmd.clone() };
        let inst = format!(
            "{cmd}\n\
             rm -f %{{buildroot}}%{{_infodir}}/dir\n\
             find %{{buildroot}} -name '*.la' -delete 2>/dev/null || true\n\
             ( cd %{{buildroot}} && find . -type f -o -type l ) | sed 's#^\\.##' | LC_ALL=C sort > %{{_builddir}}/{key}.files\n");
        (inst, format!("%files -f %{{_builddir}}/{key}.files"))
    } else {
        if items.is_empty() { return Err(format!("vendorctl: no install_map for {key} (add with `install add`)")); }
        let mut inst = install_block(&items);
        // cargo: rust tools emit man pages + shell completions into the build OUT_DIR.
        // Names/locations vary per crate, so glob best-effort and place at standard paths.
        if m.build_system == "cargo" { inst.push_str(CARGO_EXTRAS); }
        // auto-filelist: package exactly what landed in buildroot (bins, links, man, completions).
        inst.push_str(&format!(
            "( cd %{{buildroot}} && find . -type f -o -type l ) | sed 's#^\\.##' | LC_ALL=C sort > %{{_builddir}}/{key}.files\n"));
        (inst, format!("%files -f %{{_builddir}}/{key}.files"))
    };
    // Source0 = upstream tarball filename if a source URL is registered (rpm %setup handles
    // any compression); else the local-stage default. %setup -n still pins the unpacked dir.
    let src0 = match source_for(conn, m.id)? {
        Some(s) if !s.filename.is_empty() => s.filename,
        _ => format!("{key}-{}.tar.gz", m.version),
    };
    let summary = if m.summary.is_empty() { format!("{key} (static-musl, oxide)") } else { m.summary.clone() };
    let license = if m.license.is_empty() { "Unknown".into() } else { m.license.clone() };
    let url = if m.upstream_url.is_empty() { String::new() } else { format!("URL:            {}\n", m.upstream_url) };
    // -n names the unpacked dir explicitly (handles src dirs != %{name}-%{version}, e.g. dua-cli-*).
    let prep = format!("%setup -q -n {}", src_subdir(&m, key));
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
         Source0:        {src0}\n\n\
         %description\n{summary}\n\n\
         %prep\n{prep}\n\n\
         %build\n{build}\n\
         %install\n{install}\n\
         {files_section}\n\
         %changelog\n\
         * Sat Jun 13 2026 Chris Watkins <chris@watkinslabs.com> - {ver}-1\n\
         - Generated oxide spec ({bs} family).\n",
        bs = m.build_system, key = key, ver = m.version, summary = summary, license = license,
        url = url, src0 = src0, prep = prep, build = build_block(&m)?, install = install, files_section = files_section);
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
        for dep in m.build_requires.split_whitespace() {
            let dm = resolve(conn, dep)?;
            let dep_rpm = tree::rpms().join(arch).join(format!("{dep}-{}-1.ox1.{arch}.rpm", dm.version));
            if !dep_rpm.is_file() {
                return Err(format!("vendorctl: build-dep {dep} not built for {arch} (build it first)"));
            }
            let sys = tree::sysroot(arch);
            fs::create_dir_all(&sys).map_err(|e| format!("vendorctl: mkdir sysroot: {e}"))?;
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
