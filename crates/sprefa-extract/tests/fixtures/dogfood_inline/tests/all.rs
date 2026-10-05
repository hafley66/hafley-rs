#[path = ""]
mod nested {
    #[path = "0_capture.rs"]
    pub mod capture;
}
fn caller() { crate::nested::capture::capture(); }
