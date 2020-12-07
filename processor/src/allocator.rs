use isnt::std_1::primitive::IsntMutPtrExt;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    ptr,
};

/// Very space-efficient allocator for linux
///
/// All allocations above page size are handled by mmap. All other allocations use malloc.
/// This is slightly slower for large allocations but reduces the resident set size at
/// rest from >60M to 20M.
///
/// The slowdown for large allocations is acceptable because we make very few of them.
struct Allocator;

#[global_allocator]
static GLOBAL: Allocator = Allocator;

const PAGE_SIZE: usize = 4096;

unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() >= PAGE_SIZE {
            mmap(layout.size())
        } else {
            System.alloc(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() >= PAGE_SIZE {
            libc::munmap(ptr as *mut _, layout.size());
        } else {
            System.dealloc(ptr, layout)
        }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if layout.size() >= PAGE_SIZE {
            mmap(layout.size())
        } else {
            System.alloc_zeroed(layout)
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        #[allow(clippy::collapsible_if)]
        if layout.size() >= PAGE_SIZE {
            if new_size >= PAGE_SIZE {
                let new = libc::mremap(
                    ptr as *mut _,
                    layout.size(),
                    new_size,
                    libc::MREMAP_MAYMOVE,
                );
                if new == libc::MAP_FAILED {
                    ptr::null_mut()
                } else {
                    new as *mut _
                }
            } else {
                let new = System
                    .alloc(Layout::from_size_align_unchecked(new_size, layout.align()));
                if new.is_not_null() {
                    ptr::copy_nonoverlapping(ptr, new, new_size);
                    libc::munmap(ptr as _, layout.size());
                }
                new
            }
        } else {
            if new_size >= PAGE_SIZE {
                let new = mmap(new_size);
                if new.is_not_null() {
                    ptr::copy_nonoverlapping(ptr, new, layout.size());
                    System.dealloc(ptr, layout);
                }
                new
            } else {
                System.realloc(ptr, layout, new_size)
            }
        }
    }
}

unsafe fn mmap(size: usize) -> *mut u8 {
    let ptr = libc::mmap(
        ptr::null_mut(),
        size,
        libc::PROT_READ | libc::PROT_WRITE,
        libc::MAP_ANONYMOUS | libc::MAP_PRIVATE,
        -1,
        0,
    );
    if ptr == libc::MAP_FAILED {
        ptr::null_mut()
    } else {
        ptr as *mut u8
    }
}
