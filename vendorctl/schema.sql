PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS repo_releases (
    id          TEXT PRIMARY KEY,
    status      TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    frozen_at   INTEGER
);

CREATE TABLE IF NOT EXISTS packages (
    key          TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    category     TEXT NOT NULL,
    owner        TEXT NOT NULL DEFAULT '',
    enabled      INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS package_versions (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    package_key     TEXT NOT NULL,
    version         TEXT NOT NULL,
    revision        TEXT NOT NULL DEFAULT '',
    source_type     TEXT NOT NULL DEFAULT 'tarball',
    integrity_hash  TEXT NOT NULL DEFAULT '',
    patchset_id     TEXT NOT NULL DEFAULT '',
    build_recipe_id TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL DEFAULT 'draft',
    -- orchestration: spec generation + build
    build_system    TEXT NOT NULL DEFAULT 'autotools',  -- plain-make|autotools|cargo|go|meson
    summary         TEXT NOT NULL DEFAULT '',
    license         TEXT NOT NULL DEFAULT '',
    upstream_url    TEXT NOT NULL DEFAULT '',
    src_subdir      TEXT NOT NULL DEFAULT '',            -- vendor/<key>/<src_subdir> (defaults <key>-<version>)
    build_args      TEXT NOT NULL DEFAULT '',            -- family-specific: configure flags / cargo args / make snippet
    cflags          TEXT NOT NULL DEFAULT '',            -- extra CFLAGS injected by %build (e.g. -std=gnu89)
    config_cache    TEXT NOT NULL DEFAULT '',            -- autotools cross config.cache (ac_cv_* run-test answers)
    ldflags         TEXT NOT NULL DEFAULT '',            -- extra LDFLAGS (e.g. -L<dep>/lib for shared lib deps)
    install_cmd     TEXT NOT NULL DEFAULT '',            -- override %install build step (e.g. openssl 'make install_sw DESTDIR=...')
    build_requires  TEXT NOT NULL DEFAULT '',            -- space-sep lib pkg keys installed into the sysroot before build (Fedora BuildRequires)
    UNIQUE(package_key, version, revision),
    FOREIGN KEY(package_key) REFERENCES packages(key) ON DELETE CASCADE
);

-- %install / %files map: one row per installed path.
CREATE TABLE IF NOT EXISTS install_map (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    package_version_id INTEGER NOT NULL,
    src                TEXT NOT NULL DEFAULT '',   -- path in %builddir (bin/file) or '' for links
    dest               TEXT NOT NULL,              -- final path on target, e.g. /usr/bin/sed
    kind               TEXT NOT NULL DEFAULT 'bin',-- bin|file|symlink|hardlink|tree
    link_target        TEXT NOT NULL DEFAULT '',   -- for symlink/hardlink: the target dest links to
    mode               TEXT NOT NULL DEFAULT '0755',
    FOREIGN KEY(package_version_id) REFERENCES package_versions(id) ON DELETE CASCADE
);

-- build provenance: one row per (version, arch) build.
CREATE TABLE IF NOT EXISTS build_results (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    package_version_id INTEGER NOT NULL,
    arch               TEXT NOT NULL,
    rpm_path           TEXT NOT NULL DEFAULT '',
    status             TEXT NOT NULL DEFAULT 'ok', -- ok|fail
    built_at           INTEGER NOT NULL DEFAULT 0,
    UNIQUE(package_version_id, arch),
    FOREIGN KEY(package_version_id) REFERENCES package_versions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS sources (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    package_version_id INTEGER NOT NULL,
    canonical_url      TEXT NOT NULL,
    mirror_url         TEXT NOT NULL DEFAULT '',
    filename           TEXT NOT NULL DEFAULT '',
    checksum_type      TEXT NOT NULL DEFAULT 'sha256',
    checksum_value     TEXT NOT NULL DEFAULT '',
    size_bytes         INTEGER,
    signature_url      TEXT NOT NULL DEFAULT '',
    UNIQUE(package_version_id, canonical_url),
    FOREIGN KEY(package_version_id) REFERENCES package_versions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS release_membership (
    repo_release_id    TEXT NOT NULL,
    package_version_id INTEGER NOT NULL,
    PRIMARY KEY(repo_release_id, package_version_id),
    FOREIGN KEY(repo_release_id) REFERENCES repo_releases(id) ON DELETE CASCADE,
    FOREIGN KEY(package_version_id) REFERENCES package_versions(id) ON DELETE CASCADE
);
