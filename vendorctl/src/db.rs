use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CatalogExport {
    pub(crate) repo_releases: Vec<RepoRelease>,
    pub(crate) packages: Vec<Package>,
    pub(crate) package_versions: Vec<PackageVersion>,
    pub(crate) sources: Vec<Source>,
    pub(crate) release_membership: Vec<ReleaseMembership>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct RepoRelease {
    pub(crate) id: String,
    pub(crate) status: String,
    pub(crate) description: String,
    pub(crate) created_at: i64,
    pub(crate) frozen_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Package {
    pub(crate) key: String,
    pub(crate) display_name: String,
    pub(crate) category: String,
    pub(crate) owner: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PackageVersion {
    pub(crate) package_key: String,
    pub(crate) version: String,
    pub(crate) revision: String,
    pub(crate) source_type: String,
    pub(crate) integrity_hash: String,
    pub(crate) patchset_id: String,
    pub(crate) build_recipe_id: String,
    pub(crate) status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Source {
    pub(crate) package_key: String,
    pub(crate) version: String,
    pub(crate) revision: String,
    pub(crate) canonical_url: String,
    pub(crate) mirror_url: String,
    pub(crate) filename: String,
    pub(crate) checksum_type: String,
    pub(crate) checksum_value: String,
    pub(crate) size_bytes: Option<i64>,
    pub(crate) signature_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ReleaseMembership {
    pub(crate) repo_release_id: String,
    pub(crate) package_key: String,
    pub(crate) version: String,
    pub(crate) revision: String,
}

pub(crate) fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn default_db_path() -> Result<PathBuf, String> {
    Ok(crate::tree::topdir().join("catalog.db"))
}

pub(crate) fn open(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("vendorctl: mkdir {}: {e}", parent.display()))?;
    }
    let conn = Connection::open(db_path).map_err(|e| format!("vendorctl: open {}: {e}", db_path.display()))?;
    // parallel builds: many vendorctl processes write build_results concurrently. WAL +
    // a generous busy_timeout let them serialize gracefully instead of "database locked".
    conn.busy_timeout(std::time::Duration::from_secs(60))
        .map_err(|e| format!("vendorctl: busy_timeout: {e}"))?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    conn.execute_batch(include_str!("../schema.sql"))
        .map_err(|e| format!("vendorctl: init schema: {e}"))?;
    // migrations for dbs created before a column existed (ignore "duplicate column").
    let _ = conn.execute("ALTER TABLE package_versions ADD COLUMN cflags TEXT NOT NULL DEFAULT ''", []);
    Ok(conn)
}

pub(crate) fn add_repo(conn: &Connection, id: &str, status: &str, description: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO repo_releases(id, status, description, created_at) VALUES(?1, ?2, ?3, ?4)",
        params![id, status, description, now_ts()],
    ).map_err(|e| format!("vendorctl: add repo `{id}`: {e}"))?;
    Ok(())
}

pub(crate) fn update_repo(conn: &Connection, id: &str, status: Option<&str>, description: Option<&str>) -> Result<(), String> {
    let changed = conn.execute(
        "UPDATE repo_releases
            SET status = COALESCE(?2, status),
                description = COALESCE(?3, description)
          WHERE id = ?1",
        params![id, status, description],
    ).map_err(|e| format!("vendorctl: update repo `{id}`: {e}"))?;
    if changed == 0 { return Err(format!("vendorctl: repo `{id}` not found")); }
    Ok(())
}

pub(crate) fn delete_repo(conn: &Connection, id: &str) -> Result<(), String> {
    let changed = conn.execute("DELETE FROM repo_releases WHERE id = ?1", params![id])
        .map_err(|e| format!("vendorctl: delete repo `{id}`: {e}"))?;
    if changed == 0 { return Err(format!("vendorctl: repo `{id}` not found")); }
    Ok(())
}

pub(crate) fn add_package(conn: &Connection, key: &str, display_name: &str, category: &str, owner: &str, enabled: bool) -> Result<(), String> {
    conn.execute(
        "INSERT INTO packages(key, display_name, category, owner, enabled) VALUES(?1, ?2, ?3, ?4, ?5)",
        params![key, display_name, category, owner, if enabled { 1 } else { 0 }],
    ).map_err(|e| format!("vendorctl: add package `{key}`: {e}"))?;
    Ok(())
}

pub(crate) fn update_package(conn: &Connection, key: &str, display_name: Option<&str>, category: Option<&str>, owner: Option<&str>, enabled: Option<bool>) -> Result<(), String> {
    let enabled_i = enabled.map(|v| if v { 1 } else { 0 });
    let changed = conn.execute(
        "UPDATE packages
            SET display_name = COALESCE(?2, display_name),
                category = COALESCE(?3, category),
                owner = COALESCE(?4, owner),
                enabled = COALESCE(?5, enabled)
          WHERE key = ?1",
        params![key, display_name, category, owner, enabled_i],
    ).map_err(|e| format!("vendorctl: update package `{key}`: {e}"))?;
    if changed == 0 { return Err(format!("vendorctl: package `{key}` not found")); }
    Ok(())
}

pub(crate) fn delete_package(conn: &Connection, key: &str) -> Result<(), String> {
    let changed = conn.execute("DELETE FROM packages WHERE key = ?1", params![key])
        .map_err(|e| format!("vendorctl: delete package `{key}`: {e}"))?;
    if changed == 0 { return Err(format!("vendorctl: package `{key}` not found")); }
    Ok(())
}

pub(crate) fn add_version(conn: &Connection, row: &PackageVersion) -> Result<(), String> {
    conn.execute(
        "INSERT INTO package_versions(package_key, version, revision, source_type, integrity_hash, patchset_id, build_recipe_id, status)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![row.package_key, row.version, row.revision, row.source_type, row.integrity_hash, row.patchset_id, row.build_recipe_id, row.status],
    ).map_err(|e| format!("vendorctl: add version `{}` {} `{}`: {e}", row.package_key, row.version, row.revision))?;
    Ok(())
}

pub(crate) fn update_version(conn: &Connection, key: &str, version: &str, revision: &str, source_type: Option<&str>, integrity_hash: Option<&str>, patchset_id: Option<&str>, build_recipe_id: Option<&str>, status: Option<&str>) -> Result<(), String> {
    let changed = conn.execute(
        "UPDATE package_versions
            SET source_type = COALESCE(?4, source_type),
                integrity_hash = COALESCE(?5, integrity_hash),
                patchset_id = COALESCE(?6, patchset_id),
                build_recipe_id = COALESCE(?7, build_recipe_id),
                status = COALESCE(?8, status)
          WHERE package_key = ?1 AND version = ?2 AND revision = ?3",
        params![key, version, revision, source_type, integrity_hash, patchset_id, build_recipe_id, status],
    ).map_err(|e| format!("vendorctl: update version `{key}` {version} `{revision}`: {e}"))?;
    if changed == 0 { return Err(format!("vendorctl: version `{key}` {version} `{revision}` not found")); }
    Ok(())
}

pub(crate) fn delete_version(conn: &Connection, key: &str, version: &str, revision: &str) -> Result<(), String> {
    let changed = conn.execute(
        "DELETE FROM package_versions WHERE package_key = ?1 AND version = ?2 AND revision = ?3",
        params![key, version, revision],
    ).map_err(|e| format!("vendorctl: delete version `{key}` {version} `{revision}`: {e}"))?;
    if changed == 0 { return Err(format!("vendorctl: version `{key}` {version} `{revision}` not found")); }
    Ok(())
}

pub(crate) fn resolve_package_version_id(conn: &Connection, key: &str, version: &str, revision: &str) -> Result<i64, String> {
    conn.query_row(
        "SELECT id FROM package_versions WHERE package_key = ?1 AND version = ?2 AND revision = ?3",
        params![key, version, revision],
        |row| row.get(0),
    ).optional()
     .map_err(|e| format!("vendorctl: resolve version `{key}` {version} `{revision}`: {e}"))?
     .ok_or_else(|| format!("vendorctl: version `{key}` {version} `{revision}` not found"))
}

pub(crate) fn add_source(conn: &Connection, row: &Source) -> Result<(), String> {
    let pv_id = resolve_package_version_id(conn, &row.package_key, &row.version, &row.revision)?;
    conn.execute(
        "INSERT INTO sources(package_version_id, canonical_url, mirror_url, filename, checksum_type, checksum_value, size_bytes, signature_url)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![pv_id, row.canonical_url, row.mirror_url, row.filename, row.checksum_type, row.checksum_value, row.size_bytes, row.signature_url],
    ).map_err(|e| format!("vendorctl: add source to `{}` {} `{}`: {e}", row.package_key, row.version, row.revision))?;
    Ok(())
}

pub(crate) fn update_source(conn: &Connection, row: &Source, old_url: &str) -> Result<(), String> {
    let pv_id = resolve_package_version_id(conn, &row.package_key, &row.version, &row.revision)?;
    let changed = conn.execute(
        "UPDATE sources
            SET canonical_url = ?2,
                mirror_url = ?3,
                filename = ?4,
                checksum_type = ?5,
                checksum_value = ?6,
                size_bytes = ?7,
                signature_url = ?8
          WHERE package_version_id = ?1 AND canonical_url = ?9",
        params![pv_id, row.canonical_url, row.mirror_url, row.filename, row.checksum_type, row.checksum_value, row.size_bytes, row.signature_url, old_url],
    ).map_err(|e| format!("vendorctl: update source `{old_url}`: {e}"))?;
    if changed == 0 { return Err(format!("vendorctl: source `{old_url}` not found for `{}` {} `{}`", row.package_key, row.version, row.revision)); }
    Ok(())
}

pub(crate) fn delete_source(conn: &Connection, key: &str, version: &str, revision: &str, url: &str) -> Result<(), String> {
    let pv_id = resolve_package_version_id(conn, key, version, revision)?;
    let changed = conn.execute(
        "DELETE FROM sources WHERE package_version_id = ?1 AND canonical_url = ?2",
        params![pv_id, url],
    ).map_err(|e| format!("vendorctl: delete source `{url}`: {e}"))?;
    if changed == 0 { return Err(format!("vendorctl: source `{url}` not found for `{key}` {version} `{revision}`")); }
    Ok(())
}

pub(crate) fn add_pin(conn: &Connection, repo_id: &str, key: &str, version: &str, revision: &str) -> Result<(), String> {
    let pv_id = resolve_package_version_id(conn, key, version, revision)?;
    conn.execute(
        "INSERT INTO release_membership(repo_release_id, package_version_id) VALUES(?1, ?2)",
        params![repo_id, pv_id],
    ).map_err(|e| format!("vendorctl: pin `{key}` {version} `{revision}` into `{repo_id}`: {e}"))?;
    Ok(())
}

pub(crate) fn delete_pin(conn: &Connection, repo_id: &str, key: &str, version: &str, revision: &str) -> Result<(), String> {
    let pv_id = resolve_package_version_id(conn, key, version, revision)?;
    let changed = conn.execute(
        "DELETE FROM release_membership WHERE repo_release_id = ?1 AND package_version_id = ?2",
        params![repo_id, pv_id],
    ).map_err(|e| format!("vendorctl: delete pin `{repo_id}` `{key}` {version} `{revision}`: {e}"))?;
    if changed == 0 { return Err(format!("vendorctl: pin not found for `{repo_id}` `{key}` {version} `{revision}`")); }
    Ok(())
}

pub(crate) fn export_catalog(conn: &Connection) -> Result<CatalogExport, String> {
    Ok(CatalogExport {
        repo_releases: query_repo_releases(conn)?,
        packages: query_packages(conn)?,
        package_versions: query_package_versions(conn)?,
        sources: query_sources(conn)?,
        release_membership: query_release_membership(conn)?,
    })
}

pub(crate) fn import_catalog(conn: &mut Connection, cat: &CatalogExport) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| format!("vendorctl: import begin: {e}"))?;
    tx.execute_batch(
        "DELETE FROM release_membership;
         DELETE FROM sources;
         DELETE FROM package_versions;
         DELETE FROM packages;
         DELETE FROM repo_releases;",
    ).map_err(|e| format!("vendorctl: import reset: {e}"))?;
    for row in &cat.repo_releases {
        tx.execute(
            "INSERT INTO repo_releases(id, status, description, created_at, frozen_at) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![row.id, row.status, row.description, row.created_at, row.frozen_at],
        ).map_err(|e| format!("vendorctl: import repo `{}`: {e}", row.id))?;
    }
    for row in &cat.packages {
        tx.execute(
            "INSERT INTO packages(key, display_name, category, owner, enabled) VALUES(?1, ?2, ?3, ?4, ?5)",
            params![row.key, row.display_name, row.category, row.owner, if row.enabled { 1 } else { 0 }],
        ).map_err(|e| format!("vendorctl: import package `{}`: {e}", row.key))?;
    }
    for row in &cat.package_versions {
        tx.execute(
            "INSERT INTO package_versions(package_key, version, revision, source_type, integrity_hash, patchset_id, build_recipe_id, status)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![row.package_key, row.version, row.revision, row.source_type, row.integrity_hash, row.patchset_id, row.build_recipe_id, row.status],
        ).map_err(|e| format!("vendorctl: import version `{}` {} `{}`: {e}", row.package_key, row.version, row.revision))?;
    }
    for row in &cat.sources {
        let pv_id = tx.query_row(
            "SELECT id FROM package_versions WHERE package_key = ?1 AND version = ?2 AND revision = ?3",
            params![row.package_key, row.version, row.revision],
            |r| r.get::<_, i64>(0),
        ).map_err(|e| format!("vendorctl: import source resolve `{}` {} `{}`: {e}", row.package_key, row.version, row.revision))?;
        tx.execute(
            "INSERT INTO sources(package_version_id, canonical_url, mirror_url, filename, checksum_type, checksum_value, size_bytes, signature_url)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![pv_id, row.canonical_url, row.mirror_url, row.filename, row.checksum_type, row.checksum_value, row.size_bytes, row.signature_url],
        ).map_err(|e| format!("vendorctl: import source `{}`: {e}", row.canonical_url))?;
    }
    for row in &cat.release_membership {
        let pv_id = tx.query_row(
            "SELECT id FROM package_versions WHERE package_key = ?1 AND version = ?2 AND revision = ?3",
            params![row.package_key, row.version, row.revision],
            |r| r.get::<_, i64>(0),
        ).map_err(|e| format!("vendorctl: import pin resolve `{}` {} `{}`: {e}", row.package_key, row.version, row.revision))?;
        tx.execute(
            "INSERT INTO release_membership(repo_release_id, package_version_id) VALUES(?1, ?2)",
            params![row.repo_release_id, pv_id],
        ).map_err(|e| format!("vendorctl: import pin `{}`: {e}", row.repo_release_id))?;
    }
    tx.commit().map_err(|e| format!("vendorctl: import commit: {e}"))?;
    Ok(())
}

