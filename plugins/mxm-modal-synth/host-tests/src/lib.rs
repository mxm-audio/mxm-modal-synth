//! mxm-modal-synth hosted by MXM Player: every model, the voices, robustness, and the parameters
//! through CLAP state (the freeze's checks). The tests are in `tests/` and load the plugin's release
//! bundle from `target/bundled`, skipping with the command to build it when it is missing:
//! `cargo xtask bundle mxm-modal-synth --release`, then `cargo test -p mxm-modal-synth-host-tests`.

use std::path::PathBuf;

/// The plugin's CLAP id.
pub const PLUGIN: &str = "dk.mxm.mxm-modal-synth";

/// The repository's root, where `Cargo.lock` is.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .map(PathBuf::from)
        .expect("the repository's root")
}

/// The release bundle, or `None` (with the command that makes it) when it has not been built.
pub fn bundle() -> Option<PathBuf> {
    let path = root().join("target/bundled/mxm-modal-synth.clap");
    if path.exists() {
        Some(path)
    } else {
        eprintln!(
            "skipping: {} is missing — run `cargo xtask bundle mxm-modal-synth --release`",
            path.display()
        );
        None
    }
}

/// The CLAP id nice-plug gives a parameter's string id (its `hash_param_id`): a Rabin
/// fingerprint, the top bit cleared for VST3.
pub fn param_id(id: &str) -> u32 {
    let mut hash: u32 = 0;
    for byte in id.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(u32::from(byte));
    }
    hash & !(1 << 31)
}

/// An empty folder for a player session, in the build's own folder (never the system's temporary
/// folder: nothing works on C:).
pub fn scratch(name: &str) -> PathBuf {
    let dir = root().join("target/host-tests").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Every frozen parameter's id, in the order of the plugin's `frozen-parameters.txt`.
pub fn frozen_ids() -> Vec<String> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../frozen-parameters.txt");
    std::fs::read_to_string(path)
        .expect("frozen-parameters.txt")
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_whitespace().next().map(str::to_owned))
        .collect()
}
