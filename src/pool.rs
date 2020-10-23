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
};

use super::{
    Inner,
    Unique,
    PoolHead,
};

#[derive(Debug)]
pub struct Pool<T> {
    inner: Arc<PoolHead<T>>,
}

impl<T> Clone for Pool<T> {
    fn clone(&self) -> Pool<T> {
        Pool { inner: self.inner.clone(), }
    }
}

impl<T> Pool<T> {
    pub fn new() -> Pool<T> {
        Pool {
            inner: Arc::new(PoolHead {
                is_detached: AtomicBool::new(false),
                head: AtomicPtr::new(ptr::null_mut()),
            }),
        }
    }

    pub fn lend<F>(&self, make_value: F) -> Unique<T> where F: FnOnce() -> T {
        println!(" ;; requesting LEND ... ");
        unsafe {
            let mut head = self.inner.head.load(Ordering::SeqCst);
            loop {
                if head.is_null() {
                    println!(" ;; LEND done: NEW forced (empty queue)");
                    return Unique { inner: Inner::new(make_value(), self.inner.clone()), };
                }
                let entry_ptr = ptr::NonNull::new_unchecked(head);
                let next_head = match entry_ptr.as_ref().next {
                    None =>
                        ptr::null_mut(),
                    Some(non_null) =>
                        non_null.as_ptr(),
                };
                match self.inner.head.compare_exchange(head, next_head, Ordering::SeqCst, Ordering::Relaxed) {
                    Ok(..) => {
                        println!(" ;; LEND done: retrieved from queue: {:?} (next_head = {:?}", entry_ptr, next_head);
                        return Unique {
                            inner: Inner {
                                entry: entry_ptr,
                                pool_head: self.inner.clone(),
                            },
                        };
                    },
                    Err(value) => {
                        println!(" ;; LEND conflict: {:?} != {:?}, trying again", head, value);
                        head = value;
                    },
                }
            }
        }
    }
}
