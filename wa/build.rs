use std::env;

fn main() {
    let target = env::var("TARGET").unwrap_or_else(|e| panic!("{}", e));

    if target.contains("darwin") {
        println!("cargo:rustc-link-lib=framework=MetalKit");
    }
}
