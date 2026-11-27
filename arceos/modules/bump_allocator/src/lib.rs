#![no_std]

use allocator::{AllocError, AllocResult, BaseAllocator, ByteAllocator, PageAllocator};

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const SIZE: usize> {
    start: usize,
    end: usize,
    b_pos: usize,
    p_pos: usize,
    b_count: usize,
}

impl<const SIZE: usize> EarlyAllocator<SIZE> {
    pub const fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            b_pos: 0,
            p_pos: 0,
            b_count: 0,
        }
    }
}

impl<const SIZE: usize> BaseAllocator for EarlyAllocator<SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        assert!(SIZE.is_power_of_two());
        let end = align_down(start.saturating_add(size), SIZE);
        let start = align_up(start, SIZE);
        assert!(start <= end);
        self.start = start;
        self.end = end;
        self.b_pos = start;
        self.p_pos = end;
        self.b_count = 0;
    }

    fn add_memory(&mut self, _start: usize, _size: usize) -> AllocResult {
        Err(AllocError::MemoryOverlap)
    }
}

impl<const SIZE: usize> ByteAllocator for EarlyAllocator<SIZE> {
    fn alloc(
        &mut self,
        layout: core::alloc::Layout,
    ) -> AllocResult<core::ptr::NonNull<u8>> {
        if layout.size() == 0 {
            return Ok(core::ptr::NonNull::dangling());
        }

        let start = align_up(self.b_pos, layout.align());
        let end = start
            .checked_add(layout.size())
            .ok_or(AllocError::InvalidParam)?;
        if end > self.p_pos {
            return Err(AllocError::NoMemory);
        }

        self.b_pos = end;
        self.b_count += 1;
        core::ptr::NonNull::new(start as *mut u8).ok_or(AllocError::InvalidParam)
    }

    fn dealloc(&mut self, _pos: core::ptr::NonNull<u8>, _layout: core::alloc::Layout) {
        if self.b_count > 0 {
            self.b_count -= 1;
            // The byte area is a monotonic bump region. Reclaim it only when
            // every byte allocation has been released.
            if self.b_count == 0 {
                self.b_pos = self.start;
            }
        }
    }

    fn total_bytes(&self) -> usize {
        self.end - self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos - self.start
    }

    fn available_bytes(&self) -> usize {
        self.p_pos - self.b_pos
    }
}

impl<const SIZE: usize> PageAllocator for EarlyAllocator<SIZE> {
    const PAGE_SIZE: usize = SIZE;

    fn alloc_pages(
        &mut self,
        num_pages: usize,
        align_pow2: usize,
    ) -> AllocResult<usize> {
        if num_pages == 0 || align_pow2 < SIZE || align_pow2 % SIZE != 0 {
            return Err(AllocError::InvalidParam);
        }
        if !(align_pow2 / SIZE).is_power_of_two() {
            return Err(AllocError::InvalidParam);
        }

        let size = num_pages.checked_mul(SIZE).ok_or(AllocError::InvalidParam)?;
        let p_end = align_down(self.p_pos, align_pow2);
        let start = p_end.checked_sub(size).ok_or(AllocError::NoMemory)?;
        if start < self.b_pos {
            return Err(AllocError::NoMemory);
        }

        self.p_pos = start;
        Ok(start)
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {
        // The early allocator never returns page allocations because bytes
        // may still be growing toward the same boundary.
    }

    fn total_pages(&self) -> usize {
        (self.end - self.start) / SIZE
    }

    fn used_pages(&self) -> usize {
        (self.end - self.p_pos) / SIZE
    }

    fn available_pages(&self) -> usize {
        (self.p_pos - self.b_pos) / SIZE
    }
}

const fn align_up(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

const fn align_down(value: usize, align: usize) -> usize {
    value & !(align - 1)
}
