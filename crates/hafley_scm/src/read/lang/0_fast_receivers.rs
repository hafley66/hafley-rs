//! Request-scoped fast receiver policy, copied into the project's provider.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FastReceivers {
    #[default]
    Abstain,
    Written,
}

impl std::str::FromStr for FastReceivers {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "written" => Ok(Self::Written),
            _ => Err(format!("fast-receivers expects written, got {value:?}")),
        }
    }
}

thread_local! {
    static REQUEST: Cell<Option<FastReceivers>> = const { Cell::new(None) };
}

impl FastReceivers {
    pub fn current() -> Self {
        REQUEST.with(Cell::get).unwrap_or_else(|| {
            std::env::var("RYI_FAST_RECEIVERS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or_default()
        })
    }

    pub fn scoped_override(value: Option<&str>) -> Result<ScopedFastReceivers, String> {
        let value = value.map(str::parse).transpose()?;
        Ok(ScopedFastReceivers(REQUEST.with(|slot| {
            let previous = slot.get();
            if let Some(value) = value {
                slot.set(Some(value));
            }
            previous
        })))
    }
}

pub struct ScopedFastReceivers(Option<FastReceivers>);
impl Drop for ScopedFastReceivers {
    fn drop(&mut self) {
        REQUEST.with(|slot| slot.set(self.0));
    }
}
