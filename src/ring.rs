use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Wrapping FIFO Ring Buffer
/// Not tested, no guarantees (it seems to be safe though)
pub struct RingBuffer<T, const SIZE: usize> {
    buf: [MaybeUninit<T>; SIZE],
    index: AtomicUsize,
    usage: AtomicUsize,
}

impl<T, const SIZE: usize> RingBuffer<T, { SIZE }> {
    pub fn new() -> Self {
        if SIZE == 0 {
            panic!("SIZE can not be 0");
        }
        Self {
            /// copied from code of https://doc.rust-lang.org/std/mem/union.MaybeUninit.html#method.uninit_array as the function is unstable
            buf: unsafe { MaybeUninit::<[MaybeUninit<T>; SIZE]>::uninit().assume_init() },
            index: AtomicUsize::new(0),
            usage: AtomicUsize::new(0),
        }
    }

    pub fn push(&self, value: T) -> Option<T> {
        loop {
            let u = self.usage.load(Ordering::Relaxed);
            let i = self.index.load(Ordering::Acquire);

            return if u < SIZE {
                if self
                    .usage
                    .compare_exchange(u, u + 1, Ordering::AcqRel, Ordering::Relaxed)
                    .is_err()
                {
                    continue;
                }
                let index = (i + u) % SIZE;
                unsafe {
                    *(self.buf.as_ptr().add(index) as *mut MaybeUninit<T>) =
                        MaybeUninit::new(value);
                }
                None
            } else {
                if self
                    .index
                    .compare_exchange(i, (i + 1) % SIZE, Ordering::AcqRel, Ordering::Relaxed)
                    .is_err()
                {
                    continue;
                }
                let mut val = MaybeUninit::new(value);
                unsafe {
                    std::ptr::swap(
                        *(&self.buf.as_ptr().add(i)) as *mut MaybeUninit<T>,
                        &mut val as *mut MaybeUninit<T>,
                    )
                }
                Some(unsafe { val.assume_init() })
            };
        }
    }

    pub fn pop(&self) -> Option<T> {
        loop {
            let u = self.usage.load(Ordering::Acquire);
            let i = self.index.load(Ordering::Acquire);
            if u == 0 {
                return None;
            }
            if self
                .index
                .compare_exchange(i, (i + 1) % SIZE, Ordering::AcqRel, Ordering::Relaxed)
                .is_err()
            {
                continue;
            }
            let _ = self.usage.fetch_sub(1, Ordering::AcqRel);

            let mut val = std::mem::MaybeUninit::zeroed();
            unsafe {
                std::ptr::swap(
                    *(&self.buf.as_ptr().add(i)) as *mut MaybeUninit<T>,
                    &mut val as *mut MaybeUninit<T>,
                )
            };
            return Some(unsafe { val.assume_init() });
        }
    }
}

#[cfg(test)]
#[test]
pub fn st_test() {
    let rb = Arc::new(RingBuffer::<u32, 4>::new());
    for i in 0..=8 {
        rb.push(i);
    }
    let read = [
        rb.pop().unwrap(),
        rb.pop().unwrap(),
        rb.pop().unwrap(),
        rb.pop().unwrap(),
    ];
    assert_eq!(read, [5, 6, 7, 8])
}

#[cfg(test)]
#[test]
pub fn mt_test() {
    const SIZE: usize = 1024;
    let rb = Arc::new(RingBuffer::<Instant, SIZE>::new());
    let mut jj = vec![];
    for _ in 0..4 {
        let rbc = rb.clone();
        jj.push(std::thread::spawn(move || {
            for _ in 0..1000000 {
                rbc.push(Instant::now());
            }
        }));
        let rbc = rb.clone();
        jj.push(std::thread::spawn(move || {
            for _ in 0..1000000 {
                rbc.push(Instant::now());
            }
        }));
    }
    jj.into_iter().for_each(|j| j.join().unwrap());

    let mut last = rb.pop().unwrap();
    for _ in 1..SIZE {
        let next = rb.pop().unwrap();
        assert!(last <= next);
        last = next;
    }
    assert_eq!(rb.pop(), None);
}
