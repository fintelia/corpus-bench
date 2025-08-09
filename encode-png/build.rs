fn main() {
    println!("cargo::rerun-if-changed=../third_party/qoi/qoi.h");
    println!("cargo::rerun-if-changed=../third_party/qoi/stb_image.h");
    println!("cargo::rerun-if-changed=../third_party/qoi/stb_image_write.h");
    println!("cargo::rerun-if-changed=../third_party/qoi/qoibench.c");

    cc::Build::new()
        .cargo_warnings(false)
        .file("../third_party/qoi/qoibench.c")
        .compile("qoibench");

    println!("cargo::rustc-link-lib=static=png");
}
