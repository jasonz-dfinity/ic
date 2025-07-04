use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

// Build reproducibility. askama adds a include_bytes! call when it's generating
// a template impl so that rustc will recompile the module when the file changes
// on disk. See https://github.com/djc/askama/blob/180696053833147a61b3348646a953e7d92ae582/askama_shared/src/generator.rs#L141
// The stringified output of every proc-macro is added to the metadata hash for
// a crate. That output includes the full filepath to include_bytes!. It may be
// different on two machines, if they use different tempdir paths for the build.
// However, if we include the html source directly in the output, no
// inconsistency is introduced.
fn main() {
    println!("cargo:rerun-if-changed=templates/dashboard.html");
    let mut f = File::create(PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("dashboard.rs"))
        .unwrap();
    f.write_all(
        format!(
            r#"
#[derive(Template)]
#[template(escape = "html", source = {:?}, ext = "html")]
struct Dashboard<'a> {{
    height: Height,
    canisters: &'a Vec<(&'a ic_replicated_state::CanisterState, SubnetId)>,
}}
    "#,
            std::fs::read_to_string("templates/dashboard.html").unwrap()
        )
        .as_bytes(),
    )
    .unwrap();

    // The environment variable `REGISTRY_CANISTER_WASM_PATH` pointing to a file (storing the registry canister) is needed
    // for the PocketIC server to compile. There are two flows to support:
    // - code validation using `cargo`: we create a dummy file and point `REGISTRY_CANISTER_WASM_PATH` to that file for code validation to succeed;
    // - building the PocketIC server using `bazel`: `bazel` always sets `REGISTRY_CANISTER_WASM_PATH` to an actual file storing the registry canister
    //   (built separately) and thus we don't override `REGISTRY_CANISTER_WASM_PATH` if already set.
    if std::env::var("REGISTRY_CANISTER_WASM_PATH").is_err() {
        let registry_canister_wasm_path =
            PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("registry.wasm.gz");
        File::create(&registry_canister_wasm_path).unwrap();
        println!(
            "cargo:rustc-env=REGISTRY_CANISTER_WASM_PATH={}",
            registry_canister_wasm_path.display()
        );
    }
}
