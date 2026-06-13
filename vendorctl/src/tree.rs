// Canonical rpmbuild %_topdir tree resolution. vendorctl lives at
// <topdir>/vendorctl/, so the topdir is its manifest dir's parent.
use std::path::PathBuf;

pub(crate) fn topdir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn specs() -> PathBuf  { topdir().join("SPECS") }
pub(crate) fn sources() -> PathBuf { topdir().join("SOURCES") }
pub(crate) fn rpms() -> PathBuf    { topdir().join("RPMS") }
// Per-arch staging sysroot — built lib RPMs install here; dependents build against it
// (the mock-chroot analog for cross builds). e.g. <topdir>/sysroot/x86_64/usr/{include,lib}.
pub(crate) fn sysroot(arch: &str) -> PathBuf { topdir().join("sysroot").join(arch) }

// Upstream vendor source tree (the 14 GB cross-build dir in oxide2).
// Override with OXIDE_VENDOR; default is the sibling oxide2 checkout.
pub(crate) fn vendor_root() -> PathBuf {
    std::env::var("OXIDE_VENDOR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/home/nd/oxide/oxide2/vendor"))
}

pub(crate) const ARCHES: &[&str] = &["x86_64", "aarch64"];
