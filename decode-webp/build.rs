fn main() {
    println!("cargo::rerun-if-changed=../third_party/wuffs/wrapper.c");
    println!("cargo::rerun-if-changed=../third_party/wuffs/wuffs-v0.4.c");

    cc::Build::new()
        .file("../third_party/wuffs/wrapper.c")
        .compile("wuffs");
}
