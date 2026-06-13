mod db;
mod tree;
mod orch;

use db::{CatalogExport, PackageVersion, ReleaseMembership, Source};
use rusqlite::{params, Connection};
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let (args, db_override) = strip_global_flag(&raw, "--db");
    if args.is_empty() { return usage(); }
    let db_path = db_override
        .map(PathBuf::from)
        .or_else(|| parse_flag(&args, "--db").map(PathBuf::from))
        .or_else(|| db::default_db_path().ok())
        .ok_or_else(|| "vendorctl: default db path unavailable".to_string())?;
    let mut conn = db::open(&db_path)?;

    match args[0].as_str() {
        "db" => cmd_db(&conn, &args[1..], &db_path),
        "repo" => cmd_repo(&conn, &args[1..]),
        "pkg" => cmd_pkg(&conn, &args[1..]),
        "ver" => cmd_ver(&conn, &args[1..]),
        "src" => cmd_src(&conn, &args[1..]),
        "pin" => cmd_pin(&conn, &args[1..]),
        "export" => cmd_export(&conn, &args[1..]),
        "import" => cmd_import(&mut conn, &args[1..]),
        // orchestration: drive the canonical rpmbuild tree
        "meta" => cmd_meta(&conn, &args[1..]),
        "install" => cmd_install(&conn, &args[1..]),
        "stage" => orch::stage(&conn, req_pos(&args, 1, "stage <pkg>")?),
        "spec" => {
            let sub = req_sub(&args[1..], "spec gen <pkg>")?;
            if sub != "gen" { return Err(format!("vendorctl: unknown spec command `{sub}`")); }
            orch::gen_spec(&conn, req_pos(&args, 2, "spec gen <pkg>")?)
        }
        "build" => orch::build(&conn, req_pos(&args, 1, "build <pkg> [--arch x86_64|aarch64|both]")?, &arch_list(&args)),
        "plan" => orch::plan(&conn, &pkg_args(&args[1..])),
        "graph" => orch::graph(&conn, &pkg_args(&args[1..])),
        "build-all" => orch::build_all(&conn, &arch_list(&args), &pkg_args(&args[1..])),
        "publish" => orch::create_repo(),
        "all" => {
            let key = req_pos(&args, 1, "all <pkg> [--arch both]")?;
            orch::stage(&conn, key)?;
            orch::gen_spec(&conn, key)?;
            orch::build(&conn, key, &arch_list(&args))
        }
        "-h" | "--help" => usage(),
        other => Err(format!("vendorctl: unknown command `{other}`")),
    }
}

// positional package names (skip flags + their values, e.g. --arch <v>).
fn pkg_args(args: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip { skip = false; continue; }
        if a == "--arch" { skip = true; continue; }
        if a.starts_with('-') { continue; }
        out.push(a.clone());
    }
    out
}

// --arch x86_64|aarch64|both (default both).
fn arch_list(args: &[String]) -> Vec<String> {
    match parse_flag(args, "--arch") {
        Some("x86_64") => vec!["x86_64".into()],
        Some("aarch64") => vec!["aarch64".into()],
        _ => tree::ARCHES.iter().map(|s| s.to_string()).collect(),
    }
}

// meta set <pkg> [--build-system ..] [--summary ..] [--license ..] [--url ..] [--src-subdir ..] [--build-args ..]
fn cmd_meta(conn: &Connection, args: &[String]) -> Result<(), String> {
    let sub = req_sub(args, "meta set <pkg> [--build-system ..] [--summary ..] [--license ..] [--url ..] [--src-subdir ..] [--build-args ..]")?;
    if sub != "set" { return Err(format!("vendorctl: unknown meta command `{sub}`")); }
    let key = req_pos(args, 1, "meta set <pkg> ...")?;
    let m = orch::resolve(conn, key)?;
    let upd: &[(&str, Option<&str>)] = &[
        ("build_system", parse_flag(args, "--build-system")),
        ("summary", parse_flag(args, "--summary")),
        ("license", parse_flag(args, "--license")),
        ("upstream_url", parse_flag(args, "--url")),
        ("src_subdir", parse_flag(args, "--src-subdir")),
        ("build_args", parse_flag(args, "--build-args")),
        ("cflags", parse_flag(args, "--cflags")),
        ("config_cache", parse_flag(args, "--config-cache")),
        ("ldflags", parse_flag(args, "--ldflags")),
        ("install_cmd", parse_flag(args, "--install-cmd")),
        ("build_requires", parse_flag(args, "--build-requires")),
    ];
    for (col, val) in upd {
        if let Some(v) = val {
            conn.execute(&format!("UPDATE package_versions SET {col}=?1 WHERE id=?2"), params![v, m.id])
                .map_err(|e| format!("vendorctl: meta set {col}: {e}"))?;
        }
    }
    println!("meta\t{key}\t{}", m.version);
    Ok(())
}

