use crate::{Pattern, RepositoryId};

pub fn imported(value: Pattern) -> crate::Pattern {
    value
}

pub fn glob_imported(value: RepositoryId) -> crate::RepositoryId {
    value
}
