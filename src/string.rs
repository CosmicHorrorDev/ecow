use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt::{self, Debug, Display, Formatter, Write};
use std::hash::{Hash, Hasher};
use std::ops::{Add, AddAssign, Deref};

use super::EcoVec;

/// Create a new [`EcoString`] from a format string.
#[macro_export]
macro_rules! format_eco {
    ($($tts:tt)*) => {{
        use std::fmt::Write;
        let mut s = $crate::util::EcoString::new();
        write!(s, $($tts)*).unwrap();
        s
    }};
}

/// An economical string with inline storage and clone-on-write semantics.
#[derive(Clone)]
pub struct EcoString(Repr);

/// The internal representation. Either:
/// - inline when below a certain number of bytes, or
/// - reference-counted on the heap with clone-on-write semantics.
#[derive(Clone)]
enum Repr {
    Small { buf: [u8; LIMIT], len: u8 },
    Large(EcoVec<u8>),
}

/// The maximum number of bytes that can be stored inline.
///
/// The value is chosen such that an `EcoString` fits exactly into 16 bytes
/// (which are needed anyway due to the `Arc`s alignment, at least on 64-bit
/// platforms).
///
/// Must be at least 4 to hold any char.
const LIMIT: usize = 14;

impl EcoString {
    /// Create a new, empty string.
    pub const fn new() -> Self {
        Self(Repr::Small { buf: [0; LIMIT], len: 0 })
    }

    /// Create a new, empty string with the given `capacity`.
    pub fn with_capacity(capacity: usize) -> Self {
        if capacity <= LIMIT {
            Self::new()
        } else {
            Self(Repr::Large(EcoVec::with_capacity(capacity)))
        }
    }

    /// Create an instance from an existing string-like type.
    fn from_str_like(string: impl AsRef<str>) -> Self {
        let string = string.as_ref();
        let len = string.len();
        Self(if len <= LIMIT {
            let mut buf = [0; LIMIT];
            buf[..len].copy_from_slice(string.as_bytes());
            Repr::Small { buf, len: len as u8 }
        } else {
            Repr::Large(string.as_bytes().into())
        })
    }

    /// Whether the string is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The length of the string in bytes.
    pub fn len(&self) -> usize {
        match &self.0 {
            Repr::Small { len, .. } => usize::from(*len),
            Repr::Large(string) => string.len(),
        }
    }

    /// A string slice containing the entire string.
    pub fn as_str(&self) -> &str {
        self
    }

    /// Append the given character at the end.
    pub fn push(&mut self, c: char) {
        if let Repr::Small { buf, len } = &mut self.0 {
            let prev = usize::from(*len);
            if c.len_utf8() == 1 && prev < LIMIT {
                buf[prev] = c as u8;
                *len += 1;
                return;
            }
        }

        self.push_str(c.encode_utf8(&mut [0; 4]));
    }

    /// Append the given string slice at the end.
    pub fn push_str(&mut self, string: &str) {
        match &mut self.0 {
            Repr::Small { buf, len } => {
                let prev = usize::from(*len);
                let new = prev + string.len();
                if new <= LIMIT {
                    buf[prev..new].copy_from_slice(string.as_bytes());
                    *len = new as u8;
                } else {
                    let mut spilled = String::with_capacity(new);
                    spilled.push_str(self);
                    spilled.push_str(string);
                    *self = spilled.into();
                }
            }
            Repr::Large(vec) => vec.extend(string.as_bytes().iter().copied()),
        }
    }

    /// Remove the last character from the string.
    pub fn pop(&mut self) -> Option<char> {
        let c = self.as_str().chars().rev().next()?;
        let len_utf8 = c.len_utf8();
        match &mut self.0 {
            Repr::Small { len, .. } => *len -= len_utf8 as u8,
            Repr::Large(vec) => vec.truncate(vec.len() - len_utf8),
        }
        Some(c)
    }

    /// Clear the string.
    pub fn clear(&mut self) {
        match &mut self.0 {
            Repr::Small { len, .. } => *len = 0,
            Repr::Large(vec) => vec.clear(),
        }
    }

    /// Convert the string to lowercase.
    pub fn to_lowercase(&self) -> Self {
        if let Repr::Small { mut buf, len } = self.0 {
            if self.is_ascii() {
                buf[..usize::from(len)].make_ascii_lowercase();
                return Self(Repr::Small { buf, len });
            }
        }

        self.as_str().to_lowercase().into()
    }

