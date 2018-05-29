extern crate rand;

use std::cell::UnsafeCell;
use std::iter::repeat;
use std::ops::{Add, AddAssign};
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

pub mod reference;

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

struct RingBuffer<T> {
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

    // invariant: self.head % self.size is empty
    pub fn write(&self, x: T) {
        let head = self.head.load(Ordering::SeqCst);
        // Get the pointer to the next slot, which is guaranteed to be empty.
        let head_ptr: *mut Option<T> = self.data[head].get();
        let head_ptr: &mut Option<T> = unsafe { &mut *head_ptr };
        *head_ptr = Some(x);

        // linearisation point if we exit because head - tail.value < self.size:
        self.head.store(head + 1, Ordering::SeqCst);

        loop {
            let tail: Tail = self.tail.load(Ordering::SeqCst).into();

            if head - tail.value < self.size {
                break;
            }

            if tail.locked {
                continue;
            }

            // linearisation point if we exit because we pushed the tail pointer:
            if self.tail
                .compare_and_swap(tail.into(), (tail + 1).into(), Ordering::SeqCst)
                == tail.into()
            {
                break;
            }
        }
    }

    pub fn read(&self) -> RingIter<T> {
        let mut tail = loop {
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

        // The head may have been pushed some number of times since we successfully read and locked
        // the tail, but the tail has not been moved (as the reading thread does not try to push
        // the tail if it is locked).
        // If the head is no more than 'size' away from the tail, then we can take the linearisation
        // point as here. The linearisation points of the other thread will be at the moments it
        // updated the head, as it has not hit the case where it had to push the tail as well.
        //
        // If the head is `size+1` away from the tail, then the other thread has push until it hit
        // the tail, then gotten stuck as the tail was locked. In this case we can help it out by
        // pushing the tail ourselves. We do not try to pull out the data ourselves as the other
        // thread might overwrite it in the meantime via the head pointer. Rather, the writing
        // thread will implicitly drop the contents of this slot when it writes to that slot.
        //
        // In the former case, we set this to be the linearisation point. In the latter, we set the
        // linearisation point to the moment we pushed the tail.
        let head = self.head.load(Ordering::SeqCst);

        if head - tail.value == self.size + 1 {
            tail += 1;
            self.tail.store(tail.into(), Ordering::SeqCst);
        }

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