// install add <pkg> --dest <path> [--src <p>] [--kind bin|file|symlink|hardlink|tree] [--link-target <t>] [--mode 0755]
// install list <pkg> | install clear <pkg>
fn cmd_install(conn: &Connection, args: &[String]) -> Result<(), String> {
    let sub = req_sub(args, "install <add|list|clear> <pkg> ...")?;
    let key = req_pos(args, 1, "install <add|list|clear> <pkg> ...")?;
    let m = orch::resolve(conn, key)?;
    match sub {
        "add" => {
            let dest = req_flag(args, "--dest", "install add <pkg> --dest <path> ...")?;
            conn.execute(
                "INSERT INTO install_map(package_version_id,src,dest,kind,link_target,mode) VALUES(?1,?2,?3,?4,?5,?6)",
                params![m.id, flag_or(args, "--src", ""), dest, flag_or(args, "--kind", "bin"),
                        flag_or(args, "--link-target", ""), flag_or(args, "--mode", "0755")],
            ).map_err(|e| format!("vendorctl: install add: {e}"))?;
            println!("install\t{key}\t{dest}");
            Ok(())
        }
        "list" => {
            for it in orch::installs(conn, m.id)? {
                println!("{}\t{}\t{}\t{}\t{}", it.kind, it.src, it.dest, it.link_target, it.mode);
            }
            Ok(())
        }
        "clear" => {
            conn.execute("DELETE FROM install_map WHERE package_version_id=?1", params![m.id])
                .map_err(|e| format!("vendorctl: install clear: {e}"))?;
            println!("cleared\t{key}");
            Ok(())
        }
        other => Err(format!("vendorctl: unknown install command `{other}`")),
    }
}

fn cmd_db(conn: &Connection, args: &[String], db_path: &std::path::Path) -> Result<(), String> {
    let sub = req_sub(args, "db <init|path>")?;
    match sub {
        "init" => {
            let _ = conn;
            println!("{}", db_path.display());
            Ok(())
        }
        "path" => {
            println!("{}", db_path.display());
            Ok(())
        }
        other => Err(format!("vendorctl: unknown db command `{other}`")),
    }
}

fn cmd_repo(conn: &Connection, args: &[String]) -> Result<(), String> {
    let sub = req_sub(args, "repo <add|list|update|delete>")?;
    match sub {
        "add" => {
            let id = req_pos(args, 1, "repo add <id>")?;
            let status = flag_or(args, "--status", "draft");
            let description = flag_or(args, "--description", "");
            db::add_repo(conn, id, &status, &description)?;
            println!("added repo\t{id}");
            Ok(())
        }
        "list" => {
            for row in db::export_catalog(conn)?.repo_releases {
                println!("{}\t{}\t{}\t{}\t{}", row.id, row.status, row.description, row.created_at, opt_i64(row.frozen_at));
            }
            Ok(())
        }
        "update" => {
            let id = req_pos(args, 1, "repo update <id>")?;
            db::update_repo(conn, id, parse_flag(args, "--status"), parse_flag(args, "--description"))?;
            println!("updated repo\t{id}");
            Ok(())
        }
        "delete" => {
            let id = req_pos(args, 1, "repo delete <id>")?;
            db::delete_repo(conn, id)?;
            println!("deleted repo\t{id}");
            Ok(())
        }
        other => Err(format!("vendorctl: unknown repo command `{other}`")),
    }
}

