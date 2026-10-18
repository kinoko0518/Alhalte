//! WebAssembly ABI export implementation for `alhalte-saignant`.

#![allow(unsafe_code)]

use crate::resolver::NoopCharacterResolver;
use crate::{full_css, render_generast};
use generast::Generast;
use tin::Tin;

/// Current Alhalte Saignant ABI version (`1`).
#[unsafe(no_mangle)]
pub extern "C" fn alhalte_abi_version() -> i32 {
    1
}

/// Allocates memory buffer of length `len` for host input.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    match std::alloc::Layout::from_size_align(len, 1) {
        Ok(layout) => unsafe { std::alloc::alloc(layout) },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Deallocates memory buffer previously allocated by this module.
///
/// # Safety
///
/// `ptr` must have been returned by [`alloc`] and `len` must match the allocated size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    if let Ok(layout) = std::alloc::Layout::from_size_align(len, 1) {
        unsafe {
            std::alloc::dealloc(ptr, layout);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn custom_getrandom(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

#[cfg(target_arch = "wasm32")]
getrandom::register_custom_getrandom!(custom_getrandom);

/// Packs pointer and length into a 64-bit integer.
#[cfg(target_arch = "wasm32")]
pub fn pack_ptr_len(ptr: *mut u8, len: usize) -> u64 {
    ((ptr as usize as u64) << 32) | (len as u64 & 0xffff_ffff)
}

/// Unpacks pointer and length from a 64-bit integer.
#[cfg(target_arch = "wasm32")]
pub fn unpack_ptr_len(packed: u64) -> (*mut u8, usize) {
    let ptr = (packed >> 32) as usize as *mut u8;
    let len = (packed & 0xffff_ffff) as usize;
    (ptr, len)
}

/// Packs pointer and length on 64-bit platforms for testing.
#[cfg(not(target_arch = "wasm32"))]
pub fn pack_ptr_len(ptr: *mut u8, len: usize) -> u64 {
    Box::into_raw(Box::new((ptr, len))) as usize as u64
}

/// Unpacks pointer and length on 64-bit platforms for testing.
///
/// # Safety
///
/// `packed` must be a valid pointer created by [`pack_ptr_len`].
#[cfg(not(target_arch = "wasm32"))]
pub unsafe fn unpack_ptr_len(packed: u64) -> (*mut u8, usize) {
    let boxed = unsafe { Box::from_raw(packed as usize as *mut (*mut u8, usize)) };
    *boxed
}

fn allocate_and_pack_bytes(bytes: Vec<u8>) -> u64 {
    let len = bytes.len();
    if len == 0 {
        return 0;
    }
    let ptr = alloc(len);
    if ptr.is_null() {
        return 0;
    }
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, len);
    }
    pack_ptr_len(ptr, len)
}

/// Renders a Generast Tin into an HTML UTF-8 string.
///
/// Input: Raw plaintext or serialized Tin bytes containing `alhalte/generast`.
/// Output: Packed (ptr << 32) | len pointing to rendered HTML UTF-8 string bytes.
///
/// # Safety
///
/// `ptr` must point to a valid readable byte buffer of at least `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn render_generast_abi(ptr: *const u8, len: usize) -> u64 {
    if ptr.is_null() || len == 0 {
        return 0;
    }
    let input_bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let resolver = NoopCharacterResolver;

    let html = match Tin::from_plaintext(input_bytes) {
        Ok(tin) if tin.label == generast::LABEL_ALHALTE_GENERAST => {
            match Generast::from_tin(&tin) {
                Ok(ast) => render_generast(&ast, &resolver),
                Err(_) => match generast::decode_ext_io(&tin) {
                    Ok((ast, _speaker)) => render_generast(&ast, &resolver),
                    Err(e) => {
                        format!(r#"<span class="saignant-syntax-error">Generast Error: {e}</span>"#)
                    }
                },
            }
        }
        _ => {
            // Fallback: try raw postcard deserialization of Generast directly
            match postcard::from_bytes::<Generast>(input_bytes) {
                Ok(ast) => render_generast(&ast, &resolver),
                Err(_) => match postcard::from_bytes::<(Generast, u64)>(input_bytes) {
                    Ok((ast, _)) => render_generast(&ast, &resolver),
                    Err(e) => {
                        format!(r#"<span class="saignant-syntax-error">Tin Error: {e}</span>"#)
                    }
                },
            }
        }
    };

    allocate_and_pack_bytes(html.into_bytes())
}

/// Returns the full Saignant CSS stylesheet (Structural + Expressive).
#[unsafe(no_mangle)]
pub extern "C" fn get_full_css_abi() -> u64 {
    let css = full_css();
    allocate_and_pack_bytes(css.into_bytes())
}
