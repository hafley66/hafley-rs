//! Fingerprint both source workspaces, including uncommitted changes. Peer handshakes
//! use this stamp to reject incompatible simulation builds before starting GGRS.
//! Absolute checkout paths and Git metadata are deliberately excluded.
use std::{env, fs, path::Path};

fn fingerprint(root: &Path, path: &Path, hash: &mut blake3::Hasher) {
    println!("cargo:rerun-if-changed={}", path.display());
    if path.is_dir() {
        let mut children: Vec<_> = fs::read_dir(path)
            .expect("read fingerprint directory")
            .map(|entry| entry.expect("read fingerprint entry").path())
            .filter(|path| {
                !matches!(
                    path.file_name().and_then(|s| s.to_str()),
                    Some("target" | ".git" | "node_modules")
                )
            })
            .collect();
        children.sort();
        for child in children {
            fingerprint(root, &child, hash);
        }
    } else if path.exists() {
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = fs::read(path).expect("read fingerprint input");
        hash.update(&(relative.len() as u64).to_le_bytes());
        hash.update(relative.as_bytes());
        hash.update(&(bytes.len() as u64).to_le_bytes());
        hash.update(&bytes);
    }
}

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let shell = Path::new(&manifest).join("../..").canonicalize().unwrap();
    let shared = Path::new(&manifest)
        .join("../../../../..")
        .canonicalize()
        .unwrap();
    let mut hash = blake3::Hasher::new();
    // Conservative source stamp: unrelated crate source changes also require matching
    // clients. Compiler flags/target differences are not certified here.
    for (label, root, inputs) in [
        (
            "shell",
            shell,
            &[
                "Cargo.toml",
                "Cargo.lock",
                "rust-toolchain.toml",
                ".cargo",
                "crates",
                "vendor",
                "content",
            ][..],
        ),
        (
            "shared",
            shared,
            &[
                "Cargo.toml",
                "Cargo.lock",
                "crates/redux",
                "crates/input",
                "crates/rollback",
                "crates/egui-rsx-macro",
                "games/kneeman/Cargo.toml",
                "games/kneeman/src",
                "games/cascade",
            ][..],
        ),
    ] {
        hash.update(label.as_bytes());
        for input in inputs {
            fingerprint(&root, &root.join(input), &mut hash);
        }
    }
    println!(
        "cargo:rustc-env=BUILD_HASH=src-{}",
        &hash.finalize().to_hex()[..16]
    );
}
