use core::{
    marker::PhantomData,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr,
};

pub trait Memory<T>
where
    Self: Deref<Target = [T]> + DerefMut<Target = [T]>,
{
    type Err: std::fmt::Debug;

    fn len(&self) -> usize;
    fn len_mut(&mut self) -> &mut usize;
    fn reserve(&mut self, capacity: usize) -> Result<(), Self::Err>;
    fn shrink(&mut self, capacity: usize) -> Result<(), Self::Err>;
}

/// A memory-backed vector.
///
/// See document of std::vec::Vec for each methods.
pub struct MemVec<'a, T: Copy, A: 'a + Memory<T>> {
    mem: A,
    _marker: PhantomData<&'a T>,
}

impl<'a, T: Copy, A: 'a + Memory<T>> Deref for MemVec<'a, T, A> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        let len = self.mem.len();
        unsafe { self.mem.deref().get_unchecked(..len) }
    }
}

impl<'a, T: Copy, A: 'a + Memory<T>> DerefMut for MemVec<'a, T, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let len = self.mem.len();
        unsafe { self.mem.deref_mut().get_unchecked_mut(..len) }
    }
}

impl<'a, T: Copy, A: 'a + Memory<T>> From<A> for MemVec<'a, T, A> {
    fn from(mem: A) -> Self {
        Self {
            mem,
            _marker: PhantomData,
        }
    }
}

impl<'a, T: Copy, A: 'a + Memory<T>> MemVec<'a, T, A> {
    pub fn into_mem(self) -> A {
        self.mem
    }
    pub fn as_mem(&self) -> &A {
        &self.mem
    }
    pub fn as_mem_mut(&mut self) -> &mut A {
        &mut self.mem
    }
}

