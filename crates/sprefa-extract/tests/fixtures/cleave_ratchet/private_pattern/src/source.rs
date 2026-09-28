#[derive(Default)]
pub(crate) struct Packet { value: u8 }

pub mod child {
    pub fn read() -> u8 {
        let super::Packet { value } = super::packet();
        value
    }
}

pub fn packet() -> Packet {
    Packet::default()
}
