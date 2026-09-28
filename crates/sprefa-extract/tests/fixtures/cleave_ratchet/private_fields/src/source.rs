pub(crate) struct Packet { value: u8 }

pub(crate) fn read() -> u8 {
    Packet { value: 7 }.value
}
