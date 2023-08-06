use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::process::abort;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

const PTR_WIDTH: usize = usize::BITS as usize;
const BUCKET_COUNT: usize = PTR_WIDTH/*((1 << (PTR_WIDTH - 1)) / PTR_WIDTH)*/;

// just note atomic clearing support is in theory possible, but requires putting storage in a separate allocation that can be swapped using
// an atomic pointer and thus requires one additional acquiring atomic load on any action. Or else it might be possible to add an additional
// tiny bitset with a fixed capacity of PTR_WIDTH to the central data structure which indicates which buckets are valid at a moment. This
// can be inlined into the already used cache lines because the BUCKET_COUNT probably isn't a multiple of the cache line size.

pub struct AtomicBitSet {
    buckets: [AtomicPtr<AtomicUsize>; BUCKET_COUNT],
}

impl AtomicBitSet {

    pub fn new() -> Self {
        const NULL: AtomicPtr<AtomicUsize> = AtomicPtr::new(null_mut());

        Self {
            buckets: [NULL; BUCKET_COUNT],
        }
    }

    pub fn add(&self, val: usize) -> bool {
        let (bucket, bucket_size, index) = index(val / PTR_WIDTH);
        let sub_index = val % PTR_WIDTH;
        let storage_bucket = self.buckets[bucket].load(Ordering::Acquire);
        let storage_bucket = if storage_bucket.is_null() {
            let alloc = unsafe { alloc_zeroed(Layout::array::<AtomicUsize>(bucket_size).unwrap()) };
            if alloc.is_null() {
                abort();
            }
            match self.buckets[bucket].compare_exchange(null_mut(), alloc.cast::<AtomicUsize>(), Ordering::Release, Ordering::Acquire) {
                Ok(_) => alloc.cast::<AtomicUsize>(),
                Err(val) => {
                    unsafe { dealloc(alloc, Layout::array::<AtomicUsize>(bucket_size).unwrap_unchecked()); }
                    val
                }
            }
        } else {
            storage_bucket
        };
        unsafe { &*storage_bucket.add(index) }.fetch_or(1 << sub_index, Ordering::AcqRel) & (1 << sub_index) != 0
    }

    pub fn remove(&self, val: usize) -> bool {
        let (bucket, _, index) = index(val / PTR_WIDTH);
        let sub_index = val % PTR_WIDTH;
        let storage_bucket = self.buckets[bucket].load(Ordering::Acquire);
        if storage_bucket.is_null() {
            return false;
        }
        let cell_value = unsafe { &*storage_bucket.add(index) }.fetch_and(!(1 << sub_index), Ordering::AcqRel);
        cell_value & (1 << sub_index) != 0
    }

    pub fn contains(&self, val: usize) -> bool {
        let (bucket, _, index) = index(val / PTR_WIDTH);
        let sub_index = val % PTR_WIDTH;
        let storage_bucket = self.buckets[bucket].load(Ordering::Acquire);
        if storage_bucket.is_null() {
            return false;
        }
        let cell_value = unsafe { &*storage_bucket.add(index) }.load(Ordering::Acquire);
        cell_value & (1 << sub_index) != 0
    }

    pub fn clear(&mut self) {
        for (i, bucket) in self.buckets.iter_mut().enumerate() {
            if bucket.get_mut().is_null() {
                break;
            }
            unsafe { dealloc(bucket.get_mut().cast::<u8>(), Layout::array::<AtomicUsize>(1 << i).unwrap_unchecked()); }
            *bucket.get_mut() = null_mut();
        }
    }

}

impl Drop for AtomicBitSet {
    fn drop(&mut self) {
        for (i, bucket) in self.buckets.iter_mut().enumerate() {
            if bucket.get_mut().is_null() {
                break;
            }
            unsafe { dealloc(bucket.get_mut().cast::<u8>(), Layout::array::<AtomicUsize>(1 << i).unwrap_unchecked()); }
        }
    }
}

#[inline]
fn index(val: usize) -> (usize, usize, usize) {
    let bucket = usize::from(PTR_WIDTH) - ((val + 1).leading_zeros() as usize) - 1;
    let bucket_size = 1 << bucket;
    let index = val - (bucket_size - 1);
    (bucket, bucket_size, index)
}

