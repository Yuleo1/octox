// Gets the bytes of the value.
//
// as_bytes() provides access to the bytes of the value as an immutable
// byte slice.
//
// # Safety: 
// If the memory layout of T is fixed
pub unsafe fn as_bytes<T: ?Sized>(refs: &T) -> &[u8] {
    let len = core::mem::size_of_val(refs);
    core::slice::from_raw_parts(refs as *const T as *const u8, len)
}

// Gets the bytes of the value mutably.
//
// as_bytes_mut() provides access to the bytes of the value as a mutable
// byte slice.
//
// # Safety: 
// If the memory layout of T is fixed
pub unsafe fn as_bytes_mut<T: ?Sized>(refs: &mut T) -> &mut [u8] {
    let len = core::mem::size_of_val(refs);
    core::slice::from_raw_parts_mut(refs as *mut T as *mut u8, len)
}