fn cmd_pkg(conn: &Connection, args: &[String]) -> Result<(), String> {
    let sub = req_sub(args, "pkg <add|list|update|delete>")?;
    match sub {
        "add" => {
            let key = req_pos(args, 1, "pkg add <key>")?;
            let display_name = parse_flag(args, "--display-name").unwrap_or(key);
            let category = flag_or(args, "--category", "runtime");
            let owner = flag_or(args, "--owner", "");
            db::add_package(
                conn,
                key,
                display_name,
                &category,
                &owner,
                flag_or(args, "--enabled", "true").eq_ignore_ascii_case("true"),
            )?;
            println!("added package\t{key}");
            Ok(())
        }
        "list" => {
            for row in db::export_catalog(conn)?.packages {
                println!("{}\t{}\t{}\t{}\t{}", row.key, row.display_name, row.category, row.owner, row.enabled);
            }
            Ok(())
        }
        "update" => {
            let key = req_pos(args, 1, "pkg update <key>")?;
            let enabled = parse_flag(args, "--enabled").map(|v| v.eq_ignore_ascii_case("true"));
            db::update_package(conn, key, parse_flag(args, "--display-name"), parse_flag(args, "--category"), parse_flag(args, "--owner"), enabled)?;
            println!("updated package\t{key}");
            Ok(())
        }
        "delete" => {
            let key = req_pos(args, 1, "pkg delete <key>")?;
            db::delete_package(conn, key)?;
            println!("deleted package\t{key}");
            Ok(())
        }
        other => Err(format!("vendorctl: unknown pkg command `{other}`")),
    }
}

fn cmd_ver(conn: &Connection, args: &[String]) -> Result<(), String> {
    let sub = req_sub(args, "ver <add|list|update|delete>")?;
    match sub {
        "add" => {
            let row = version_from_args(args)?;
            db::add_version(conn, &row)?;
            println!("added version\t{}\t{}\t{}", row.package_key, row.version, row.revision);
            Ok(())
        }
        "list" => {
            let cat = db::export_catalog(conn)?;
            for row in cat.package_versions.into_iter().filter(|r| match parse_flag(args, "--package") {
                Some(pkg) => r.package_key == pkg,
                None => true,
            }) {
                println!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}", row.package_key, row.version, row.revision, row.source_type, row.integrity_hash, row.patchset_id, row.build_recipe_id, row.status);
            }
            Ok(())
        }
        "update" => {
            let key = req_flag(args, "--package", "ver update --package <pkg> --version <v> [--revision <r>]")?;
            let version = req_flag(args, "--version", "ver update --package <pkg> --version <v> [--revision <r>]")?;
            let revision = flag_or(args, "--revision", "");
            db::update_version(conn, key, version, &revision, parse_flag(args, "--source-type"), parse_flag(args, "--integrity-hash"), parse_flag(args, "--patchset-id"), parse_flag(args, "--build-recipe-id"), parse_flag(args, "--status"))?;
            println!("updated version\t{key}\t{version}\t{revision}");
            Ok(())
        }
        "delete" => {
            let key = req_flag(args, "--package", "ver delete --package <pkg> --version <v> [--revision <r>]")?;
            let version = req_flag(args, "--version", "ver delete --package <pkg> --version <v> [--revision <r>]")?;
            let revision = flag_or(args, "--revision", "");
            db::delete_version(conn, key, version, &revision)?;
            println!("deleted version\t{key}\t{version}\t{revision}");
            Ok(())
        }
        other => Err(format!("vendorctl: unknown ver command `{other}`")),
    }
}

