// These methods are necessarily inefficient, because they must revert encoding
// conversions performed by the standard library. However, there is currently
// no better alternative.

use std::borrow::Cow;
use std::char;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::ffi::OsStringExt;
use std::str;

use crate::EncodingError;
use crate::OsStrBytes;
use crate::OsStringBytes;

fn decode_utf16<TString>(encoded_string: TString, length: usize) -> Vec<u8>
where
    TString: IntoIterator<Item = u16>,
{
    // https://github.com/rust-lang/rust/blob/4560ea788cb760f0a34127156c78e2552949f734/src/libstd/sys_common/wtf8.rs#L183-L201

    let mut string = Vec::with_capacity(length);
    let mut buffer = [0; size_of::<char>()];
    for ch in char::decode_utf16(encoded_string) {
        let unchecked_char = ch.unwrap_or_else(|surrogate| {
            let surrogate = surrogate.unpaired_surrogate().into();
            // SAFETY: This conversion creates an invalid [char] value.
            // However, there is otherwise no way to encode a [u32] value as
            // invalid UTF-8, which is why the standard library uses the same
            // approach:
            // https://github.com/rust-lang/rust/blob/4560ea788cb760f0a34127156c78e2552949f734/src/libstd/sys_common/wtf8.rs#L206-L208
            unsafe { char::from_u32_unchecked(surrogate) }
        });
        string.extend_from_slice(
            unchecked_char.encode_utf8(&mut buffer).as_bytes(),
        );
    }
    debug_assert_eq!(string.len(), length);
    string
}

fn encode_utf16(string: &[u8]) -> Vec<u16> {
    // https://github.com/rust-lang/rust/blob/4560ea788cb760f0a34127156c78e2552949f734/src/libstd/sys_common/wtf8.rs#L813-L831

    // SAFETY: This conversion technically causes undefined behavior when
    // [string] is not representable as UTF-8. However, [next_code_point()] is
    // not exposed; it is only available through [str] methods. This string
    // will be dropped at the end of this method.
    // https://github.com/rust-lang/rust/blob/4560ea788cb760f0a34127156c78e2552949f734/src/libcore/str/mod.rs#L500-L528
    let unchecked_string = unsafe { str::from_utf8_unchecked(string) };
    unchecked_string.encode_utf16().collect::<Vec<_>>()
}

impl OsStrBytes for OsStr {
    #[inline]
    fn from_bytes(string: &[u8]) -> Result<Cow<'_, Self>, EncodingError> {
        Ok(Cow::Owned(OsString::from_bytes(string)?))
    }

    #[inline]
    unsafe fn from_bytes_unchecked(string: &[u8]) -> Cow<'_, Self> {
        Cow::Owned(OsString::from_bytes_unchecked(string))
    }

    #[inline]
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(decode_utf16(self.encode_wide(), self.len()))
    }
}

impl OsStringBytes for OsString {
    #[allow(clippy::map_clone)]
    fn from_bytes<TString>(string: TString) -> Result<Self, EncodingError>
    where
        TString: AsRef<[u8]>,
    {
        let string = string.as_ref();
        let encoded_string = encode_utf16(string);
        if decode_utf16(encoded_string.iter().map(|&x| x), string.len())
            == string
        {
            Ok(OsString::from_wide(&encoded_string))
        } else {
            Err(EncodingError(()))
        }
    }

    #[inline]
    unsafe fn from_bytes_unchecked<TString>(string: TString) -> Self
    where
        TString: AsRef<[u8]>,
    {
        OsString::from_wide(&encode_utf16(string.as_ref()))
    }

    #[inline]
    fn from_vec(string: Vec<u8>) -> Result<Self, EncodingError> {
        Self::from_bytes(string)
    }

    #[inline]
    unsafe fn from_vec_unchecked(string: Vec<u8>) -> Self {
        Self::from_bytes_unchecked(string)
    }

    #[inline]
    fn into_vec(self) -> Vec<u8> {
        self.to_bytes().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::ffi::OsString;

    use crate::EncodingError;
    use crate::OsStrBytes;
    use crate::OsStringBytes;

    const INVALID_STRING: &[u8] = b"\xF1foo\xF1\x80bar\xF1\x80\x80baz";

    #[test]
    fn test_invalid_bytes() {
        assert_eq!(Err(EncodingError(())), OsStr::from_bytes(INVALID_STRING));
        assert_eq!(
            Err(EncodingError(())),
            OsString::from_bytes(INVALID_STRING),
        );
    }

    #[test]
    fn test_invalid_vec() {
        assert_eq!(
            Err(EncodingError(())),
            OsString::from_vec(INVALID_STRING.to_vec()),
        );
    }
}
