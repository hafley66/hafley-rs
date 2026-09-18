use serde::Serialize;

pub struct Helper {
    pub size: u32,
}

#[derive(Serialize)]
pub struct S {
    #[serde(rename = "size")]
    pub len: u32,
}
