#[derive(Default)]
pub(crate) struct Packet { value: u8 }

pub fn packet() -> Packet {
    Packet::default()
}

pub mod child {
    pub fn make() {
        let _ = super::Packet { value: 7 };
    }
}
