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
        let head = self.inner.head.load(Ordering::Acquire);
        let mut maybe_entry_ptr = ptr::NonNull::new(head);

        let mut unhappy = false;

        loop {
            if let Some(entry_ptr) = maybe_entry_ptr {
                let next_head = match unsafe { entry_ptr.as_ref().next } {
                    None =>
                        ptr::null_mut(),
                    Some(non_null) =>
                        non_null.as_ptr(),
                };
                match self.inner.head.compare_exchange(entry_ptr.as_ptr(), next_head, Ordering::Release, Ordering::Relaxed) {
                    Ok(..) => {

                        if unhappy {
                            println!(
                                " ;; alloc_pool::pool::Pool::lend HAPPY path at last for entry_ptr = {:?}, next_head = {:?}",
                                entry_ptr.as_ptr(),
                                next_head,
                            );
                        }

                        let mut entry = unsafe { Box::from_raw(entry_ptr.as_ptr()) };
                        entry.next = None;
                        return Unique {
                            inner: Inner {
                                entry: Some(entry),
                                pool_head: self.inner.clone(),
                            },
                        };
                    },
                    Err(next_ptr) => {

                        println!(
                            " ;; alloc_pool::pool::Pool::lend unhappy path for entry_ptr = {:?}, next_ptr = {:?}, next_head = {:?}",
                            entry_ptr.as_ptr(),
                            next_ptr,
                            next_head,
                        );
                        unhappy = true;

                        maybe_entry_ptr = ptr::NonNull::new(next_ptr);
                    },
                }
            } else {
                return Unique { inner: Inner::new(make_value(), self.inner.clone()), };
            }
        }
    }
}
