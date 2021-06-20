pub fn transmute_raw_buffer_mut<F, T>(buffer: &mut [F]) -> &mut [T] {
    use std::mem::size_of;
    use std::slice;
    unsafe {
        slice::from_raw_parts_mut(
            buffer.as_mut_ptr() as *mut T,
            buffer.len() * size_of::<F>() / size_of::<T>(),
        )
    }
}

pub fn transmute_raw_buffer<F, T>(buffer: &[F]) -> &[T] {
    use std::mem::size_of;
    use std::slice;

    unsafe {
        slice::from_raw_parts_mut(
            buffer.as_ptr() as *mut T,
            buffer.len() * size_of::<F>() / size_of::<T>(),
        )
    }
}
