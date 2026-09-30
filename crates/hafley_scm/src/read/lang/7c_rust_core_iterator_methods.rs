                fn enumerate(self) -> crate::shim::Enumerate<Self> where Self: Sized { loop {} }
                fn zip<U: IntoIterator>(self, other: U) -> crate::shim::Zip<Self, U::IntoIter> where Self: Sized { loop {} }
                fn chain<U: IntoIterator<Item = Self::Item>>(self, other: U) -> crate::shim::Chain<Self, U::IntoIter> where Self: Sized { loop {} }
                fn rev(self) -> crate::shim::Same<Self> where Self: Sized { loop {} }
                fn skip(self, n: usize) -> crate::shim::Same<Self> where Self: Sized { loop {} }
                fn step_by(self, step: usize) -> crate::shim::Same<Self> where Self: Sized { loop {} }
                fn peekable(self) -> crate::shim::Peekable<Self> where Self: Sized { loop {} }
                fn inspect<F: FnMut(&Self::Item)>(self, f: F) -> crate::shim::Same<Self> where Self: Sized { loop {} }
                fn take_while<P: FnMut(&Self::Item) -> bool>(self, predicate: P) -> crate::shim::Same<Self> where Self: Sized { loop {} }
                fn skip_while<P: FnMut(&Self::Item) -> bool>(self, predicate: P) -> crate::shim::Same<Self> where Self: Sized { loop {} }
                fn cloned<'a, T: 'a + Clone>(self) -> crate::shim::Cloned<Self> where Self: Sized + Iterator<Item = &'a T> { loop {} }
                fn copied<'a, T: 'a + Copy>(self) -> crate::shim::Cloned<Self> where Self: Sized + Iterator<Item = &'a T> { loop {} }
                fn flat_map<U: IntoIterator, F: FnMut(Self::Item) -> U>(self, f: F) -> crate::shim::FlatMap<Self, U, F> where Self: Sized { loop {} }
                fn flatten(self) -> crate::shim::Flatten<Self> where Self: Sized, Self::Item: IntoIterator { loop {} }
                fn count(self) -> usize where Self: Sized { loop {} }
                fn last(self) -> Option<Self::Item> where Self: Sized { loop {} }
                fn max(self) -> Option<Self::Item> where Self: Sized { loop {} }
                fn min(self) -> Option<Self::Item> where Self: Sized { loop {} }
                fn max_by_key<B, F: FnMut(&Self::Item) -> B>(self, f: F) -> Option<Self::Item> where Self: Sized { loop {} }
                fn min_by_key<B, F: FnMut(&Self::Item) -> B>(self, f: F) -> Option<Self::Item> where Self: Sized { loop {} }
                fn sum<S>(self) -> S where Self: Sized { loop {} }
                fn any<F: FnMut(Self::Item) -> bool>(&mut self, f: F) -> bool where Self: Sized { loop {} }
                fn all<F: FnMut(Self::Item) -> bool>(&mut self, f: F) -> bool where Self: Sized { loop {} }
                fn find<P: FnMut(&Self::Item) -> bool>(&mut self, predicate: P) -> Option<Self::Item> where Self: Sized { loop {} }
                fn find_map<B, F: FnMut(Self::Item) -> Option<B>>(&mut self, f: F) -> Option<B> where Self: Sized { loop {} }
                fn position<P: FnMut(Self::Item) -> bool>(&mut self, predicate: P) -> Option<usize> where Self: Sized { loop {} }
                fn fold<B, F: FnMut(B, Self::Item) -> B>(self, init: B, f: F) -> B where Self: Sized { loop {} }
                fn for_each<F: FnMut(Self::Item)>(self, f: F) where Self: Sized { loop {} }
