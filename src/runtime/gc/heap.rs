/// A type tag for GC-managed objects, embedded in the object header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcTag {
    /// An integer value (unboxed in most paths, boxed here for uniformity).
    Int,
    /// A heap-allocated string.
    Str,
    /// A class instance (fields follow the header in memory).
    Object,
    /// A forwarding pointer set during copying collection.
    Forwarded,
}

/// The header that precedes every GC-managed allocation.
///
/// Layout (16 bytes on 64-bit):
/// ```text
/// [ tag: u64 | forward_or_size: u64 ]
/// ```
///
/// During a minor GC the `tag` field is set to `GcTag::Forwarded` and
/// `forward_or_size` stores the address of the new copy in old-space.
#[derive(Debug)]
#[repr(C)]
pub struct GcObject {
    /// Type tag identifying the kind of object.
    pub tag: GcTag,
    /// Either the size of the object's payload in bytes, or (after
    /// the object has been copied) the forwarding address.
    pub forward_or_size: usize,
}

impl GcObject {
    /// Create a fresh object header with the given tag and payload size.
    pub fn new(tag: GcTag, size: usize) -> Self {
        Self {
            tag,
            forward_or_size: size,
        }
    }
}

// ---------------------------------------------------------------------------
// Generation arenas
// ---------------------------------------------------------------------------

/// A simple bump-pointer arena used for one generation.
///
/// Memory is a `Vec<u8>` treated as a contiguous slab.  Allocation simply
/// advances a cursor; reclamation is done by the GC.
pub struct Arena {
    data: Vec<u8>,
    /// Cursor: next free byte index.
    cursor: usize,
    /// Maximum capacity.
    capacity: usize,
}

impl Arena {
    /// Create an arena with the given byte capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            data: vec![0u8; capacity],
            cursor: 0,
            capacity,
        }
    }

    /// How many bytes are currently used.
    pub fn used(&self) -> usize {
        self.cursor
    }

    /// How many bytes are free.
    pub fn free(&self) -> usize {
        self.capacity - self.cursor
    }

    /// Bump-allocate `size` bytes aligned to 8 bytes.
    /// Returns `Some(raw_ptr)` on success, `None` if the arena is full.
    ///
    /// # Safety
    /// The returned pointer is valid for `size` bytes within the arena's
    /// backing buffer for the lifetime of `self`.
    pub fn alloc(&mut self, size: usize) -> Option<*mut u8> {
        // Align up to 8 bytes
        let aligned = (size + 7) & !7;
        if self.cursor + aligned > self.capacity {
            return None;
        }
        let ptr = unsafe { self.data.as_mut_ptr().add(self.cursor) };
        self.cursor += aligned;
        Some(ptr)
    }

    /// Reset the arena (reclaim all memory, used after a collecting phase).
    pub fn reset(&mut self) {
        self.cursor = 0;
        // Zero out to catch dangling pointer bugs during development
        for b in &mut self.data {
            *b = 0;
        }
    }

    /// Raw pointer to the start of the arena's data.
    pub fn base_ptr(&self) -> *const u8 {
        self.data.as_ptr()
    }

    /// Check whether a raw pointer is within this arena.
    pub fn contains(&self, ptr: *const u8) -> bool {
        let base = self.data.as_ptr() as usize;
        let end = base + self.cursor;
        let p = ptr as usize;
        p >= base && p < end
    }
}

// ---------------------------------------------------------------------------
// GcHeap
// ---------------------------------------------------------------------------

/// Default young-generation size: 2 MiB.
const DEFAULT_YOUNG_CAPACITY: usize = 2 * 1024 * 1024;
/// Default old-generation size: 8 MiB.
const DEFAULT_OLD_CAPACITY: usize = 8 * 1024 * 1024;

/// Statistics collected by the GC.
#[derive(Debug, Default, Clone)]
pub struct GcStats {
    pub minor_collections: u64,
    pub major_collections: u64,
    pub total_allocated: usize,
    pub total_promoted: usize,
}

/// The Aura garbage-collected heap.
///
/// Objects are first allocated in the *young* generation.  After surviving
/// a minor GC they are promoted to the *old* generation.  The old generation
/// is collected via a full mark-and-sweep pass.
pub struct GcHeap {
    /// Young generation — from-space (allocation happens here).
    pub young: Arena,
    /// Young generation — to-space (used during copying collection).
    pub young_to: Arena,
    /// Old generation.
    pub old: Arena,
    /// Collected statistics.
    pub stats: GcStats,
}

