use std::iter::FromIterator;
use std::ops::{Deref, Index, Rem};

pub struct WrappingVec<T> {
    inner: Vec<T>,
}

impl<T> Deref for WrappingVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &<Self as Deref>::Target {
        &self.inner
    }
}

impl<T> From<Vec<T>> for WrappingVec<T> {
    fn from(inner: Vec<T>) -> Self {
        WrappingVec { inner }
    }
}

impl<T> FromIterator<T> for WrappingVec<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        iter.into_iter().collect::<Vec<_>>().into()
    }
}

impl<T, I: Rem<usize, Output = O>, O> Index<I> for WrappingVec<T>
where
    Vec<T>: Index<O>,
{
    type Output = <Vec<T> as Index<O>>::Output;

    fn index(&self, index: I) -> &<Self as Index<I>>::Output {
        &self.inner[index % self.inner.len()]
    }
}
