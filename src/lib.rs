extern crate rand;

use std::cell::UnsafeCell;
use std::iter::repeat;
use std::ops::{Add, AddAssign};
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

pub struct RingBufferReader<T> {
    inner: Arc<RingBuffer<T>>,
}

impl<T: Send> RingBufferReader<T> {
    pub fn read(&mut self) -> RingIter<T> {
        self.inner.read()
    }
}

impl<T> From<Arc<RingBuffer<T>>> for RingBufferReader<T> {
    fn from(inner: Arc<RingBuffer<T>>) -> Self {
        RingBufferReader { inner }
    }
}

pub struct RingBufferWriter<T> {
    inner: Arc<RingBuffer<T>>,
}

impl<T: Send> RingBufferWriter<T> {
    pub fn write(&mut self, x: T) {
        self.inner.write(x)
    }
}

impl<T> From<Arc<RingBuffer<T>>> for RingBufferWriter<T> {
    fn from(inner: Arc<RingBuffer<T>>) -> Self {
        RingBufferWriter { inner }
    }
}

pub fn new<T: Send>(size: usize) -> (RingBufferReader<T>, RingBufferWriter<T>) {
    let buffer = Arc::new(RingBuffer::new(size));

    (buffer.clone().into(), buffer.clone().into())
}

pub struct RingBuffer<T> {
    pub(crate) size: usize,
    pub(crate) head: AtomicUsize,
    pub(crate) tail: AtomicIsize,
    pub(crate) data: Vec<UnsafeCell<Option<T>>>,
}

unsafe impl<T: Send> Sync for RingBuffer<T> {}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct Tail {
    pub locked: bool,
    pub value: usize,
}

impl Add<usize> for Tail {
    type Output = Self;

    fn add(mut self, rhs: usize) -> <Self as Add<usize>>::Output {
        self.value += rhs;
        self
    }
}

impl AddAssign<usize> for Tail {
    fn add_assign(&mut self, rhs: usize) {
        self.value += rhs;
    }
}

impl From<isize> for Tail {
    fn from(v: isize) -> Self {
        Tail {
            locked: v.is_negative(),
            value: if !v.is_negative() {
                v as usize
            } else {
                v.abs() as usize - 1
            },
        }
    }
}

impl Into<isize> for Tail {
    fn into(self) -> isize {
        if !self.locked {
            self.value as isize
        } else {
            -(self.value as isize) - 1
        }
    }
}

impl<T: Send> RingBuffer<T> {
    pub fn new(size: usize) -> Self {
        RingBuffer {
            size,
            head: 0.into(),
            tail: 0.into(),
            data: repeat(()).map(|()| None.into()).take(size + 1).collect(),
        }
    }

    pub fn write(&self, x: T) {
        let head = self.head.load(Ordering::SeqCst);
        // Get the pointer to the next slot, which is guaranteed to be empty.
        let head_ptr: *mut Option<T> = self.data[head].get();
        let head_ptr: &mut Option<T> = unsafe { &mut *head_ptr };
        *head_ptr = Some(x);

        loop {
            let tail: Tail = self.tail.load(Ordering::SeqCst).into();

            if head - tail.value < self.size {
                break;
            }

            if tail.locked {
                continue;
            }

            if self.tail
                .compare_and_swap(tail.into(), (tail + 1).into(), Ordering::SeqCst)
                == tail.into()
            {
                let tail_ptr: *mut Option<T> = self.data[tail.value].get();
                let tail_ptr: &mut Option<T> = unsafe { &mut *tail_ptr };
                *tail_ptr = None;
                break;
            }
        }

        self.head.store(head + 1, Ordering::SeqCst);
    }

    pub fn read(&self) -> RingIter<T> {
        let tail = loop {
            let tail: Tail = self.tail.load(Ordering::SeqCst).into();
            assert!(!tail.locked);
            let mut new_tail = tail;
            new_tail.locked = true;
            if self.tail
                .compare_and_swap(tail.into(), new_tail.into(), Ordering::SeqCst)
                == tail.into()
            {
                break new_tail;
            }
        };
        let head = self.head.load(Ordering::SeqCst);
        RingIter {
            fixed_head: head,
            current_tail: tail,
            inner: &self,
        }
    }
}

pub struct RingIter<'a, T: 'a> {
    pub(crate) fixed_head: usize,
    pub(crate) current_tail: Tail,
    pub(crate) inner: &'a RingBuffer<T>,
}

impl<'a, T> Iterator for RingIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        if !self.current_tail.locked {
            None
        } else {
            let res = {
                let tail_ptr: *mut Option<T> = self.inner.data[self.current_tail.value].get();
                let tail_ptr: &mut Option<T> = unsafe { &mut *tail_ptr };
                tail_ptr.take()
            };
            self.current_tail += 1;
            if self.current_tail.value == self.fixed_head {
                self.current_tail.locked = false;
            }
            self.inner
                .tail
                .store(self.current_tail.into(), Ordering::SeqCst);
            res
        }
    }
}

impl<'a, T: 'a> Drop for RingIter<'a, T> {
    fn drop(&mut self) {
        if self.current_tail.locked {
            self.current_tail.locked = false;
            self.inner
                .tail
                .store(self.current_tail.into(), Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_conversion() {
        for locked in &[true, false] {
            for value in 0..100 {
                let tail = Tail {
                    locked: *locked,
                    value,
                };
                assert_eq!(<Tail as From<isize>>::from(tail.into()), tail)
            }
        }

        for i in -20isize..=20isize {
            assert_eq!(<Tail as Into<isize>>::into(Tail::from(i)), i)
        }
    }
}
