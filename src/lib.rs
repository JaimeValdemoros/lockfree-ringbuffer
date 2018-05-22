extern crate rand;

mod wrapping_vec;

use wrapping_vec::WrappingVec;

use std::cell::UnsafeCell;
use std::iter::repeat;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

pub struct RingBuffer<T> {
    pub(crate) size: usize,
    pub(crate) head: AtomicUsize,
    pub(crate) tail: AtomicIsize,
    pub(crate) data: WrappingVec<UnsafeCell<Option<T>>>,
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

    pub fn push(&self, x: T) {
        let head = self.head.load(Ordering::SeqCst);
        // Get the pointer to the next slot, which is guaranteed to be empty.
        let head_ptr: *mut Option<T> = self.data[head].get();
        let head_ptr: &mut Option<T> = unsafe { &mut *head_ptr };
        *head_ptr = Some(x);
        self.head.store(head + 1, Ordering::SeqCst);

        loop {
            let tail = self.tail.load(Ordering::SeqCst);

            if head - (tail.abs() as usize) < self.size {
                break;
            }

            if tail.is_negative() {
                continue;
            }

            if self.tail.compare_and_swap(tail, tail + 1, Ordering::SeqCst) == tail {
                let tail_ptr: *mut Option<T> = self.data[tail as usize].get();
                let tail_ptr: &mut Option<T> = unsafe { &mut *tail_ptr };
                tail_ptr.take();
                break;
            }
        }
    }

    pub fn read(&self) -> RingIter<T> {
        let tail = loop {
            let tail = self.tail.load(Ordering::SeqCst);
            if self.tail.compare_and_swap(tail, -tail, Ordering::SeqCst) == tail {
                break tail;
            }
        };
        let head = self.head.load(Ordering::SeqCst);
        RingIter {
            fixed_head: head,
            current_tail: tail as usize,
            inner: &self,
        }
    }
}

pub struct RingIter<'a, T: 'a> {
    pub(crate) fixed_head: usize,
    pub(crate) current_tail: usize,
    pub(crate) inner: &'a RingBuffer<T>,
}

impl<'a, T> Iterator for RingIter<'a, T> {
    type Item = T;

    fn next(&mut self) -> Option<<Self as Iterator>::Item> {
        if self.current_tail == self.fixed_head {
            None
        } else {
            let res = {
                let tail_ptr: *mut Option<T> = self.inner.data[self.current_tail].get();
                let tail_ptr: &mut Option<T> = unsafe { &mut *tail_ptr };
                tail_ptr.take()
            };
            let new_tail = self.current_tail + 1;
            self.current_tail = new_tail;
            let store_tail = if new_tail == self.fixed_head {
                new_tail as isize
            } else {
                -(new_tail as isize)
            };
            self.inner.tail.store(store_tail, Ordering::SeqCst);
            res
        }
    }
}

impl<'a, T: 'a> Drop for RingIter<'a, T> {
    fn drop(&mut self) {
        if self.current_tail < self.fixed_head {
            self.inner
                .tail
                .store(self.current_tail as isize, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
