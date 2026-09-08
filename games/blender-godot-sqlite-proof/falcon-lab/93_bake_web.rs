fn main() -> Result<(), Box<dyn std::error::Error>> {
    falcon_gdext::bake_web(std::env::args().nth(1).ok_or("output path required")?.as_ref())
}
