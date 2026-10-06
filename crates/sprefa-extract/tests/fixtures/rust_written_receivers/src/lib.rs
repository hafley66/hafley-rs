pub trait Read { fn read(&self) -> u32; }
pub struct Record;
impl Read for Record { fn read(&self) -> u32 { 1 } }
impl Read for &Record { fn read(&self) -> u32 { 2 } }
pub fn owned(value: Record) -> u32 { value.read() }
pub fn borrowed(value: &Record) -> u32 { value.read() }
pub struct Decoy;
impl Decoy { pub fn read(&self) -> u32 { 3 } }
pub fn pattern(value: Option<Record>) -> u32 {
    let out: Decoy = match value {
        Some(untyped) => { untyped.read(); Decoy },
        None => Decoy,
    };
    out.read()
}

pub fn reference_expr(value: Record) -> u32 { (&value).read() }

pub struct RefOnly;
impl Read for &RefOnly { fn read(&self) -> u32 { 4 } }
pub fn ref_only(value: &RefOnly) -> u32 { value.read() }
