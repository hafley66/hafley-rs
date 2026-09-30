// Signatures of the std items workspace code reaches through, for the Fast tier's
// rust-analyzer. Not compiled into hafley_scm: 7a writes it beside minicore's core.
#![no_std]

pub use core::{
    borrow, cell, clone, cmp, convert, default, fmt, future, hash, iter, marker, mem, num, ops,
    option, pin, ptr, result, slice, str, task,
};

pub mod panic {
    pub use core::panic::*;
    pub struct AssertUnwindSafe<T>(pub T);
    impl<T> core::ops::Deref for AssertUnwindSafe<T> { type Target = T; fn deref(&self) -> &T { loop {} } }
    pub fn catch_unwind<F: FnOnce() -> R, R>(f: F) -> Result<R, crate::boxed::Box<()>> { loop {} }
}

use core::ops::{Deref, DerefMut, Index, IndexMut};

pub mod boxed {
    use super::*;
    pub struct Box<T: ?Sized>(*mut T);
    impl<T> Box<T> {
        pub fn new(value: T) -> Box<T> { loop {} }
        pub fn pin(value: T) -> core::pin::Pin<Box<T>> { loop {} }
    }
    impl<T: ?Sized> Deref for Box<T> { type Target = T; fn deref(&self) -> &T { loop {} } }
    impl<T: ?Sized> DerefMut for Box<T> { fn deref_mut(&mut self) -> &mut T { loop {} } }
    impl<T: Clone> Clone for Box<T> { fn clone(&self) -> Self { loop {} } }
}

pub mod rc {
    use super::*;
    pub struct Rc<T: ?Sized>(*const T);
    impl<T> Rc<T> {
        pub fn new(value: T) -> Rc<T> { loop {} }
    }
    impl<T: ?Sized> Rc<T> {
        pub fn clone(this: &Self) -> Self { loop {} }
        pub fn ptr_eq(this: &Self, other: &Self) -> bool { loop {} }
        pub fn strong_count(this: &Self) -> usize { loop {} }
    }
    impl<T: ?Sized> Deref for Rc<T> { type Target = T; fn deref(&self) -> &T { loop {} } }
    impl<T: ?Sized> Clone for Rc<T> { fn clone(&self) -> Self { loop {} } }
}

pub mod sync {
    use super::*;
    pub struct Arc<T: ?Sized>(*const T);
    impl<T> Arc<T> {
        pub fn new(value: T) -> Arc<T> { loop {} }
    }
    impl<T: ?Sized> Arc<T> {
        pub fn clone(this: &Self) -> Self { loop {} }
        pub fn ptr_eq(this: &Self, other: &Self) -> bool { loop {} }
        pub fn strong_count(this: &Self) -> usize { loop {} }
    }
    impl<T: ?Sized> Deref for Arc<T> { type Target = T; fn deref(&self) -> &T { loop {} } }
    impl<T: ?Sized> Clone for Arc<T> { fn clone(&self) -> Self { loop {} } }

    pub struct PoisonError<G>(G);
    impl<G> PoisonError<G> {
        pub fn into_inner(self) -> G { loop {} }
    }
    pub type LockResult<G> = Result<G, PoisonError<G>>;