fn query_repo_releases(conn: &Connection) -> Result<Vec<RepoRelease>, String> {
    let mut st = conn.prepare("SELECT id, status, description, created_at, frozen_at FROM repo_releases ORDER BY id")
        .map_err(|e| format!("vendorctl: query repos: {e}"))?;
    let rows = st.query_map([], |r| Ok(RepoRelease {
        id: r.get(0)?,
        status: r.get(1)?,
        description: r.get(2)?,
        created_at: r.get(3)?,
        frozen_at: r.get(4)?,
    })).map_err(|e| format!("vendorctl: map repos: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("vendorctl: collect repos: {e}"))
}

fn query_packages(conn: &Connection) -> Result<Vec<Package>, String> {
    let mut st = conn.prepare("SELECT key, display_name, category, owner, enabled FROM packages ORDER BY key")
        .map_err(|e| format!("vendorctl: query packages: {e}"))?;
    let rows = st.query_map([], |r| Ok(Package {
        key: r.get(0)?,
        display_name: r.get(1)?,
        category: r.get(2)?,
        owner: r.get(3)?,
        enabled: r.get::<_, i64>(4)? != 0,
    })).map_err(|e| format!("vendorctl: map packages: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("vendorctl: collect packages: {e}"))
}

fn query_package_versions(conn: &Connection) -> Result<Vec<PackageVersion>, String> {
    let mut st = conn.prepare(
        "SELECT package_key, version, revision, source_type, integrity_hash, patchset_id, build_recipe_id, status
           FROM package_versions
       ORDER BY package_key, version, revision"
    ).map_err(|e| format!("vendorctl: query versions: {e}"))?;
    let rows = st.query_map([], |r| Ok(PackageVersion {
        package_key: r.get(0)?,
        version: r.get(1)?,
        revision: r.get(2)?,
        source_type: r.get(3)?,
        integrity_hash: r.get(4)?,
        patchset_id: r.get(5)?,
        build_recipe_id: r.get(6)?,
        status: r.get(7)?,
    })).map_err(|e| format!("vendorctl: map versions: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("vendorctl: collect versions: {e}"))
}

fn query_sources(conn: &Connection) -> Result<Vec<Source>, String> {
    let mut st = conn.prepare(
        "SELECT pv.package_key, pv.version, pv.revision,
                s.canonical_url, s.mirror_url, s.filename, s.checksum_type, s.checksum_value, s.size_bytes, s.signature_url
           FROM sources s
           JOIN package_versions pv ON pv.id = s.package_version_id
       ORDER BY pv.package_key, pv.version, pv.revision, s.canonical_url"
    ).map_err(|e| format!("vendorctl: query sources: {e}"))?;
    let rows = st.query_map([], |r| Ok(Source {
        package_key: r.get(0)?,
        version: r.get(1)?,
        revision: r.get(2)?,
        canonical_url: r.get(3)?,
        mirror_url: r.get(4)?,
        filename: r.get(5)?,
        checksum_type: r.get(6)?,
        checksum_value: r.get(7)?,
        size_bytes: r.get(8)?,
        signature_url: r.get(9)?,
    })).map_err(|e| format!("vendorctl: map sources: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("vendorctl: collect sources: {e}"))
}

fn query_release_membership(conn: &Connection) -> Result<Vec<ReleaseMembership>, String> {
    let mut st = conn.prepare(
        "SELECT rm.repo_release_id, pv.package_key, pv.version, pv.revision
           FROM release_membership rm
           JOIN package_versions pv ON pv.id = rm.package_version_id
       ORDER BY rm.repo_release_id, pv.package_key, pv.version, pv.revision"
    ).map_err(|e| format!("vendorctl: query pins: {e}"))?;
    let rows = st.query_map([], |r| Ok(ReleaseMembership {
        repo_release_id: r.get(0)?,
        package_key: r.get(1)?,
        version: r.get(2)?,
        revision: r.get(3)?,
    })).map_err(|e| format!("vendorctl: map pins: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| format!("vendorctl: collect pins: {e}"))
}