fn cmd_src(conn: &Connection, args: &[String]) -> Result<(), String> {
    let sub = req_sub(args, "src <add|list|update|delete|fetch>")?;
    match sub {
        "fetch" => return orch::fetch(conn, req_pos(args, 1, "src fetch <pkg>")?),
        "add" => {
            let row = source_from_args(args, true)?;
            db::add_source(conn, &row)?;
            println!("added source\t{}\t{}\t{}\t{}", row.package_key, row.version, row.revision, row.canonical_url);
            Ok(())
        }
        "list" => {
            let key = parse_flag(args, "--package");
            let version = parse_flag(args, "--version");
            let revision = parse_flag(args, "--revision").unwrap_or("");
            for row in db::export_catalog(conn)?.sources.into_iter().filter(|r| {
                key.map(|v| r.package_key == v).unwrap_or(true)
                && version.map(|v| r.version == v).unwrap_or(true)
                && (parse_flag(args, "--revision").is_none() || r.revision == revision)
            }) {
                println!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}", row.package_key, row.version, row.revision, row.canonical_url, row.mirror_url, row.filename, row.checksum_type, row.checksum_value, opt_i64(row.size_bytes), row.signature_url);
            }
            Ok(())
        }
        "update" => {
            let old_url = req_flag(args, "--old-url", "src update --package <pkg> --version <v> [--revision <r>] --old-url <url> [--url <new>]")?;
            let mut row = source_from_args(args, false)?;
            if row.canonical_url.is_empty() { row.canonical_url = old_url.to_string(); }
            db::update_source(conn, &row, old_url)?;
            println!("updated source\t{}\t{}\t{}\t{}", row.package_key, row.version, row.revision, row.canonical_url);
            Ok(())
        }
        "delete" => {
            let key = req_flag(args, "--package", "src delete --package <pkg> --version <v> [--revision <r>] --url <url>")?;
            let version = req_flag(args, "--version", "src delete --package <pkg> --version <v> [--revision <r>] --url <url>")?;
            let revision = flag_or(args, "--revision", "");
            let url = req_flag(args, "--url", "src delete --package <pkg> --version <v> [--revision <r>] --url <url>")?;
            db::delete_source(conn, key, version, &revision, url)?;
            println!("deleted source\t{key}\t{version}\t{revision}\t{url}");
            Ok(())
        }
        other => Err(format!("vendorctl: unknown src command `{other}`")),
    }
}

fn cmd_pin(conn: &Connection, args: &[String]) -> Result<(), String> {
    let sub = req_sub(args, "pin <add|list|delete>")?;
    match sub {
        "add" => {
            let row = pin_from_args(args)?;
            db::add_pin(conn, &row.repo_release_id, &row.package_key, &row.version, &row.revision)?;
            println!("added pin\t{}\t{}\t{}\t{}", row.repo_release_id, row.package_key, row.version, row.revision);
            Ok(())
        }
        "list" => {
            for row in db::export_catalog(conn)?.release_membership.into_iter().filter(|r| match parse_flag(args, "--repo") {
                Some(repo) => r.repo_release_id == repo,
                None => true,
            }) {
                println!("{}\t{}\t{}\t{}", row.repo_release_id, row.package_key, row.version, row.revision);
            }
            Ok(())
        }
        "delete" => {
            let row = pin_from_args(args)?;
            db::delete_pin(conn, &row.repo_release_id, &row.package_key, &row.version, &row.revision)?;
            println!("deleted pin\t{}\t{}\t{}\t{}", row.repo_release_id, row.package_key, row.version, row.revision);
            Ok(())
        }
        other => Err(format!("vendorctl: unknown pin command `{other}`")),
    }
}

fn cmd_export(conn: &Connection, args: &[String]) -> Result<(), String> {
    let fmt = req_sub(args, "export <json> [--out <path>]")?;
    match fmt {
        "json" => {
            let cat = db::export_catalog(conn)?;
            let txt = serde_json::to_string_pretty(&cat).map_err(|e| format!("vendorctl: encode json: {e}"))?;
            if let Some(out) = parse_flag(args, "--out") {
                let path = PathBuf::from(out);
                if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|e| format!("vendorctl: mkdir {}: {e}", parent.display()))?; }
                fs::write(&path, txt).map_err(|e| format!("vendorctl: write {}: {e}", path.display()))?;
                println!("{}", path.display());
            } else {
                println!("{txt}");
            }
            Ok(())
        }
        other => Err(format!("vendorctl: unknown export format `{other}`")),
    }
}

fn cmd_import(conn: &mut Connection, args: &[String]) -> Result<(), String> {
    let fmt = req_sub(args, "import <json> --in <path>")?;
    match fmt {
        "json" => {
            let path = PathBuf::from(req_flag(args, "--in", "import json --in <path>")?);
            let txt = fs::read_to_string(&path).map_err(|e| format!("vendorctl: read {}: {e}", path.display()))?;
            let cat: CatalogExport = serde_json::from_str(&txt).map_err(|e| format!("vendorctl: parse {}: {e}", path.display()))?;
            db::import_catalog(conn, &cat)?;
            println!("{}", path.display());
            Ok(())
        }
        other => Err(format!("vendorctl: unknown import format `{other}`")),
    }
}

