fn main() {
    println!("cargo::rerun-if-changed=../third_party/qoi/qoi.h");
    println!("cargo::rerun-if-changed=../third_party/qoi/stb_image.h");
    println!("cargo::rerun-if-changed=../third_party/qoi/stb_image_write.h");
    println!("cargo::rerun-if-changed=../third_party/qoi/qoibench.c");

    println!("cargo::rerun-if-changed=../third_party/wuffs/wrapper.c");
    println!("cargo::rerun-if-changed=../third_party/wuffs/wuffs-v0.4.c");

    cc::Build::new()
        .cargo_warnings(false)
        .file("../third_party/qoi/qoibench.c")
        .compile("qoibench");

    cc::Build::new()
        .file("../third_party/wuffs/wrapper.c")
        .compile("wuffs");

    println!("cargo::rustc-link-lib=static=png");
}
