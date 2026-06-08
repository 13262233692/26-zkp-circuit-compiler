use std::alloc::{alloc, dealloc, Layout};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::ptr;
use std::sync::atomic::{fence, Ordering};

#[cfg(target_os = "windows")]
extern "system" {
    fn VirtualLock(lpAddress: *mut std::ffi::c_void, dwSize: usize) -> i32;
    fn VirtualUnlock(lpAddress: *mut std::ffi::c_void, dwSize: usize) -> i32;
}

#[cfg(unix)]
extern "C" {
    fn mlock(addr: *const std::ffi::c_void, len: usize) -> i32;
    fn munlock(addr: *const std::ffi::c_void, len: usize) -> i32;
}

fn lock_memory(ptr: *mut u8, len: usize) -> bool {
    if len == 0 || ptr.is_null() {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        unsafe { VirtualLock(ptr as *mut std::ffi::c_void, len) != 0 }
    }
    #[cfg(unix)]
    {
        unsafe { mlock(ptr as *const std::ffi::c_void, len) == 0 }
    }
    #[cfg(not(any(target_os = "windows", unix)))]
    {
        let _ = (ptr, len);
        false
    }
}

fn unlock_memory(ptr: *mut u8, len: usize) -> bool {
    if len == 0 || ptr.is_null() {
        return false;
    }
    #[cfg(target_os = "windows")]
    {
        unsafe { VirtualUnlock(ptr as *mut std::ffi::c_void, len) != 0 }
    }
    #[cfg(unix)]
    {
        unsafe { munlock(ptr as *const std::ffi::c_void, len) == 0 }
    }
    #[cfg(not(any(target_os = "windows", unix)))]
    {
        let _ = (ptr, len);
        false
    }
}

pub fn volatile_zero(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    unsafe {
        for i in 0..len {
            ptr::write_volatile(ptr.add(i), 0u8);
        }
    }
    fence(Ordering::SeqCst);
}

pub struct SecureBox<T> {
    ptr: *mut T,
    layout: Layout,
    locked: bool,
}

impl<T> SecureBox<T> {
    pub fn new(value: T) -> Result<Self, String> {
        let layout = Layout::new::<T>();
        if layout.size() == 0 {
            return Err("SecureBox: zero-sized type".to_string());
        }
        let ptr = unsafe { alloc(layout) as *mut T };
        if ptr.is_null() {
            return Err("SecureBox: allocation failed".to_string());
        }
        unsafe {
            ptr::write(ptr, value);
        }
        let locked = lock_memory(ptr as *mut u8, layout.size());
        Ok(SecureBox {
            ptr,
            layout,
            locked,
        })
    }

    pub fn locked(&self) -> bool {
        self.locked
    }

    pub fn destroy(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        volatile_zero(self.ptr as *mut u8, self.layout.size());
        if self.locked {
            unlock_memory(self.ptr as *mut u8, self.layout.size());
            self.locked = false;
        }
        unsafe {
            ptr::read(self.ptr);
            dealloc(self.ptr as *mut u8, self.layout);
        }
        self.ptr = ptr::null_mut();
    }
}

impl<T> Deref for SecureBox<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl<T> DerefMut for SecureBox<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}

impl<T> Drop for SecureBox<T> {
    fn drop(&mut self) {
        self.destroy();
    }
}

impl<T: fmt::Debug> fmt::Debug for SecureBox<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecureBox<{}> {{ locked: {}, [REDACTED] }}", std::any::type_name::<T>(), self.locked)
    }
}

pub struct SecureVec {
    ptr: *mut u8,
    len: usize,
    capacity: usize,
    locked: bool,
}

impl SecureVec {
    pub fn with_capacity(capacity: usize) -> Result<Self, String> {
        if capacity == 0 {
            return Err("SecureVec: zero capacity".to_string());
        }
        let layout = Layout::from_size_align(capacity, 8).map_err(|e| e.to_string())?;
        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return Err("SecureVec: allocation failed".to_string());
        }
        unsafe {
            ptr::write_bytes(ptr, 0, capacity);
        }
        let locked = lock_memory(ptr, capacity);
        Ok(SecureVec {
            ptr,
            len: 0,
            capacity,
            locked,
        })
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        let mut vec = Self::with_capacity(data.len())?;
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), vec.ptr, data.len());
        }
        vec.len = data.len();
        Ok(vec)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[u8] {
        if self.ptr.is_null() || self.len == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    pub fn extend_from_slice(&mut self, data: &[u8]) -> Result<(), String> {
        let new_len = self.len + data.len();
        if new_len > self.capacity {
            return Err("SecureVec: would exceed capacity".to_string());
        }
        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.ptr.add(self.len), data.len());
        }
        self.len = new_len;
        Ok(())
    }

    pub fn locked(&self) -> bool {
        self.locked
    }

    pub fn destroy(&mut self) {
        if self.ptr.is_null() {
            return;
        }
        volatile_zero(self.ptr, self.capacity);
        if self.locked {
            unlock_memory(self.ptr, self.capacity);
            self.locked = false;
        }
        let layout = Layout::from_size_align(self.capacity, 8).unwrap();
        unsafe {
            dealloc(self.ptr, layout);
        }
        self.ptr = ptr::null_mut();
        self.len = 0;
        self.capacity = 0;
    }
}

impl Drop for SecureVec {
    fn drop(&mut self) {
        self.destroy();
    }
}

impl fmt::Debug for SecureVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecureVec {{ len: {}, locked: {}, [REDACTED] }}", self.len, self.locked)
    }
}