// std::vec::Vec methods
impl<'a, T: Copy, A: 'a + Memory<T>> MemVec<'a, T, A> {
    #[inline]
    pub fn capacity(&self) -> usize {
        self.mem.len()
    }

    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.try_reserve(additional).expect("reserve failed");
    }

    pub fn try_reserve(&mut self, additional: usize) -> Result<(), A::Err> {
        let len = self.len();
        if self.needs_to_grow(len, additional) {
            self.grow_amortized(len, additional)
        } else {
            Ok(())
        }
    }

    pub fn reserve_exact(&mut self, additional: usize) {
        self.try_reserve_exact(additional).expect("reserve failed");
    }

    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), A::Err> {
        let len = self.len();
        if self.needs_to_grow(len, additional) {
            self.grow_exact(len, additional)
        } else {
            Ok(())
        }
    }

    pub fn shrink_to_fit(&mut self) {
        // The capacity is never less than the length, and there's nothing to do when
        // they are equal, so we can avoid the panic case in `RawVec::shrink_to_fit`
        // by only calling it with a greater capacity.
        if self.capacity() > self.len() {
            self.mem.shrink(self.len()).expect("shrink failed");
        }
    }

    pub fn shrink_to(&mut self, min_capacity: usize) {
        if self.capacity() > min_capacity {
            self.mem
                .shrink(std::cmp::max(self.len(), min_capacity))
                .expect("shrink failed");
        }
    }

    pub fn truncate(&mut self, len: usize) {
        if len > self.len() {
            return;
        }
        unsafe {
            // Note: It's intentional that this is `>` and not `>=`.
            //       Changing it to `>=` has negative performance
            //       implications in some cases. See #78884 for more.

            let remaining_len = self.mem.len() - len;
            let s = ptr::slice_from_raw_parts_mut(self.as_mut_ptr().add(len), remaining_len);
            *self.mem.len_mut() = len;
            ptr::drop_in_place(s);
        }
    }

    pub fn as_slice(&self) -> &[T] {
        self.mem.deref()
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.mem.deref_mut()
    }

    #[inline]
    pub fn as_ptr(&self) -> *const T {
        self.mem.deref().as_ptr()
    }

    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut T {
        self.mem.deref_mut().as_mut_ptr()
    }

    pub unsafe fn set_len(&mut self, len: usize) {
        #[cold]
        #[inline(never)]
        fn assert_failed(len: usize, cap: usize) -> ! {
            panic!("`set_len` len (is {len}) should be <= cap (is {cap})");
        }
        let cap = self.mem.len();
        if !(len <= cap) {
            assert_failed(len, self.capacity());
        }
        *self.mem.len_mut() = len;
    }

    #[inline]
    pub fn swap_remove(&mut self, index: usize) -> T {
        #[cold]
        #[inline(never)]
        fn assert_failed(index: usize, len: usize) -> ! {
            panic!(
                "swap_remove index (is {}) should be < len (is {})",
                index, len
            );
        }

        let len = self.len();
        if index >= len {
            assert_failed(index, len);
        }
        unsafe {
            // We replace self[index] with the last element. Note that if the
            // bounds check above succeeds there must be a last element (which
            // can be self[index] itself).
            let value = ptr::read(self.as_ptr().add(index));
            let base_ptr = self.as_mut_ptr();
            ptr::copy(base_ptr.add(len - 1), base_ptr.add(index), 1);
            self.set_len(len - 1);
            value
        }
    }

    pub fn insert(&mut self, index: usize, element: T) {
        #[cold]
        #[inline(never)]
        fn assert_failed(index: usize, len: usize) -> ! {
            panic!(
                "insertion index (is {}) should be <= len (is {})",
                index, len
            );
        }

        let len = self.len();
        if index > len {
            assert_failed(index, len);
        }

        // space for the new element
        if len == self.capacity() {
            self.reserve(1);
        }

        unsafe {
            // infallible
            // The spot to put the new value
            {
                let p = self.as_mut_ptr().add(index);
                // Shift everything over to make space. (Duplicating the
                // `index`th element into two consecutive places.)
                ptr::copy(p, p.offset(1), len - index);
                // Write it in, overwriting the first copy of the `index`th
                // element.
                ptr::write(p, element);
            }
            self.set_len(len + 1);
        }
    }

    #[track_caller]
    pub fn remove(&mut self, index: usize) -> T {
        #[cold]
        #[inline(never)]
        #[track_caller]
        fn assert_failed(index: usize, len: usize) -> ! {
            panic!("removal index (is {}) should be < len (is {})", index, len);
        }

        let len = self.len();
        if index >= len {
            assert_failed(index, len);
        }
        unsafe {
            // infallible
            let ret;
            {
                // the place we are taking from.
                let ptr = self.as_mut_ptr().add(index);
                // copy it out, unsafely having a copy of the value on
                // the stack and in the vector at the same time.
                ret = ptr::read(ptr);

                // Shift everything down to fill in that spot.
                ptr::copy(ptr.offset(1), ptr, len - index - 1);
            }
            self.set_len(len - 1);
            ret
        }
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.retain_mut(|elem| f(elem));
    }

    pub fn retain_mut<F>(&mut self, mut f: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        let original_len = self.len();
        // Avoid double drop if the drop guard is not executed,
        // since we may make some holes during the process.
        unsafe { self.set_len(0) };

        // Vec: [Kept, Kept, Hole, Hole, Hole, Hole, Unchecked, Unchecked]
        //      |<-              processed len   ->| ^- next to check
        //                  |<-  deleted cnt     ->|
        //      |<-              original_len                          ->|
        // Kept: Elements which predicate returns true on.
        // Hole: Moved or dropped element slot.
        // Unchecked: Unchecked valid elements.
        //
        // This drop guard will be invoked when predicate or `drop` of element panicked.
        // It shifts unchecked elements to cover holes and `set_len` to the correct length.
        // In cases when predicate and `drop` never panick, it will be optimized out.
        struct BackshiftOnDrop<'a, 'v, T: Copy, A: Memory<T>> {
            v: &'a mut MemVec<'v, T, A>,
            processed_len: usize,
            deleted_cnt: usize,
            original_len: usize,
        }

        impl<T: Copy, A: Memory<T>> Drop for BackshiftOnDrop<'_, '_, T, A> {
            fn drop(&mut self) {
                if self.deleted_cnt > 0 {
                    // SAFETY: Trailing unchecked items must be valid since we never touch them.
                    unsafe {
                        ptr::copy(
                            self.v.as_ptr().add(self.processed_len),
                            self.v
                                .as_mut_ptr()
                                .add(self.processed_len - self.deleted_cnt),
                            self.original_len - self.processed_len,
                        );
                    }
                }
                // SAFETY: After filling holes, all items are in contiguous memory.
                unsafe {
                    self.v.set_len(self.original_len - self.deleted_cnt);
                }
            }
        }

        let mut g = BackshiftOnDrop {
            v: self,
            processed_len: 0,
            deleted_cnt: 0,
            original_len,
        };

        fn process_loop<F, T: Copy, A: Memory<T>, const DELETED: bool>(
            original_len: usize,
            f: &mut F,
            g: &mut BackshiftOnDrop<'_, '_, T, A>,
        ) where
            F: FnMut(&mut T) -> bool,
        {
            while g.processed_len != original_len {
                // SAFETY: Unchecked element must be valid.
                let cur = unsafe { &mut *g.v.as_mut_ptr().add(g.processed_len) };
                if !f(cur) {
                    // Advance early to avoid double drop if `drop_in_place` panicked.
                    g.processed_len += 1;
                    g.deleted_cnt += 1;
                    // SAFETY: We never touch this element again after dropped.
                    unsafe { ptr::drop_in_place(cur) };
                    // We already advanced the counter.
                    if DELETED {
                        continue;
                    } else {
                        break;
                    }
                }
                if DELETED {
                    // SAFETY: `deleted_cnt` > 0, so the hole slot must not overlap with current element.
                    // We use copy for move, and never touch this element again.
                    unsafe {
                        let hole_slot = g.v.as_mut_ptr().add(g.processed_len - g.deleted_cnt);
                        ptr::copy_nonoverlapping(cur, hole_slot, 1);
                    }
                }
                g.processed_len += 1;
            }
        }

        // Stage 1: Nothing was deleted.
        process_loop::<F, T, A, false>(original_len, &mut f, &mut g);

        // Stage 2: Some elements were deleted.
        process_loop::<F, T, A, true>(original_len, &mut f, &mut g);

        // All item are processed. This can be optimized to `set_len` by LLVM.
        drop(g);
    }

    #[inline]
    pub fn dedup_by_key<F, K>(&mut self, mut key: F)
    where
        F: FnMut(&mut T) -> K,
        K: PartialEq,
    {
        self.dedup_by(|a, b| key(a) == key(b))
    }

    pub fn dedup_by<F>(&mut self, mut same_bucket: F)
    where
        F: FnMut(&mut T, &mut T) -> bool,
    {
        let len = self.len();
        if len <= 1 {
            return;
        }

        /* INVARIANT: vec.len() > read >= write > write-1 >= 0 */
        struct FillGapOnDrop<'a, 'b, T: Copy, A: Memory<T>> {
            /* Offset of the element we want to check if it is duplicate */
            read: usize,

            /* Offset of the place where we want to place the non-duplicate
             * when we find it. */
            write: usize,

            /* The Vec that would need correction if `same_bucket` panicked */
            vec: &'a mut MemVec<'b, T, A>,
        }

        impl<'a, 'b, T: Copy, A: Memory<T>> Drop for FillGapOnDrop<'a, 'b, T, A> {
            fn drop(&mut self) {
                /* This code gets executed when `same_bucket` panics */
                /* SAFETY: invariant guarantees that `read - write`
                 * and `len - read` never overflow and that the copy is always
                 * in-bounds. */
                unsafe {
                    let ptr = self.vec.as_mut_ptr();
                    let len = self.vec.len();

                    /* How many items were left when `same_bucket` panicked.
                     * Basically vec[read..].len() */
                    let items_left = len.wrapping_sub(self.read);

                    /* Pointer to first item in vec[write..write+items_left] slice */
                    let dropped_ptr = ptr.add(self.write);
                    /* Pointer to first item in vec[read..] slice */
                    let valid_ptr = ptr.add(self.read);

                    /* Copy `vec[read..]` to `vec[write..write+items_left]`.
                     * The slices can overlap, so `copy_nonoverlapping` cannot be used */
                    ptr::copy(valid_ptr, dropped_ptr, items_left);

                    /* How many items have been already dropped
                     * Basically vec[read..write].len() */
                    let dropped = self.read.wrapping_sub(self.write);

                    self.vec.set_len(len - dropped);
                }
            }
        }

        let mut gap = FillGapOnDrop {
            read: 1,
            write: 1,
            vec: self,
        };
        let ptr = gap.vec.as_mut_ptr();

        /* Drop items while going through Vec, it should be more efficient than
         * doing slice partition_dedup + truncate */
        /* SAFETY: Because of the invariant, read_ptr, prev_ptr and write_ptr
         * are always in-bounds and read_ptr never aliases prev_ptr */
        unsafe {
            while gap.read < len {
                let read_ptr = ptr.add(gap.read);
                let prev_ptr = ptr.add(gap.write.wrapping_sub(1));

                if same_bucket(&mut *read_ptr, &mut *prev_ptr) {
                    // Increase `gap.read` now since the drop may panic.
                    gap.read += 1;
                    /* We have found duplicate, drop it in-place */
                    ptr::drop_in_place(read_ptr);
                } else {
                    let write_ptr = ptr.add(gap.write);

                    /* Because `read_ptr` can be equal to `write_ptr`, we either
                     * have to use `copy` or conditional `copy_nonoverlapping`.
                     * Looks like the first option is faster. */
                    ptr::copy(read_ptr, write_ptr, 1);

                    /* We have filled that place, so go further */
                    gap.write += 1;
                    gap.read += 1;
                }
            }

            /* Technically we could let `gap` clean up with its Drop, but
             * when `same_bucket` is guaranteed to not panic, this bloats a little
             * the codegen, so we just do it manually */
            gap.vec.set_len(gap.write);
            std::mem::forget(gap);
        }
    }

    #[inline]
    pub fn push(&mut self, value: T) {
        if self.len() == self.capacity() {
            self.reserve_for_push(self.len()).unwrap();
        }
        unsafe {
            let end = self.as_mut_ptr().add(self.len());
            ptr::write(end, value);
            *self.mem.len_mut() += 1;
        }
    }

    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.len() == 0 {
            None
        } else {
            unsafe {
                *self.mem.len_mut() -= 1;
                Some(ptr::read(self.ptr().add(self.len())))
            }
        }
    }

    // drain

    #[inline]
    pub fn clear(&mut self) {
        self.truncate(0)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.mem.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[cfg(not(no_global_oom_handling))]
    pub fn resize_with<F>(&mut self, new_len: usize, f: F)
    where
        F: FnMut() -> T,
    {
        let len = self.len();
        if new_len > len {
            self.extend_with(new_len - len, ExtendFunc(f));
        } else {
            self.truncate(new_len);
        }
    }

    #[inline]
    pub fn spare_capacity_mut(&mut self) -> &mut [MaybeUninit<T>] {
        // Note:
        // This method is not implemented in terms of `split_at_spare_mut`,
        // to prevent invalidation of pointers to the buffer.
        unsafe {
            std::slice::from_raw_parts_mut(
                self.as_mut_ptr().add(self.len()) as *mut MaybeUninit<T>,
                self.capacity() - self.len(),
            )
        }
    }

    #[inline]
    pub fn ptr(&self) -> *mut T {
        self.mem.deref() as *const _ as *mut T
    }
}

