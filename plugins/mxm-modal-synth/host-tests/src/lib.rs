//! mxm-modal-synth hosted by MXM Player. The tests are in `tests/` and load the plugin's release
//! bundle from `target/bundled`, skipping with the command to build it when it is missing:
//! `cargo xtask bundle mxm-modal-synth --release`, then `cargo test -p mxm-modal-synth-host-tests`.

use std::path::PathBuf;

/// The plugin's CLAP id.
pub const PLUGIN: &str = "dk.mxm.mxm-modal-synth";

/// The release bundle, or `None` (with the command that makes it) when it has not been built.
pub fn bundle() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|dir| dir.join("Cargo.lock").is_file())
        .map(PathBuf::from)?;
    let path = root.join("target/bundled/mxm-modal-synth.clap");
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
