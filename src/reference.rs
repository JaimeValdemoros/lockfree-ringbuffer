pub struct BoundedBuffer<T> {
    size: usize,
    data: Vec<T>,
}

impl<T> BoundedBuffer<T> {
    pub fn new(size: usize) -> Self {
        assert!(size > 0);
        BoundedBuffer {
            size,
            data: vec![],
        }
    }

    pub fn write(&mut self, x: T) {
        if self.data.len() < self.size {
            self.data.push(x)
        } else {
            self.data.rotate_left(1);
            self.data[self.size-1] = x
        }
    }

    pub fn read(&mut self) -> Vec<T> {
        ::std::mem::replace(&mut self.data, vec![])
    }
}