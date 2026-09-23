/// Every match of every file in one run. Rows index into `spans` by range; no lifetimes.
#[derive(Default)]
pub struct MatchArena {
    pub files: Vec<Box<str>>,
    pub spans: Vec<super::CapturedSpan>,
    pub rows: Vec<super::MatchRow>,
    pub emitted: Vec<super::EmittedFact>,
    pub emitted_fields: Vec<super::EmittedField>,
}