#[inline]
const fn most_sig_set_bit(val: usize) -> Option<u32> {
    let mut i = 0;
    let mut ret = None;
    while i < usize::BITS {
        if val & (1 << i) != 0 {
            ret = Some(i);
        }
        i += 1;
    }
    ret
}

mod tests {
    use super::*;

    #[test]
    fn test_insert() {
        let set = AtomicBitSet::new();
        set.add(20);
        assert!(set.contains(20));
        assert!(!set.contains(21));
    }

    #[test]
    fn test_removal() {
        let set = AtomicBitSet::new();
        set.add(20);
        set.add(21);
        assert!(set.contains(20));
        set.remove(20);
        assert!(!set.contains(20));
        assert!(set.contains(21));
    }

}
#[cfg(test)]
mod atomic_set_test {
    use super::*;

    #[test]
    fn insert() {
        let mut c = AtomicBitSet::new();
        for i in 0..1_000 {
            assert!(!c.add(i));
            assert!(c.add(i));
        }

        for i in 0..1_000 {
            assert!(c.contains(i));
        }
    }

    #[test]
    fn insert_100k() {
        let mut c = AtomicBitSet::new();
        for i in 0..100_000 {
            assert!(!c.add(i));
            assert!(c.add(i));
        }

        for i in 0..100_000 {
            assert!(c.contains(i));
        }
    }

    #[test]
    fn remove() {
        let mut c = AtomicBitSet::new();
        for i in 0..1_000 {
            assert!(!c.add(i));
        }

        for i in 0..1_000 {
            assert!(c.contains(i));
            assert!(c.remove(i));
            assert!(!c.contains(i));
            assert!(!c.remove(i));
        }
    }

    /*#[test]
    fn iter() {
        let mut c = AtomicBitSet::new();
        for i in 0..100_000 {
            c.add(i);
        }

        let mut count = 0;
        for (idx, i) in c.iter().enumerate() {
            count += 1;
            assert_eq!(idx, i as usize);
        }
        assert_eq!(count, 100_000);
    }*/

    /*#[test]
    fn clear() {
        let mut set = AtomicBitSet::new();
        for i in 0..1_000 {
            set.add(i);
        }

        assert_eq!((&set).iter().sum::<u32>(), 500_500 - 1_000);

        assert_eq!((&set).iter().count(), 1_000);
        set.clear();
        assert_eq!((&set).iter().count(), 0);

        for i in 0..1_000 {
            set.add(i * 64);
        }

        assert_eq!((&set).iter().count(), 1_000);
        set.clear();
        assert_eq!((&set).iter().count(), 0);

        for i in 0..1_000 {
            set.add(i * 1_000);
        }

        assert_eq!((&set).iter().count(), 1_000);
        set.clear();
        assert_eq!((&set).iter().count(), 0);

        for i in 0..100 {
            set.add(i * 10_000);
        }

        assert_eq!((&set).iter().count(), 100);
        set.clear();
        assert_eq!((&set).iter().count(), 0);

        for i in 0..10 {
            set.add(i * 10_000);
        }

        assert_eq!((&set).iter().count(), 10);
        set.clear();
        assert_eq!((&set).iter().count(), 0);
    }*/
}

/*
mod test_other {
    use std::hint::{black_box, spin_loop};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use hibitset::*;

    #[test]
    fn test_atomics() {
        let tmp = Arc::new(AtomicBitSet::new());
        for _ in 0..2 {
            let started = Arc::new(AtomicBool::new(false));
            let mut threads = vec![];
            for _ in 0..1 {
                let tmp = tmp.clone();
                let started = started.clone();
                threads.push(thread::spawn(move || {
                    while !started.load(Ordering::Acquire) {
                        spin_loop();
                    }
                    for _ in 0..20000 {
                        let l1 = tmp.contains(rand::random());
                        black_box(l1);
                    }
                }));
            }
            for _ in 0..1 {
                let tmp = tmp.clone();
                let started = started.clone();
                threads.push(thread::spawn(move || {
                    while !started.load(Ordering::Acquire) {
                        spin_loop();
                    }
                    for _ in 0..20000 {
                        tmp.add_atomic(In)
                    }
                }));
            }
            let start = Instant::now();
            started.store(true, Ordering::Release);
            threads.into_iter().for_each(|thread| thread.join().unwrap());
            diff += start.elapsed();
        }
    }

}
*/