    pub struct Mutex<T: ?Sized>(T);
    pub struct MutexGuard<'a, T: ?Sized>(&'a mut T);
    impl<T> Mutex<T> {
        pub fn new(value: T) -> Mutex<T> { loop {} }
        pub fn into_inner(self) -> LockResult<T> { loop {} }
    }
    impl<T: ?Sized> Mutex<T> {
        pub fn lock(&self) -> LockResult<MutexGuard<'_, T>> { loop {} }
        pub fn get_mut(&mut self) -> LockResult<&mut T> { loop {} }
    }
    impl<T: ?Sized> Deref for MutexGuard<'_, T> { type Target = T; fn deref(&self) -> &T { loop {} } }
    impl<T: ?Sized> DerefMut for MutexGuard<'_, T> { fn deref_mut(&mut self) -> &mut T { loop {} } }

    pub struct RwLock<T: ?Sized>(T);
    pub struct RwLockReadGuard<'a, T: ?Sized>(&'a T);
    pub struct RwLockWriteGuard<'a, T: ?Sized>(&'a mut T);
    impl<T> RwLock<T> {
        pub fn new(value: T) -> RwLock<T> { loop {} }
    }
    impl<T: ?Sized> RwLock<T> {
        pub fn read(&self) -> LockResult<RwLockReadGuard<'_, T>> { loop {} }
        pub fn write(&self) -> LockResult<RwLockWriteGuard<'_, T>> { loop {} }
    }
    impl<T: ?Sized> Deref for RwLockReadGuard<'_, T> { type Target = T; fn deref(&self) -> &T { loop {} } }
    impl<T: ?Sized> Deref for RwLockWriteGuard<'_, T> { type Target = T; fn deref(&self) -> &T { loop {} } }
    impl<T: ?Sized> DerefMut for RwLockWriteGuard<'_, T> { fn deref_mut(&mut self) -> &mut T { loop {} } }

    pub struct OnceLock<T>(T);
    impl<T> OnceLock<T> {
        pub const fn new() -> OnceLock<T> { loop {} }
        pub fn get(&self) -> Option<&T> { loop {} }
        pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T { loop {} }
        pub fn set(&self, value: T) -> Result<(), T> { loop {} }
    }
    pub struct LazyLock<T, F = fn() -> T>(T, F);
    impl<T, F: FnOnce() -> T> LazyLock<T, F> {
        pub const fn new(f: F) -> LazyLock<T, F> { loop {} }
    }
    impl<T, F: FnOnce() -> T> Deref for LazyLock<T, F> { type Target = T; fn deref(&self) -> &T { loop {} } }
}

