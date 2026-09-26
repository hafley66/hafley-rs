pub struct SelfType;

impl SelfType {
    pub fn wrap(value: Self) -> Self {
        value
    }

    pub fn annotate() {
        let _: Self = SelfType;
    }
}