impl GcHeap {
    /// Create a new heap with default generation sizes.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_YOUNG_CAPACITY, DEFAULT_OLD_CAPACITY)
    }

    /// Create a new heap with explicit generation capacities (useful for tests).
    pub fn with_capacity(young_cap: usize, old_cap: usize) -> Self {
        Self {
            young: Arena::new(young_cap),
            young_to: Arena::new(young_cap),
            old: Arena::new(old_cap),
            stats: GcStats::default(),
        }
    }

    /// Allocate an object of the given `tag` with `payload_bytes` of payload.
    ///
    /// The layout in memory is:
    /// ```text
    /// [ GcObject header | payload ... ]
    /// ```
    ///
    /// Returns a mutable pointer to the *header*.  The caller can then
    /// cast `ptr.add(size_of::<GcObject>())` to access the payload.
    ///
    /// Triggers a minor GC if the young generation is full.
    pub fn alloc(&mut self, tag: GcTag, payload_bytes: usize) -> *mut GcObject {
        let total = std::mem::size_of::<GcObject>() + payload_bytes;
        self.stats.total_allocated += total;

        if let Some(ptr) = self.young.alloc(total) {
            let obj = ptr as *mut GcObject;
            unsafe {
                obj.write(GcObject::new(tag, payload_bytes));
            }
            obj
        } else {
            // Young generation is full — this will be handled by sweep.rs.
            // Return a null pointer to signal that GC is needed.
            std::ptr::null_mut()
        }
    }

    /// Promote an object into the old generation.
    ///
    /// Copies `total_bytes` starting at `src` into the old arena and returns
    /// the new address.  Returns `None` if the old generation is full
    /// (triggering should invoke major_gc first).
    pub unsafe fn promote(&mut self, src: *const u8, total_bytes: usize) -> Option<*mut u8> {
        let dst = self.old.alloc(total_bytes)?;
        unsafe {
            std::ptr::copy_nonoverlapping(src, dst, total_bytes);
        }
        self.stats.total_promoted += total_bytes;
        Some(dst)
    }

    /// Swap from-space and to-space after a copying minor GC.
    pub fn swap_young_spaces(&mut self) {
        std::mem::swap(&mut self.young, &mut self.young_to);
        self.young.reset(); // now the old from-space becomes the new to-space
    }

    /// Print a human-readable summary of heap usage.
    pub fn report(&self) {
        println!(
            "GC Heap — young: {}/{} B, old: {}/{} B | minor: {}, major: {}",
            self.young.used(),
            self.young.capacity,
            self.old.used(),
            self.old.capacity,
            self.stats.minor_collections,
            self.stats.major_collections,
        );
    }
}

impl Default for GcHeap {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_alloc_basic() {
        let mut a = Arena::new(256);
        let p1 = a.alloc(16).expect("first alloc");
        let p2 = a.alloc(16).expect("second alloc");
        // Pointers should differ
        assert_ne!(p1, p2);
        assert_eq!(a.used(), 32);
    }

    #[test]
    fn arena_alloc_alignment() {
        let mut a = Arena::new(256);
        // Allocate 3 bytes — should be padded to 8
        let _ = a.alloc(3);
        assert_eq!(a.used(), 8);
        // Allocate 9 bytes — should be padded to 16
        let _ = a.alloc(9);
        assert_eq!(a.used(), 24);
    }

    #[test]
    fn arena_alloc_oom() {
        let mut a = Arena::new(16);
        let _ = a.alloc(16).expect("fits exactly");
        assert!(a.alloc(1).is_none(), "should fail when full");
    }

    #[test]
    fn arena_reset() {
        let mut a = Arena::new(64);
        let _ = a.alloc(32);
        assert_eq!(a.used(), 32);
        a.reset();
        assert_eq!(a.used(), 0);
    }

    #[test]
    fn heap_alloc_basic() {
        let mut heap = GcHeap::with_capacity(4096, 4096);
        let obj = heap.alloc(GcTag::Int, 8);
        assert!(!obj.is_null());
        unsafe {
            assert_eq!((*obj).tag, GcTag::Int);
            assert_eq!((*obj).forward_or_size, 8);
        }
    }

    #[test]
    fn heap_alloc_triggers_null_when_full() {
        // Very small young gen — only fits one object
        let header_size = std::mem::size_of::<GcObject>();
        let mut heap = GcHeap::with_capacity(header_size + 8, 4096);
        let first = heap.alloc(GcTag::Int, 0);
        assert!(!first.is_null());
        // Second alloc should fail (returns null)
        let second = heap.alloc(GcTag::Int, 0);
        assert!(second.is_null());
    }

    #[test]
    fn heap_promote() {
        let mut heap = GcHeap::with_capacity(4096, 4096);
        let obj = heap.alloc(GcTag::Object, 16);
        assert!(!obj.is_null());
        let header_size = std::mem::size_of::<GcObject>();
        let total = header_size + 16;
        let new_ptr = unsafe { heap.promote(obj as *const u8, total) };
        assert!(new_ptr.is_some());
        assert_eq!(heap.old.used(), (total + 7) & !7);
    }
}
