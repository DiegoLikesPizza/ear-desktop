use std::{env, fs, path::Path};

fn main() {
    // Embed the product renders as a static table so images load from memory, lazily.
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("device_images.rs");
    let dir = Path::new("assets/devices");
    println!("cargo:rerun-if-changed=assets/devices");
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".webp"))
        .collect();
    names.sort();
    let mut src = String::from("pub static DEVICE_IMAGES: &[(&str, &[u8])] = &[\n");
    for n in names {
        let stem = n.trim_end_matches(".webp");
        let abs = fs::canonicalize(dir.join(&n)).unwrap();
        src.push_str(&format!("    ({stem:?}, include_bytes!({:?})),\n", abs.display().to_string()));
    }
    src.push_str("];\n");
    fs::write(out, src).unwrap();

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "ear (desktop)");
        res.set("FileDescription", "ear (desktop)");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed icon: {e}");
        }
    }
}
