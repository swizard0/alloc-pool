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

#[derive(Clone, Debug)]
pub struct Bytes {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    entry: ptr::NonNull<Entry>,
    pool_head: Arc<PoolHead>,
}

#[derive(Debug)]
struct PoolHead {
    is_detached: AtomicBool,
    head: AtomicPtr<Entry>,
}

#[derive(Debug)]
struct Entry {
    bytes: Vec<u8>,
    next: Option<ptr::NonNull<Entry>>,
}

impl AsRef<[u8]> for Bytes {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        unsafe {
            &self.inner.entry.as_ref().bytes
        }
    }
}

impl Deref for Bytes {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_ref()
    }
}

impl PartialEq for Bytes {
    fn eq(&self, other: &Bytes) -> bool {
        self.as_ref() == other.as_ref()
    }
}

#[derive(Debug)]
pub struct BytesMut {
    inner: Inner,
}

impl BytesMut {
    pub fn new_detached() -> BytesMut {
        BytesMut { inner: Inner::new_detached(), }
    }

    pub fn freeze(self) -> Bytes {
        Bytes {
            inner: Arc::new(self.inner),
        }
    }
}

impl Inner {
    fn new(pool_head: Arc<PoolHead>) -> Inner {
        let entry_box = Box::new(Entry { bytes: Vec::new(), next: None, });
        let entry = unsafe {
            ptr::NonNull::new_unchecked(Box::into_raw(entry_box))
        };
        Inner { entry, pool_head, }
    }

    fn new_detached() -> Inner {
        Inner::new(Arc::new(PoolHead {
            is_detached: AtomicBool::new(true),
            head: AtomicPtr::new(ptr::null_mut()),
        }))
    }
}

impl AsRef<[u8]> for BytesMut {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        unsafe {
            &self.inner.entry.as_ref().bytes
        }
    }
}

impl Deref for BytesMut {
    type Target = Vec<u8>;

    #[inline]
    fn deref(&self) -> &Vec<u8> {
        unsafe {
            &self.inner.entry.as_ref().bytes
        }
    }
}

impl AsMut<[u8]> for BytesMut {
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe {
            &mut self.inner.entry.as_mut().bytes
        }
    }
}

impl DerefMut for BytesMut {
    #[inline]
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        unsafe {
            &mut self.inner.entry.as_mut().bytes
        }
    }
}

impl Drop for Inner {
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

impl Drop for PoolHead {
    fn drop(&mut self) {
        // forbid entries list append
        self.is_detached.store(true, Ordering::SeqCst);

        // drop entries
        let mut head = self.head.load(Ordering::SeqCst);
        while !head.is_null() {
            unsafe {
                let entry_ptr = ptr::NonNull::new_unchecked(head);
                let next_head = match entry_ptr.as_ref().next {
                    None =>
                        ptr::null_mut(),
                    Some(non_null) =>
                        non_null.as_ptr(),
                };
                match self.head.compare_exchange(head, next_head, Ordering::SeqCst, Ordering::Relaxed) {
                    Ok(..) =>
                        head = next_head,
                    Err(value) =>
                        head = value,
                }
            }
        }
    }
}