pub mod vec {
    use super::*;
    pub struct Vec<T>(*mut T);
    pub struct Iter<'a, T>(&'a T);
    pub struct IterMut<'a, T>(&'a mut T);
    pub struct IntoIter<T>(T);
    impl<'a, T> Iterator for Iter<'a, T> { type Item = &'a T; fn next(&mut self) -> Option<&'a T> { loop {} } }
    impl<'a, T> Iterator for IterMut<'a, T> { type Item = &'a mut T; fn next(&mut self) -> Option<&'a mut T> { loop {} } }
    impl<T> Iterator for IntoIter<T> { type Item = T; fn next(&mut self) -> Option<T> { loop {} } }
    impl<T> Vec<T> {
        pub const fn new() -> Vec<T> { loop {} }
        pub fn with_capacity(capacity: usize) -> Vec<T> { loop {} }
        pub fn from_array<const N: usize>(items: [T; N]) -> Vec<T> { loop {} }
        pub fn push(&mut self, value: T) { loop {} }
        pub fn pop(&mut self) -> Option<T> { loop {} }
        pub fn insert(&mut self, index: usize, value: T) { loop {} }
        pub fn remove(&mut self, index: usize) -> T { loop {} }
        pub fn clear(&mut self) { loop {} }
        pub fn len(&self) -> usize { loop {} }
        pub fn is_empty(&self) -> bool { loop {} }
        pub fn get(&self, index: usize) -> Option<&T> { loop {} }
        pub fn get_mut(&mut self, index: usize) -> Option<&mut T> { loop {} }
        pub fn first(&self) -> Option<&T> { loop {} }
        pub fn last(&self) -> Option<&T> { loop {} }
        pub fn iter(&self) -> Iter<'_, T> { loop {} }
        pub fn iter_mut(&mut self) -> IterMut<'_, T> { loop {} }
        pub fn drain<R>(&mut self, range: R) -> IntoIter<T> { loop {} }
        pub fn extend<I: IntoIterator<Item = T>>(&mut self, items: I) { loop {} }
        pub fn retain<F: FnMut(&T) -> bool>(&mut self, keep: F) { loop {} }
        pub fn sort(&mut self) { loop {} }
        pub fn sort_by_key<K, F: FnMut(&T) -> K>(&mut self, key: F) { loop {} }
        pub fn dedup(&mut self) { loop {} }
        pub fn truncate(&mut self, len: usize) { loop {} }
        pub fn contains(&self, value: &T) -> bool { loop {} }
        pub fn as_slice(&self) -> &[T] { loop {} }
    }
    impl<T> Deref for Vec<T> { type Target = [T]; fn deref(&self) -> &[T] { loop {} } }
    impl<T> DerefMut for Vec<T> { fn deref_mut(&mut self) -> &mut [T] { loop {} } }
    impl<T> Index<usize> for Vec<T> { type Output = T; fn index(&self, index: usize) -> &T { loop {} } }
    impl<T> IndexMut<usize> for Vec<T> { fn index_mut(&mut self, index: usize) -> &mut T { loop {} } }
    impl<T: Clone> Clone for Vec<T> { fn clone(&self) -> Self { loop {} } }
    impl<T> Default for Vec<T> { fn default() -> Self { loop {} } }
    impl<T> IntoIterator for Vec<T> { type Item = T; type IntoIter = IntoIter<T>; fn into_iter(self) -> IntoIter<T> { loop {} } }
    impl<'a, T> IntoIterator for &'a Vec<T> { type Item = &'a T; type IntoIter = Iter<'a, T>; fn into_iter(self) -> Iter<'a, T> { loop {} } }
    impl<'a, T> IntoIterator for &'a mut Vec<T> { type Item = &'a mut T; type IntoIter = IterMut<'a, T>; fn into_iter(self) -> IterMut<'a, T> { loop {} } }
    impl<T> FromIterator<T> for Vec<T> { fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self { loop {} } }
}

pub mod string {
    use super::*;
    pub struct String(vec::Vec<u8>);
    impl String {
        pub const fn new() -> String { loop {} }
        pub fn with_capacity(capacity: usize) -> String { loop {} }
        pub fn push(&mut self, ch: char) { loop {} }
        pub fn push_str(&mut self, text: &str) { loop {} }
        pub fn as_str(&self) -> &str { loop {} }
        pub fn len(&self) -> usize { loop {} }
        pub fn is_empty(&self) -> bool { loop {} }
        pub fn clear(&mut self) { loop {} }
    }
    impl Deref for String { type Target = str; fn deref(&self) -> &str { loop {} } }
    impl Clone for String { fn clone(&self) -> Self { loop {} } }
    impl Default for String { fn default() -> Self { loop {} } }
    impl From<&str> for String { fn from(text: &str) -> String { loop {} } }
    pub trait ToString { fn to_string(&self) -> String; }
    impl<T: fmt::Display + ?Sized> ToString for T { fn to_string(&self) -> String { loop {} } }
}

pub mod collections {
    use super::*;
    pub mod hash_map {
        pub use super::HashMap;
    }
    pub struct HashMap<K, V>(K, V);
    pub struct HashSet<T>(T);
    pub struct BTreeMap<K, V>(K, V);
    pub struct BTreeSet<T>(T);
    pub struct VecDeque<T>(T);
    pub struct Iter<'a, K, V>(&'a K, &'a V);
    impl<'a, K, V> Iterator for Iter<'a, K, V> { type Item = (&'a K, &'a V); fn next(&mut self) -> Option<(&'a K, &'a V)> { loop {} } }
    pub struct Keys<'a, K>(&'a K);
    impl<'a, K> Iterator for Keys<'a, K> { type Item = &'a K; fn next(&mut self) -> Option<&'a K> { loop {} } }
    pub struct Values<'a, V>(&'a V);
    impl<'a, V> Iterator for Values<'a, V> { type Item = &'a V; fn next(&mut self) -> Option<&'a V> { loop {} } }
    macro_rules! map {
        ($map:ident) => {
            impl<K, V> $map<K, V> {
                pub fn new() -> Self { loop {} }
                pub fn insert(&mut self, key: K, value: V) -> Option<V> { loop {} }
                pub fn get(&self, key: &K) -> Option<&V> { loop {} }
                pub fn get_mut(&mut self, key: &K) -> Option<&mut V> { loop {} }
                pub fn remove(&mut self, key: &K) -> Option<V> { loop {} }
                pub fn contains_key(&self, key: &K) -> bool { loop {} }
                pub fn len(&self) -> usize { loop {} }
                pub fn is_empty(&self) -> bool { loop {} }
                pub fn iter(&self) -> Iter<'_, K, V> { loop {} }
                pub fn keys(&self) -> Keys<'_, K> { loop {} }
                pub fn values(&self) -> Values<'_, V> { loop {} }
            }
            impl<K, V> Default for $map<K, V> { fn default() -> Self { loop {} } }
            impl<'a, K, V> IntoIterator for &'a $map<K, V> { type Item = (&'a K, &'a V); type IntoIter = Iter<'a, K, V>; fn into_iter(self) -> Iter<'a, K, V> { loop {} } }
        };
    }
    map!(HashMap);
    map!(BTreeMap);
    macro_rules! set {
        ($set:ident) => {
            impl<T> $set<T> {
                pub fn new() -> Self { loop {} }
                pub fn insert(&mut self, value: T) -> bool { loop {} }
                pub fn contains(&self, value: &T) -> bool { loop {} }
                pub fn remove(&mut self, value: &T) -> bool { loop {} }
                pub fn len(&self) -> usize { loop {} }
                pub fn is_empty(&self) -> bool { loop {} }
                pub fn iter(&self) -> Keys<'_, T> { loop {} }
            }
            impl<T> Default for $set<T> { fn default() -> Self { loop {} } }
            impl<'a, T> IntoIterator for &'a $set<T> { type Item = &'a T; type IntoIter = Keys<'a, T>; fn into_iter(self) -> Keys<'a, T> { loop {} } }
        };
    }
    set!(HashSet);
    set!(BTreeSet);
    impl<T> VecDeque<T> {
        pub fn new() -> Self { loop {} }
        pub fn push_back(&mut self, value: T) { loop {} }
        pub fn push_front(&mut self, value: T) { loop {} }
        pub fn pop_back(&mut self) -> Option<T> { loop {} }
        pub fn pop_front(&mut self) -> Option<T> { loop {} }
        pub fn len(&self) -> usize { loop {} }
        pub fn is_empty(&self) -> bool { loop {} }
    }
}