    /// Convert the string to uppercase.
    pub fn to_uppercase(&self) -> Self {
        if let Repr::Small { mut buf, len } = self.0 {
            if self.is_ascii() {
                buf[..usize::from(len)].make_ascii_uppercase();
                return Self(Repr::Small { buf, len });
            }
        }

        self.as_str().to_uppercase().into()
    }

    /// Repeat this string `n` times.
    pub fn repeat(&self, n: usize) -> Self {
        if n == 0 {
            return Self::new();
        }

        if let Repr::Small { buf, len } = self.0 {
            let prev = usize::from(len);
            let new = prev.saturating_mul(n);
            if new <= LIMIT {
                let src = &buf[..prev];
                let mut buf = [0; LIMIT];
                for i in 0..n {
                    buf[prev * i..prev * (i + 1)].copy_from_slice(src);
                }
                return Self(Repr::Small { buf, len: new as u8 });
            }
        }

        self.as_str().repeat(n).into()
    }
}

impl Deref for EcoString {
    type Target = str;

    fn deref(&self) -> &str {
        let buf = match &self.0 {
            Repr::Small { buf, len } => &buf[..usize::from(*len)],
            Repr::Large(vec) => &vec,
        };

        // Safety:
        // The buffer contents stem from correct UTF-8 sources:
        // - Valid ASCII characters
        // - Other string slices
        // - Chars that were encoded with char::encode_utf8
        // Furthermore, we still do the bounds-check on the len in case
        // it gets corrupted somehow.
        unsafe { std::str::from_utf8_unchecked(buf) }
    }
}

impl Default for EcoString {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for EcoString {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Debug::fmt(self.as_str(), f)
    }
}

impl Display for EcoString {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(self.as_str(), f)
    }
}

impl Eq for EcoString {}

impl PartialEq for EcoString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl PartialEq<str> for EcoString {
    fn eq(&self, other: &str) -> bool {
        self.as_str().eq(other)
    }
}

impl PartialEq<&str> for EcoString {
    fn eq(&self, other: &&str) -> bool {
        self.as_str().eq(*other)
    }
}

impl PartialEq<String> for EcoString {
    fn eq(&self, other: &String) -> bool {
        self.as_str().eq(other)
    }
}

impl PartialEq<EcoString> for str {
    fn eq(&self, other: &EcoString) -> bool {
        self.eq(other.as_str())
    }
}

impl PartialEq<EcoString> for &str {
    fn eq(&self, other: &EcoString) -> bool {
        (*self).eq(other.as_str())
    }
}

impl PartialEq<EcoString> for String {
    fn eq(&self, other: &EcoString) -> bool {
        self.eq(other.as_str())
    }
}

impl Ord for EcoString {
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for EcoString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.as_str().partial_cmp(other.as_str())
    }
}

impl Hash for EcoString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl Write for EcoString {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }

    fn write_char(&mut self, c: char) -> fmt::Result {
        self.push(c);
        Ok(())
    }
}

impl Add for EcoString {
    type Output = Self;

    fn add(mut self, rhs: Self) -> Self::Output {
        self += rhs;
        self
    }
}

impl AddAssign for EcoString {
    fn add_assign(&mut self, rhs: Self) {
        self.push_str(rhs.as_str());
    }
}

impl AsRef<str> for EcoString {
    fn as_ref(&self) -> &str {
        self
    }
}

impl Borrow<str> for EcoString {
    fn borrow(&self) -> &str {
        self
    }
}

impl From<char> for EcoString {
    fn from(c: char) -> Self {
        let mut buf = [0; LIMIT];
        let len = c.encode_utf8(&mut buf).len();
        Self(Repr::Small { buf, len: len as u8 })
    }
}

impl From<&str> for EcoString {
    fn from(s: &str) -> Self {
        Self::from_str_like(s)
    }
}

impl From<String> for EcoString {
    /// When the string does not fit inline, this needs to allocate to change
    /// the layout.
    fn from(s: String) -> Self {
        Self::from_str_like(s)
    }
}

impl FromIterator<char> for EcoString {
    fn from_iter<T: IntoIterator<Item = char>>(iter: T) -> Self {
        let mut s = Self::new();
        for c in iter {
            s.push(c);
        }
        s
    }
}

