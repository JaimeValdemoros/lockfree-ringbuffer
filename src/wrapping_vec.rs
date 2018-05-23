use std::iter::FromIterator;
use std::ops::{Deref, Index, IndexMut, Rem};

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

impl<T, I: Rem<usize>> Index<I> for WrappingVec<T>
where
    Vec<T>: Index<<I as Rem<usize>>::Output>,
{
    type Output = <Vec<T> as Index<<I as Rem<usize>>::Output>>::Output;

    fn index(&self, index: I) -> &<Self as Index<I>>::Output {
        &self.inner[index % self.inner.len()]
    }
}

impl<T, I: Rem<usize>> IndexMut<I> for WrappingVec<T>
    where
        Vec<T>: IndexMut<<I as Rem<usize>>::Output>,
{
    fn index_mut(&mut self, index: I) -> &mut <Self as Index<I>>::Output {
        let len = self.inner.len();
        &mut self.inner[index % len]
    }
}