trait ExtendWith<T> {
    fn next(&mut self) -> T;
    fn last(self) -> T;
}

struct ExtendElement<T>(T);
impl<T: Clone> ExtendWith<T> for ExtendElement<T> {
    fn next(&mut self) -> T {
        self.0.clone()
    }
    fn last(self) -> T {
        self.0
    }
}

struct ExtendFunc<F>(F);
impl<T, F: FnMut() -> T> ExtendWith<T> for ExtendFunc<F> {
    fn next(&mut self) -> T {
        (self.0)()
    }
    fn last(mut self) -> T {
        (self.0)()
    }
}

#[cfg(not(no_global_oom_handling))]
fn capacity_overflow() -> usize {
    panic!("capacity overflow");
}

impl<'a, T: Copy, A: 'a + Memory<T>> MemVec<'a, T, A> {
    // pub(crate) const MIN_NON_ZERO_CAP: usize = if std::mem::size_of::<T>() == 1 {
    //     8
    // } else if std::mem::size_of::<T>() <= 1024 {
    //     4
    // } else {
    //     1
    // };

    /// Returns if the buffer needs to grow to fulfill the needed extra capacity.
    /// Mainly used to make inlining reserve-calls possible without inlining `grow`.
    fn needs_to_grow(&self, len: usize, additional: usize) -> bool {
        additional > self.capacity().wrapping_sub(len)
    }

    pub fn reserve_for_push(&mut self, len: usize) -> Result<(), A::Err> {
        self.grow_amortized(len, 1)
    }

    // fn set_ptr_and_cap(&mut self, ptr: NonNull<[u8]>, cap: usize) {
    //     // Allocators currently return a `NonNull<[u8]>` whose length matches
    //     // the size requested. If that ever changes, the capacity here should
    //     // change to `ptr.len() / mem::size_of::<T>()`.
    //     self.ptr = unsafe { Unique::new_unchecked(ptr.cast().as_ptr()) };
    //     self.cap = cap;
    // }

    // This method is usually instantiated many times. So we want it to be as
    // small as possible, to improve compile times. But we also want as much of
    // its contents to be statically computable as possible, to make the
    // generated code run faster. Therefore, this method is carefully written
    // so that all of the code that depends on `T` is within it, while as much
    // of the code that doesn't depend on `T` as possible is in functions that
    // are non-generic over `T`.
    fn grow_amortized(&mut self, len: usize, additional: usize) -> Result<(), A::Err> {
        // This is ensured by the calling contexts.
        debug_assert!(additional > 0);

        // if std::mem::size_of::<T>() == 0 {
        //     // Since we return a capacity of `usize::MAX` when `elem_size` is
        //     // 0, getting to here necessarily means the `RawVec` is overfull.
        //     return Err(CapacityOverflow.into());
        // }

        // Nothing we can really do about these checks, sadly.
        let required_cap = len
            .checked_add(additional)
            .unwrap_or_else(capacity_overflow);

        // This guarantees exponential growth. The doubling cannot overflow
        // because `cap <= isize::MAX` and the type of `cap` is `usize`.
        let cap = std::cmp::max(self.capacity() * 2, required_cap);
        self.mem.reserve(cap)
    }

    // The constraints on this method are much the same as those on
    // `grow_amortized`, but this method is usually instantiated less often so
    // it's less critical.
    fn grow_exact(&mut self, len: usize, additional: usize) -> Result<(), A::Err> {
        // if std::mem::size_of::<T>() == 0 {
        //     // Since we return a capacity of `usize::MAX` when the type size is
        //     // 0, getting to here necessarily means the `RawVec` is overfull.
        //     return Err(CapacityOverflow.into());
        // }

        let cap = len
            .checked_add(additional)
            .unwrap_or_else(capacity_overflow);
        self.mem.reserve(cap)
    }

    // fn shrink(&mut self, cap: usize) -> Result<(), TryReserveError> {
    //     assert!(cap <= self.capacity(), "Tried to shrink to a larger capacity");

    //     let (ptr, layout) = if let Some(mem) = self.current_memory() { mem } else { return Ok(()) };

    //     let ptr = unsafe {
    //         // `Layout::array` cannot overflow here because it would have
    //         // overflowed earlier when capacity was larger.
    //         let new_layout = Layout::array::<T>(cap).unwrap_unchecked();
    //         self.alloc
    //             .shrink(ptr, layout, new_layout)
    //             .map_err(|_| AllocError { layout: new_layout, non_exhaustive: () })?
    //     };
    //     self.set_ptr_and_cap(ptr, cap);
    //     Ok(())
    // }

    /// Extend the vector by `n` values, using the given generator.
    fn extend_with<E: ExtendWith<T>>(&mut self, n: usize, mut value: E) {
        self.reserve(n);

        unsafe {
            let mut ptr = self.as_mut_ptr().add(self.len());
            // Write all elements except the last one
            for _ in 1..n {
                ptr::write(ptr, value.next());
                ptr = ptr.offset(1);
                // Increment the length in every step in case next() panics
                *self.mem.len_mut() += 1;
            }

            if n > 0 {
                // We can write the last element directly without cloning needlessly
                std::ptr::write(ptr, value.last());
                *self.mem.len_mut() += 1;
            }

            // len set by scope guard
        }
    }
}