impl FromIterator<Self> for EcoString {
    fn from_iter<T: IntoIterator<Item = Self>>(iter: T) -> Self {
        let mut s = Self::new();
        for piece in iter {
            s.push_str(&piece);
        }
        s
    }
}

impl Extend<char> for EcoString {
    fn extend<T: IntoIterator<Item = char>>(&mut self, iter: T) {
        for c in iter {
            self.push(c);
        }
    }
}

impl From<EcoString> for String {
    /// This needs to allocate to change the layout.
    fn from(s: EcoString) -> Self {
        s.as_str().to_owned()
    }
}

impl From<&EcoString> for String {
    fn from(s: &EcoString) -> Self {
        s.as_str().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALPH: &str = "abcdefghijklmnopqrstuvwxyz";

    #[test]
    fn test_str_new() {
        // Test inline strings.
        assert_eq!(EcoString::new(), "");
        assert_eq!(EcoString::from('a'), "a");
        assert_eq!(EcoString::from('😀'), "😀");
        assert_eq!(EcoString::from("abc"), "abc");

        // Test around the inline limit.
        assert_eq!(EcoString::from(&ALPH[..LIMIT - 1]), ALPH[..LIMIT - 1]);
        assert_eq!(EcoString::from(&ALPH[..LIMIT]), ALPH[..LIMIT]);
        assert_eq!(EcoString::from(&ALPH[..LIMIT + 1]), ALPH[..LIMIT + 1]);

        // Test heap string.
        assert_eq!(EcoString::from(ALPH), ALPH);
    }

    #[test]
    fn test_str_push() {
        let mut v = EcoString::new();
        v.push('a');
        v.push('b');
        v.push_str("cd😀");
        assert_eq!(v, "abcd😀");
        assert_eq!(v.len(), 8);

        // Test fully filling the inline storage.
        v.push_str("efghij");
        assert_eq!(v.len(), LIMIT);

        // Test spilling with `push`.
        let mut a = v.clone();
        a.push('k');
        assert_eq!(a, "abcd😀efghijk");
        assert_eq!(a.len(), 15);

        // Test spilling with `push_str`.
        let mut b = v.clone();
        b.push_str("klmn");
        assert_eq!(b, "abcd😀efghijklmn");
        assert_eq!(b.len(), 18);

        // v should be unchanged.
        assert_eq!(v.len(), LIMIT);
    }

    #[test]
    fn test_str_pop() {
        // Test with inline string.
        let mut v = EcoString::from("Hello World!");
        assert_eq!(v.pop(), Some('!'));
        assert_eq!(v, "Hello World");

        // Remove one-by-one.
        for _ in 0..10 {
            v.pop();
        }

        assert_eq!(v, "H");
        assert_eq!(v.pop(), Some('H'));
        assert_eq!(v, "");
        assert!(v.is_empty());

        // Test with large string.
        let mut v = EcoString::from(ALPH);
        assert_eq!(v.pop(), Some('z'));
        assert_eq!(v.len(), 25);
    }

    #[test]
    fn test_str_index() {
        // Test that we can use the index syntax.
        let v = EcoString::from("abc");
        assert_eq!(&v[..2], "ab");
    }

    #[test]
    fn test_str_case() {
        assert_eq!(EcoString::new().to_uppercase(), "");
        assert_eq!(EcoString::from("abc").to_uppercase(), "ABC");
        assert_eq!(EcoString::from("AΣ").to_lowercase(), "aς");
        assert_eq!(
            EcoString::from("a").repeat(100).to_uppercase(),
            EcoString::from("A").repeat(100)
        );
        assert_eq!(
            EcoString::from("Ö").repeat(20).to_lowercase(),
            EcoString::from("ö").repeat(20)
        );
    }

    #[test]
    fn test_str_repeat() {
        // Test with empty string.
        assert_eq!(EcoString::new().repeat(0), "");
        assert_eq!(EcoString::new().repeat(100), "");

        // Test non-spilling and spilling case.
        let v = EcoString::from("abc");
        assert_eq!(v.repeat(0), "");
        assert_eq!(v.repeat(3), "abcabcabc");
        assert_eq!(v.repeat(5), "abcabcabcabcabc");
    }
}