fn version_from_args(args: &[String]) -> Result<PackageVersion, String> {
    Ok(PackageVersion {
        package_key: req_flag(args, "--package", "ver add --package <pkg> --version <v> [--revision <r>]")?.to_string(),
        version: req_flag(args, "--version", "ver add --package <pkg> --version <v> [--revision <r>]")?.to_string(),
        revision: flag_or(args, "--revision", ""),
        source_type: flag_or(args, "--source-type", "tarball"),
        integrity_hash: flag_or(args, "--integrity-hash", ""),
        patchset_id: flag_or(args, "--patchset-id", ""),
        build_recipe_id: flag_or(args, "--build-recipe-id", ""),
        status: flag_or(args, "--status", "draft"),
    })
}

fn source_from_args(args: &[String], require_url: bool) -> Result<Source, String> {
    let canonical_url = if require_url {
        req_flag(args, "--url", "src add --package <pkg> --version <v> [--revision <r>] --url <url>")?.to_string()
    } else {
        flag_or(args, "--url", "")
    };
    Ok(Source {
        package_key: req_flag(args, "--package", "src add --package <pkg> --version <v> [--revision <r>] --url <url>")?.to_string(),
        version: req_flag(args, "--version", "src add --package <pkg> --version <v> [--revision <r>] --url <url>")?.to_string(),
        revision: flag_or(args, "--revision", ""),
        canonical_url,
        mirror_url: flag_or(args, "--mirror-url", ""),
        filename: flag_or(args, "--filename", ""),
        checksum_type: flag_or(args, "--checksum-type", "sha256"),
        checksum_value: flag_or(args, "--checksum-value", ""),
        size_bytes: parse_flag(args, "--size-bytes").map(|v| v.parse::<i64>()).transpose().map_err(|e| format!("vendorctl: --size-bytes parse: {e}"))?,
        signature_url: flag_or(args, "--signature-url", ""),
    })
}

fn pin_from_args(args: &[String]) -> Result<ReleaseMembership, String> {
    Ok(ReleaseMembership {
        repo_release_id: req_flag(args, "--repo", "pin add --repo <repo> --package <pkg> --version <v> [--revision <r>]")?.to_string(),
        package_key: req_flag(args, "--package", "pin add --repo <repo> --package <pkg> --version <v> [--revision <r>]")?.to_string(),
        version: req_flag(args, "--version", "pin add --repo <repo> --package <pkg> --version <v> [--revision <r>]")?.to_string(),
        revision: flag_or(args, "--revision", ""),
    })
}

fn req_sub<'a>(args: &'a [String], usage: &str) -> Result<&'a str, String> {
    args.first().map(|s| s.as_str()).ok_or_else(|| format!("usage: vendorctl {usage}"))
}

fn req_pos<'a>(args: &'a [String], idx: usize, usage: &str) -> Result<&'a str, String> {
    args.get(idx).map(|s| s.as_str()).ok_or_else(|| format!("usage: vendorctl {usage}"))
}

fn req_flag<'a>(args: &'a [String], flag: &str, usage: &str) -> Result<&'a str, String> {
    parse_flag(args, flag).ok_or_else(|| format!("usage: vendorctl {usage}"))
}

fn parse_flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == flag {
            return args.get(i + 1).map(|v| v.as_str());
        }
        if let Some(v) = a.strip_prefix(&format!("{flag}=")) {
            return Some(v);
        }
        i += 1;
    }
    None
}

fn strip_global_flag(args: &[String], flag: &str) -> (Vec<String>, Option<String>) {
    let mut out = Vec::with_capacity(args.len());
    let mut value = None;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == flag {
            value = args.get(i + 1).cloned();
            i += 2;
            continue;
        }
        if let Some(v) = a.strip_prefix(&format!("{flag}=")) {
            value = Some(v.to_string());
            i += 1;
            continue;
        }
        out.push(args[i].clone());
        i += 1;
    }
    (out, value)
}

fn flag_or(args: &[String], flag: &str, default: &str) -> String {
    parse_flag(args, flag).unwrap_or(default).to_string()
}

fn opt_i64(v: Option<i64>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

fn usage() -> Result<(), String> {
    Err("usage: vendorctl <db|repo|pkg|ver|src|pin|export|import> ...".into())
}
