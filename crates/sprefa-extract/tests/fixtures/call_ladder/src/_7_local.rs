pub struct Probe;

impl Probe {
    pub fn rows(&self) -> usize { 7 }
}

pub fn local_probe_seven() -> usize {
    Probe.rows()
}
