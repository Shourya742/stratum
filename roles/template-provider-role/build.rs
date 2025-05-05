use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-changed=capnp");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("Missing OUT_DIR"));
    let capnp_dir = Path::new("capnp");

    let schemas = [
        "common.capnp",
        "echo.capnp",
        "init.capnp",
        "mining.capnp",
        "proxy.capnp",
    ];
    let mut cmd = capnpc::CompilerCommand::new();
    cmd.src_prefix(capnp_dir).output_path(&out_dir);
    for schema in &schemas {
        cmd.file(capnp_dir.join(schema));
    }
    cmd.run().expect("capnpc compilation failed");

    // Too much hassle look into it later.
    // let re = Regex::new(r"crate::(\w+_capnp)").unwrap();

    // for schema in &schemas {
    //     let module_name = schema.strip_suffix(".capnp").unwrap();
    //     let file_name = format!("{}_capnp.rs", module_name);
    //     let generated_file = out_dir.join(&file_name);

    //     let content = fs::read_to_string(&generated_file)
    //         .unwrap_or_else(|e| panic!("Failed to read {}: {}", generated_file.display(), e));

    //     // Patch all references like `crate::proxy_capnp::...` → `crate::capnp::proxy_capnp::...`
    //     let patched = re.replace_all(&content, "crate::capnp::$1");

    //     fs::write(&generated_file, patched.as_ref())
    //         .unwrap_or_else(|e| panic!("Failed to write {}: {}", generated_file.display(), e));
    // }
}
