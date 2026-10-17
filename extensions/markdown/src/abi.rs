//! WebAssembly ABI export implementation for Alhalte extensions.

use generast::{Generast, decode_ext_io, encode_ext_io};
use tin::Tin;

/// Returns current Alhalte extension ABI version (`1`).
#[unsafe(no_mangle)]
pub extern "C" fn alhalte_abi_version() -> i32 {
    1
}

/// Allocates memory buffer of length `len` for host input.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: i32) -> i32 {
    if len <= 0 {
        return 0;
    }
    match std::alloc::Layout::from_size_align(len as usize, 1) {
        Ok(layout) => unsafe { std::alloc::alloc(layout) as i32 },
        Err(_) => 0,
    }
}

/// Deallocates memory buffer previously allocated by this module.
#[unsafe(no_mangle)]
pub extern "C" fn dealloc(ptr: i32, len: i32) {
    if ptr == 0 || len <= 0 {
        return;
    }
    if let Ok(layout) = std::alloc::Layout::from_size_align(len as usize, 1) {
        unsafe {
            std::alloc::dealloc(ptr as *mut u8, layout);
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn custom_getrandom(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

#[cfg(target_arch = "wasm32")]
getrandom::register_custom_getrandom!(custom_getrandom);

/// Runs the extension processor on the raw input bytes and returns the raw output bytes.
pub fn process_raw_bytes<F>(input_bytes: &[u8], transform: F) -> Vec<u8>
where
    F: FnOnce(Generast, u64) -> Generast,
{
    let (input_ast, speaker_id, had_opener) = match parse_input(input_bytes) {
        Ok(v) => v,
        Err(err) => {
            let error_ast = Generast::syntax_error(err);
            let tin = match encode_ext_io(&error_ast, 0) {
                Ok(t) => t,
                Err(_) => return Vec::new(),
            };
            return tin.to_plaintext();
        }
    };

    let result_ast = transform(input_ast, speaker_id);
    let out_tin = match encode_ext_io(&result_ast, speaker_id) {
        Ok(t) => t,
        Err(e) => {
            let err_ast = Generast::syntax_error(e.to_string());
            match encode_ext_io(&err_ast, speaker_id) {
                Ok(t) => t,
                Err(_) => return Vec::new(),
            }
        }
    };

    if had_opener {
        match out_tin.encode_plaintext(tin::Compression::None) {
            Ok((wire, _)) => wire,
            Err(_) => out_tin.to_plaintext(),
        }
    } else {
        out_tin.to_plaintext()
    }
}

fn parse_input(bytes: &[u8]) -> Result<(Generast, u64, bool), String> {
    if bytes.is_empty() {
        return Err("empty input bytes".to_string());
    }
    if let Ok((tin, _opener, _)) = Tin::decode(bytes, None) {
        let (ast, speaker_id) = decode_ext_io(&tin).map_err(|e| e.to_string())?;
        return Ok((ast, speaker_id, true));
    }
    if let Ok(tin) = Tin::from_plaintext(bytes) {
        let (ast, speaker_id) = decode_ext_io(&tin).map_err(|e| e.to_string())?;
        return Ok((ast, speaker_id, false));
    }
    Err("failed to parse input as plaintext tin".to_string())
}

/// WebAssembly entry point for processing Generast syntax trees.
#[unsafe(no_mangle)]
pub extern "C" fn process_generast(ptr: i32, len: i32) -> i64 {
    if ptr == 0 || len <= 0 {
        return 0;
    }
    let input_bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let out_bytes = process_raw_bytes(input_bytes, crate::process_ast);
    let out_len = out_bytes.len();
    if out_len == 0 {
        return 0;
    }
    let out_layout = match std::alloc::Layout::from_size_align(out_len, 1) {
        Ok(l) => l,
        Err(_) => return 0,
    };
    let out_ptr = unsafe {
        let p = std::alloc::alloc(out_layout);
        if p.is_null() {
            return 0;
        }
        std::ptr::copy_nonoverlapping(out_bytes.as_ptr(), p, out_len);
        p as i32
    };

    ((out_ptr as i64) << 32) | (out_len as i64 & 0xffff_ffff)
}

/// Encodes the self-introduction metadata `(name, icon_svg, description)` using Postcard.
pub fn encode_self_introduction() -> Vec<u8> {
    let (name, icon_svg, description) = crate::self_introduction_info();
    postcard::to_allocvec(&(
        name.to_string(),
        icon_svg.to_string(),
        description.to_string(),
    ))
    .unwrap_or_default()
}

/// WebAssembly entry point returning self-introduction metadata `(name, icon_svg, description)` as Postcard bytes.
///
/// Output memory format is packed `(ptr << 32) | len`.
#[unsafe(no_mangle)]
pub extern "C" fn self_introduction() -> i64 {
    let out_bytes = encode_self_introduction();
    let out_len = out_bytes.len();
    if out_len == 0 {
        return 0;
    }
    let out_layout = match std::alloc::Layout::from_size_align(out_len, 1) {
        Ok(l) => l,
        Err(_) => return 0,
    };
    let out_ptr = unsafe {
        let p = std::alloc::alloc(out_layout);
        if p.is_null() {
            return 0;
        }
        std::ptr::copy_nonoverlapping(out_bytes.as_ptr(), p, out_len);
        p as i32
    };

    ((out_ptr as i64) << 32) | (out_len as i64 & 0xffff_ffff)
}