pub mod borrow_owned {
    pub trait ToOwned { type Owned; fn to_owned(&self) -> Self::Owned; }
    impl ToOwned for str { type Owned = crate::string::String; fn to_owned(&self) -> crate::string::String { loop {} } }
    impl<T: Clone> ToOwned for [T] { type Owned = crate::vec::Vec<T>; fn to_owned(&self) -> crate::vec::Vec<T> { loop {} } }
}

pub mod fmt_string {
    pub fn format(arguments: core::fmt::Arguments<'_>) -> crate::string::String { loop {} }
    pub fn print(arguments: core::fmt::Arguments<'_>) { loop {} }
}

#[macro_export]
macro_rules! vec {
    () => { $crate::vec::Vec::new() };
    ($element:expr; $count:expr) => { { let _ = $count; $crate::vec::Vec::from_array([$element]) } };
    ($($element:expr),+ $(,)?) => { $crate::vec::Vec::from_array([$($element),+]) };
}
#[macro_export]
macro_rules! format { ($($argument:tt)*) => { $crate::fmt_string::format($crate::format_args!($($argument)*)) }; }
#[macro_export]
macro_rules! print { ($($argument:tt)*) => { $crate::fmt_string::print($crate::format_args!($($argument)*)) }; }
#[macro_export]
macro_rules! println { ($($argument:tt)*) => { $crate::fmt_string::print($crate::format_args!($($argument)*)) }; }
#[macro_export]
macro_rules! eprint { ($($argument:tt)*) => { $crate::fmt_string::print($crate::format_args!($($argument)*)) }; }
#[macro_export]
macro_rules! eprintln { ($($argument:tt)*) => { $crate::fmt_string::print($crate::format_args!($($argument)*)) }; }
#[macro_export]
macro_rules! dbg { ($($value:expr),* $(,)?) => { ($($value),*) }; }
#[macro_export]
macro_rules! assert_eq { ($left:expr, $right:expr $(, $($message:tt)*)?) => { { let _ = (&$left, &$right); } }; }
#[macro_export]
macro_rules! assert_ne { ($left:expr, $right:expr $(, $($message:tt)*)?) => { { let _ = (&$left, &$right); } }; }
#[macro_export]
macro_rules! debug_assert { ($($argument:tt)*) => { $crate::assert!($($argument)*) }; }
#[macro_export]
macro_rules! debug_assert_eq { ($($argument:tt)*) => { $crate::assert_eq!($($argument)*) }; }
#[macro_export]
macro_rules! unreachable { ($($argument:tt)*) => { $crate::panic!($($argument)*) }; }

pub use core::{assert, format_args, panic, todo, unimplemented, write, writeln, matches};

pub mod prelude {
    pub mod v1 {
        pub use core::prelude::v1::*;
        pub use crate::borrow_owned::ToOwned;
        pub use crate::boxed::Box;
        pub use crate::string::{String, ToString};
        pub use crate::vec::Vec;
        pub use crate::{
            assert_eq, assert_ne, dbg, debug_assert, debug_assert_eq, eprint, eprintln, format,
            print, println, unreachable, vec,
        };
    }
    pub mod rust_2015 { pub use super::v1::*; }
    pub mod rust_2018 { pub use super::v1::*; }
    pub mod rust_2021 { pub use super::v1::*; }
    pub mod rust_2024 { pub use super::v1::*; }
}
