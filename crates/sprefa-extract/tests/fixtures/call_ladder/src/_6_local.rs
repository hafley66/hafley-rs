pub struct Probe;

impl Probe {
    pub fn rows(&self) -> usize { 6 }
}

pub fn local_probe_six() -> usize {
    Probe.rows()
}
