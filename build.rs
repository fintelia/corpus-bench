fn main() {
    println!("cargo::rerun-if-changed=qoi/qoi.h");
    println!("cargo::rerun-if-changed=qoi/stb_image.h");
    println!("cargo::rerun-if-changed=qoi/stb_image_write.h");
    println!("cargo::rerun-if-changed=qoi/qoibench.c");

    println!("cargo::rerun-if-changed=wuffs/wrapper.c");
    println!("cargo::rerun-if-changed=wuffs/wuffs-v0.4.c");

    cc::Build::new()
        .cargo_warnings(false)
        .file("qoi/qoibench.c")
        .compile("qoibench");

    cc::Build::new()
        .file("wuffs/wrapper.c")
        .compile("wuffs");

    println!("cargo::rustc-link-lib=static=png");
}
