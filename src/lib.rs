use std::{
    ptr,
    sync::{
        Arc,
        atomic::{
            Ordering,
            AtomicPtr,
            AtomicBool,
        },
    },
    ops::{
        Deref,
        DerefMut,
    },
};

pub mod pool;
pub mod bytes;

#[derive(Debug)]
pub struct Shared<T> {
    inner: Arc<Inner<T>>,
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Shared<T> {
        Shared { inner: self.inner.clone(), }
    }
}

#[derive(Debug)]
struct Inner<T> {
    entry: ptr::NonNull<Entry<T>>,
    pool_head: Arc<PoolHead<T>>,
}

#[derive(Debug)]
struct PoolHead<T> {
    is_detached: AtomicBool,
    head: AtomicPtr<Entry<T>>,
}

#[derive(Debug)]
struct Entry<T> {
    value: T,
    next: Option<ptr::NonNull<Entry<T>>>,
}

impl<T> AsRef<T> for Shared<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        unsafe {
            &self.inner.entry.as_ref().value
        }
    }
}

impl<T> Deref for Shared<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.as_ref()
    }
}

impl<T> PartialEq for Shared<T> where T: PartialEq {
    fn eq(&self, other: &Shared<T>) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<T> PartialEq<T> for Shared<T> where T: PartialEq {
    fn eq(&self, other: &T) -> bool {
        self.as_ref() == other
    }
}

#[derive(Debug)]
pub struct Unique<T> {
    inner: Inner<T>,
}

impl<T> Unique<T> {
    pub fn new_detached(value: T) -> Self {
        Self { inner: Inner::new_detached(value), }
    }

    pub fn freeze(self) -> Shared<T> {
        Shared {
            inner: Arc::new(self.inner),
        }
    }
}

impl<T> Inner<T> {
    fn new(value: T, pool_head: Arc<PoolHead<T>>) -> Inner<T> {
        let entry_box = Box::new(Entry { value, next: None, });
        let entry = unsafe {
            ptr::NonNull::new_unchecked(Box::into_raw(entry_box))
        };
        Inner { entry, pool_head, }
    }

    fn new_detached(value: T) -> Inner<T> {
        Inner::new(value, Arc::new(PoolHead {
            is_detached: AtomicBool::new(true),
            head: AtomicPtr::new(ptr::null_mut()),
        }))
    }
}

impl<T> AsRef<T> for Unique<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        unsafe {
            &self.inner.entry.as_ref().value
        }
    }
}

impl<T> Deref for Unique<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        self.as_ref()
    }
}

impl<T> AsMut<T> for Unique<T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        unsafe {
            &mut self.inner.entry.as_mut().value
        }
    }
}

impl<T> DerefMut for Unique<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        self.as_mut()
    }
}

impl<T> PartialEq for Unique<T> where T: PartialEq {
    fn eq(&self, other: &Unique<T>) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<T> PartialEq<T> for Unique<T> where T: PartialEq {
    fn eq(&self, other: &T) -> bool {
        self.as_ref() == other
    }
}

unsafe impl<T> Send for Inner<T> where T: Send {}
unsafe impl<T> Sync for Inner<T> where T: Sync {}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        unsafe {
            let mut head = self.pool_head.head.load(Ordering::SeqCst);
            loop {
                if self.pool_head.is_detached.load(Ordering::SeqCst) {
                    // pool is detached, terminate reenqueue process and drop entry
                    let _entry = Box::from_raw(self.entry.as_ptr());
                    break;
                }
                self.entry.as_mut().next = if head.is_null() {
                    None
                } else {
                    Some(ptr::NonNull::new_unchecked(head))
                };
                match self.pool_head.head.compare_exchange(head, self.entry.as_ptr(), Ordering::SeqCst, Ordering::Relaxed) {
                    Ok(..) =>
                        break,
                    Err(value) =>
                        head = value,
                }
            }
        }
    }
}

impl<T> Drop for PoolHead<T> {
    fn drop(&mut self) {
        unsafe {
            // forbid entries list append
            self.is_detached.store(true, Ordering::SeqCst);

            // drop entries
            let mut head = self.head.load(Ordering::SeqCst);
            while !head.is_null() {
                let entry_ptr = ptr::NonNull::new_unchecked(head);
                let next_head = match entry_ptr.as_ref().next {
                    None =>
                        ptr::null_mut(),
                    Some(non_null) =>
                        non_null.as_ptr(),
                };
                match self.head.compare_exchange(head, next_head, Ordering::SeqCst, Ordering::Relaxed) {
                    Ok(..) => {
                        let _entry = Box::from_raw(entry_ptr.as_ptr());
                        head = next_head;
                    },
                    Err(value) =>
                        head = value,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        mem::drop,
        sync::{
            Arc,
            atomic::{
                Ordering,
                AtomicUsize,
            },
        },
    };

    use super::{
        pool::Pool,
        bytes::BytesPool,
    };

    #[test]
    fn basic() {
        let mut make_counter = 0;
        let drop_counter = Arc::new(AtomicUsize::new(0));

        #[derive(Debug)]
        struct Sample {
            contents: &'static str,
            drop_counter: Arc<AtomicUsize>,
        }

        impl Drop for Sample {
            fn drop(&mut self) {
                self.drop_counter.fetch_add(1, Ordering::SeqCst);
            }
        }

        let pool = Pool::new();

        let sample_a = "hello, world!";
        let sample_b = "goodbye, world!";

        let value = pool.lend(|| { make_counter += 1; Sample { contents: sample_a, drop_counter: drop_counter.clone(), } });
        assert_eq!(value.contents, sample_a);
        assert_eq!(make_counter, 1);

        drop(value);
        assert_eq!(drop_counter.load(Ordering::SeqCst), 0);

        let value_a = pool.lend(|| { make_counter += 1; Sample { contents: sample_b, drop_counter: drop_counter.clone(), } });
        assert_eq!(value_a.contents, sample_a);
        assert_eq!(make_counter, 1);

        let value_b = pool.lend(|| { make_counter += 1; Sample { contents: sample_b, drop_counter: drop_counter.clone(), } });
        assert_eq!(value_b.contents, sample_b);
        assert_eq!(make_counter, 2);

        let value_a_shared = value_a.freeze();
        assert_eq!(value_a_shared.contents, sample_a);
        let value_a_shared_cloned = value_a_shared.clone();
        assert_eq!(value_a_shared_cloned.contents, sample_a);

        drop(value_a_shared);
        drop(value_a_shared_cloned);
        assert_eq!(drop_counter.load(Ordering::SeqCst), 0);

        let value_a = pool.lend(|| { make_counter += 1; Sample { contents: sample_b, drop_counter: drop_counter.clone(), } });
        assert_eq!(value_a.contents, sample_a);
        assert_eq!(make_counter, 2);

        drop(value_a);
        drop(value_b);
        drop(pool);
        assert_eq!(drop_counter.load(Ordering::SeqCst), make_counter);
    }

    #[test]
    fn bytes_pool_send_sync() {
        let pool = BytesPool::new();
        let bytes = pool.lend();

        std::thread::spawn(move || {
            let _bytes = bytes.freeze();
        });
    }
}
