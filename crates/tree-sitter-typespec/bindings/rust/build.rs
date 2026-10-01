fn main() {
    let src_dir = std::path::Path::new("src");
    let parser_path = src_dir.join("parser.c");
    let mut c_config = cc::Build::new();
    c_config.std("c11").include(src_dir).file(&parser_path);
    #[cfg(target_env = "msvc")]
    c_config.flag("-utf-8");
    println!("cargo:rerun-if-changed={}", parser_path.to_str().unwrap());
    c_config.compile("tree-sitter-typespec");
}
