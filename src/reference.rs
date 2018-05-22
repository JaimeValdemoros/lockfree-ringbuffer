use std::iter::once;
use rand::{Rand, Rng};

pub struct BoundedBuffer<T> {
    size: usize,
    data: Vec<T>,
}

pub trait Apply {
    type Op: Rand;
    type OpRes;
    fn apply(&mut self, op: Self::Op) -> Self::OpRess;
}

pub enum Ops<T> {
    Push(T),
    Pop,
}

impl<T: Rand> Rand for Ops<T> {
    fn rand<R: Rng>(rng: &mut R) -> Self {
        <Option<T> as Rand>::rand(rng).map_or(Ops::Pop, Ops::Push)
    }
}

pub enum OpRes<T> {
    Pushed,
    PoppedEmpty,
    PoppedGot(T),
}

impl<T> Apply for RingBuffer<T> {
    type Op = Ops<T>;
    type OpRes = OpRes<T>;
    fn apply(&mut self, op: Self::Op) -> Self::OpRes {
        match op {
            Ops::Push(x) => { self.push(x); OpRes::Pushed }
            Ops::Pop => self.read().map_or(OpRes::PoppedEmpty, OpRes::PoppedGot),
        }
    }
}

impl<T> RingBuffer<T> {
    pub fn new(size: usize) -> Self {
        assert!(size > 0);
        RingBuffer {
            size,
            data: Vec::with_capacity(size),
        }
    }

    pub fn write(&mut self, x: T) -> Self {
        if self.data.len() < self.size {
            self.data.push(x)
        } else {
            self.data.rotate_left(1);
            self.data[self.size-1] = x
        }
    }

    pub fn read(&mut self) -> Option<T> {
        if self.data.is_empty() {
            None
        } else {
            self.data.rotate_left(1);
            self.data.pop()
        }
    }
}