fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args_os().nth(1) {
        Some(directory) => pigeon_gdext::inspect_catalog(std::path::Path::new(&directory)),
        None => pigeon_gdext::inspect_import(),
    }
}
