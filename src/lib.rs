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
    hash::{
        Hash,
        Hasher,
    },
};

pub mod pool;
pub mod bytes;

#[derive(Debug)]
pub struct Unique<T> {
    inner: Inner<T>,
}

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
    entry: Option<Box<Entry<T>>>,
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
        &self.inner.entry.as_ref().unwrap().value
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

impl<T> Eq for Shared<T> where T: Eq { }

impl<T> Hash for Shared<T> where T: Hash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
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
        let entry = Some(Box::new(Entry { value, next: None, }));
        Inner { entry, pool_head, }
    }

    fn new_detached(value: T) -> Inner<T> {
        Inner::new(value, Arc::new(PoolHead {
            is_detached: AtomicBool::new(true),
            head: AtomicPtr::default(),
        }))
    }
}

impl<T> AsRef<T> for Unique<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        &self.inner.entry.as_ref().unwrap().value
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
        &mut self.inner.entry.as_mut().unwrap().value
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

impl<T> Hash for Unique<T> where T: Hash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

unsafe impl<T> Send for Inner<T> where T: Send {}
unsafe impl<T> Sync for Inner<T> where T: Sync {}

impl<T> Drop for Inner<T> {
    fn drop(&mut self) {
        if let Some(mut entry_box) = self.entry.take() {
            let mut head = self.pool_head.head.load(Ordering::SeqCst);
            loop {
                if self.pool_head.is_detached.load(Ordering::SeqCst) {
                    // pool is detached, terminate reenqueue process and drop entry
                    break;
                }
                let next = ptr::NonNull::new(head);
                entry_box.next = next;
                let entry = Box::leak(entry_box);
                match self.pool_head.head.compare_exchange(head, entry as *mut _, Ordering::SeqCst, Ordering::Relaxed) {
                    Ok(..) =>
                        break,
                    Err(value) => {

                        println!(
                            " ;; alloc_pool::Inner::Drop unhappy path for head = {:?}, value = {:?}, entry = {:?}",
                            head,
                            value,
                            entry as *mut _,
                        );

                        head = value;
                        entry_box = unsafe { Box::from_raw(entry as *mut _) };
                    },
                }
            }
        }
    }
}

impl<T> Drop for PoolHead<T> {
    fn drop(&mut self) {
        // forbid entries list append
        self.is_detached.store(true, Ordering::SeqCst);

        // drop entries
        let head = self.head.load(Ordering::SeqCst);
        let mut maybe_entry_ptr = ptr::NonNull::new(head);
        while let Some(entry_ptr) = maybe_entry_ptr {
            let next_head = match unsafe { entry_ptr.as_ref().next } {
                None =>
                    ptr::null_mut(),
                Some(non_null) =>
                    non_null.as_ptr(),
            };
            let entry_ptr_raw = entry_ptr.as_ptr();
            let next_ptr = match self.head.compare_exchange(entry_ptr_raw, next_head, Ordering::SeqCst, Ordering::Relaxed) {
                Ok(entry_ptr_raw) => {
                    let _entry = unsafe { Box::from_raw(entry_ptr_raw) };
                    next_head
                },
                Err(value) => {

                    println!(
                        " ;; alloc_pool::PoolHead::Drop unhappy path for entry_ptr_raw = {:?}, value = {:?}",
                        entry_ptr_raw,
                        value,
                    );

                    value
                },
            };
            maybe_entry_ptr = ptr::NonNull::new(next_ptr);
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
