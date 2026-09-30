// Appended to minicore's core for the Fast tier: the Option, Result, slice, str and
// iterator-adapter signatures minicore leaves out. Not compiled into hafley_scm.
pub mod shim {
    use crate::iter::{IntoIterator, Iterator};
    use crate::option::Option;
    pub struct Same<I>(I);
    impl<I: Iterator> Iterator for Same<I> { type Item = I::Item; fn next(&mut self) -> Option<I::Item> { loop {} } }
    pub struct Enumerate<I>(I);
    impl<I: Iterator> Iterator for Enumerate<I> { type Item = (usize, I::Item); fn next(&mut self) -> Option<(usize, I::Item)> { loop {} } }
    pub struct Zip<A, B>(A, B);
    impl<A: Iterator, B: Iterator> Iterator for Zip<A, B> { type Item = (A::Item, B::Item); fn next(&mut self) -> Option<(A::Item, B::Item)> { loop {} } }
    pub struct Chain<A, B>(A, B);
    impl<A: Iterator, B: Iterator<Item = A::Item>> Iterator for Chain<A, B> { type Item = A::Item; fn next(&mut self) -> Option<A::Item> { loop {} } }
    pub struct Peekable<I: Iterator>(I, Option<I::Item>);
    impl<I: Iterator> Iterator for Peekable<I> { type Item = I::Item; fn next(&mut self) -> Option<I::Item> { loop {} } }
    impl<I: Iterator> Peekable<I> { pub fn peek(&mut self) -> Option<&I::Item> { loop {} } }
    pub struct Cloned<I>(I);
    impl<'a, T: 'a + Clone, I: Iterator<Item = &'a T>> Iterator for Cloned<I> { type Item = T; fn next(&mut self) -> Option<T> { loop {} } }
    pub struct FlatMap<I, U, F>(I, U, F);
    impl<I: Iterator, U: IntoIterator, F: FnMut(I::Item) -> U> Iterator for FlatMap<I, U, F> { type Item = U::Item; fn next(&mut self) -> Option<U::Item> { loop {} } }
    pub struct Flatten<I>(I);
    impl<I: Iterator> Iterator for Flatten<I> where I::Item: IntoIterator { type Item = <I::Item as IntoIterator>::Item; fn next(&mut self) -> Option<Self::Item> { loop {} } }
    pub struct SliceIter<'a, T>(&'a [T]);
    impl<'a, T> Iterator for SliceIter<'a, T> { type Item = &'a T; fn next(&mut self) -> Option<&'a T> { loop {} } }
    pub struct SliceIterMut<'a, T>(&'a mut [T]);
    impl<'a, T> Iterator for SliceIterMut<'a, T> { type Item = &'a mut T; fn next(&mut self) -> Option<&'a mut T> { loop {} } }
    pub struct StrIter<'a, T>(&'a str, T);
    impl<'a, T> Iterator for StrIter<'a, T> { type Item = T; fn next(&mut self) -> Option<T> { loop {} } }
}

impl<T> option::Option<T> {
    pub fn expect(self, message: &str) -> T { loop {} }
    pub fn is_some(&self) -> bool { loop {} }
    pub fn is_none(&self) -> bool { loop {} }
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Option<U> { loop {} }
    pub fn as_mut(&mut self) -> Option<&mut T> { loop {} }
    pub fn take(&mut self) -> Option<T> { loop {} }
    pub fn ok_or_else<E, F: FnOnce() -> E>(self, err: F) -> result::Result<T, E> { loop {} }
    pub fn filter<P: FnOnce(&T) -> bool>(self, predicate: P) -> Option<T> { loop {} }
    pub fn unwrap_or_default(self) -> T where T: default::Default { loop {} }
    pub fn get_or_insert_with<F: FnOnce() -> T>(&mut self, f: F) -> &mut T { loop {} }
    pub fn is_some_and(self, f: impl FnOnce(T) -> bool) -> bool { loop {} }
    pub fn iter(&self) -> shim::SliceIter<'_, T> { loop {} }
}
impl<T: Clone> option::Option<&T> {
    pub fn cloned(self) -> Option<T> { loop {} }
}
impl<T, E> result::Result<T, E> {
    pub fn unwrap(self) -> T { loop {} }
    pub fn expect(self, message: &str) -> T { loop {} }
    pub fn unwrap_err(self) -> E { loop {} }
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> result::Result<U, E> { loop {} }
    pub fn map_err<D, F: FnOnce(E) -> D>(self, f: F) -> result::Result<T, D> { loop {} }
    pub fn and_then<U, F: FnOnce(T) -> result::Result<U, E>>(self, f: F) -> result::Result<U, E> { loop {} }
    pub fn ok(self) -> Option<T> { loop {} }
    pub fn err(self) -> Option<E> { loop {} }
    pub fn is_ok(&self) -> bool { loop {} }
    pub fn is_err(&self) -> bool { loop {} }
    pub fn as_ref(&self) -> result::Result<&T, &E> { loop {} }
    pub fn unwrap_or_default(self) -> T where T: default::Default { loop {} }
}
impl<T> [T] {
    pub fn iter(&self) -> shim::SliceIter<'_, T> { loop {} }
    pub fn iter_mut(&mut self) -> shim::SliceIterMut<'_, T> { loop {} }
    pub fn is_empty(&self) -> bool { loop {} }
    pub fn first(&self) -> Option<&T> { loop {} }
    pub fn last(&self) -> Option<&T> { loop {} }
    pub fn get(&self, index: usize) -> Option<&T> { loop {} }
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> { loop {} }
    pub fn contains(&self, value: &T) -> bool { loop {} }
}
impl str {
    pub fn is_empty(&self) -> bool { loop {} }
    pub fn trim(&self) -> &str { loop {} }
    pub fn trim_start(&self) -> &str { loop {} }
    pub fn trim_end(&self) -> &str { loop {} }
    pub fn starts_with(&self, pattern: &str) -> bool { loop {} }
    pub fn ends_with(&self, pattern: &str) -> bool { loop {} }
    pub fn contains(&self, pattern: &str) -> bool { loop {} }
    pub fn find(&self, pattern: &str) -> Option<usize> { loop {} }
    pub fn strip_prefix<'a>(&'a self, prefix: &str) -> Option<&'a str> { loop {} }
    pub fn strip_suffix<'a>(&'a self, suffix: &str) -> Option<&'a str> { loop {} }
    pub fn split_once<'a>(&'a self, delimiter: &str) -> Option<(&'a str, &'a str)> { loop {} }
    pub fn split<'a>(&'a self, pattern: &str) -> shim::StrIter<'a, &'a str> { loop {} }
    pub fn lines(&self) -> shim::StrIter<'_, &str> { loop {} }
    pub fn chars(&self) -> shim::StrIter<'_, char> { loop {} }
    pub fn bytes(&self) -> shim::StrIter<'_, u8> { loop {} }
    pub fn as_bytes(&self) -> &[u8] { loop {} }
}
