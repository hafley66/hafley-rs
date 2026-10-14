use signal_domain::_0_source::{Level, Signal};

pub fn direct() -> u32 { Signal::Ready(Level(3)).value() }
pub fn reexport(signal: signal_domain::Signal) -> u32 { signal.value() }
