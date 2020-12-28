pub fn transmute_raw_buffer_mut<T>(buffer: &mut [f32]) -> &mut [T] {
    use std::slice;
    use std::mem::size_of;
    unsafe {
        slice::from_raw_parts_mut(
            buffer.as_mut_ptr() as *mut T,
            buffer.len() * size_of::<f32>() / size_of::<T>()
        )
    }
}

pub fn transmute_raw_buffer<T>(buffer: & [f32]) -> &[T] {
    use std::slice;
    use std::mem::size_of;

    unsafe {
        slice::from_raw_parts_mut(
            buffer.as_ptr() as *mut T,
            buffer.len() * size_of::<f32>() / size_of::<T>()
        )
    }
}
