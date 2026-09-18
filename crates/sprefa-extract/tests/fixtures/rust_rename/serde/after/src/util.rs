use serde::Serialize;

pub struct Helper {
    pub width: u32,
}

#[derive(Serialize)]
pub struct S {
    #[serde(rename = "size")]
    pub len: u32,
}
