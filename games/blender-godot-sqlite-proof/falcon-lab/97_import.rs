fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args_os().nth(1) {
        Some(directory) => falcon_gdext::inspect_catalog(std::path::Path::new(&directory)),
        None => falcon_gdext::inspect_import(),
    }
}
