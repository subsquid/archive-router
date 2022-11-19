//
// Attempt to develop analog of
// https://docs.oracle.com/javase/8/docs/api/java/util/concurrent/atomic/AtomicReference.html
//
use std::mem::forget;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};

pub struct Atom<T> {
    data: AtomicPtr<T>
}

unsafe impl<T: Send> Send for Atom<T> {}
unsafe impl<T: Send> Sync for Atom<T> {}

impl <T> Atom<T> {
    pub fn new(val: Arc<T>) -> Self {
        let data_ptr = Arc::into_raw(val);
        let data = AtomicPtr::new(data_ptr as *mut T);
        Atom { data }
    }

    pub fn get(&self) -> Arc<T> {
        let data_ptr = self.data.load(Ordering::SeqCst);
        let arc = unsafe {
            Arc::from_raw(data_ptr)
        };
        let new_arc = arc.clone();
        forget(arc);
        new_arc
    }

    pub fn set(&self, val: Arc<T>) {
        let data_ptr = Arc::into_raw(val);
        let old_data_ptr = self.data.swap(data_ptr as *mut T, Ordering::SeqCst);
        unsafe {
            drop_arc(old_data_ptr)
        }
    }

    pub fn update<F: FnMut(&T) -> Option<Arc<T>>>(&self, mut f: F) {
        if let Ok(old_data_ptr) = self.data.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |data_ptr| {
            unsafe {
                f(data_ptr.as_ref().unwrap())
            }.map(|v| {
                Arc::into_raw(v) as *mut T
            })
        }) {
            unsafe {
                drop_arc(old_data_ptr)
            }
        }
    }
}


impl <T> Drop for Atom<T> {
    fn drop(&mut self) {
        let data_ptr = self.data.load(Ordering::SeqCst);
        unsafe {
            drop_arc(data_ptr)
        }
    }
}


unsafe fn drop_arc<T>(ptr: *mut T) {
    let _ = Arc::from_raw(ptr);
}


impl <T> Clone for Atom<T> {
    fn clone(&self) -> Self {
        Atom::new(self.get())
    }
}
