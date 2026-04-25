use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    const JS_FILES: &[&str] = &[
        "js/core.js",
        "js/stealth.js",
        "js/timers.js",
        "js/dom.js",
        "js/navigator.js",
        "js/fetch.js",
        "js/webapis.js",
        "js/workers.js",
        "js/polyfills.js",
        "js/init.js",
    ];

    for f in JS_FILES {
        println!("cargo:rerun-if-changed={f}");
    }

    let bootstrap_js: String = JS_FILES
        .iter()
        .map(|f| std::fs::read_to_string(f).unwrap_or_else(|e| panic!("Failed to read {f}: {e}")))
        .collect::<Vec<_>>()
        .join("");

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let snapshot_path = out_dir.join("OBSCURA_SNAPSHOT.bin");

    let output = deno_core::snapshot::create_snapshot(
        deno_core::snapshot::CreateSnapshotOptions {
            cargo_manifest_dir: env!("CARGO_MANIFEST_DIR"),
            startup_snapshot: None,
            skip_op_registration: true,
            extensions: vec![],
            extension_transpiler: None,
            with_runtime_cb: Some(Box::new(move |runtime| {
                runtime
                    .execute_script("<obscura:bootstrap>", bootstrap_js.to_string())
                    .expect("bootstrap.js should not fail during snapshot creation");
            })),
        },
        None,
    )
    .expect("Failed to create V8 snapshot");

    std::fs::write(&snapshot_path, &*output.output).expect("Failed to write snapshot");
    println!("cargo:rustc-env=OBSCURA_SNAPSHOT_PATH={}", snapshot_path.display());

    for file in &output.files_loaded_during_snapshot {
        println!("cargo:rerun-if-changed={}", file.display());
    }
}
