mod copy;
pub use self::copy::copy;

pub fn copy_twice() -> u32 {
    copy() + self::copy::copy()
}
