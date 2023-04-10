use regex::Regex;
use rustc_hash::FxHashSet;
use smol_str::SmolStr;

/**
 * Valid Identifiers: [a-zA-Z][a-zA-Z0-9_-]*
 *
 * 52 characters [a-zA-Z]
 * 64 characters [a-zA-Z0-9_-]
 */
static ID_CHARS: &'static [u8] = &[
    97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115,
    116, 117, 118, 119, 120, 121, 122, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79,
    80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 95, 45,
];

/// IdSet contains a set of unique ids and generates new unique ids.
#[derive(Debug)]
pub struct IdSet {
    index: usize,
    exclude: Vec<Regex>,
    set: FxHashSet<SmolStr>,
    buf: [u8; 4],
}

impl IdSet {
    pub fn new(index: usize, exclude: Vec<Regex>) -> Self {
        Self {
            index,
            exclude,
            set: FxHashSet::default(),
            buf: [0, 0, 0, 0],
        }
    }

    /// Adds an identifier to unique set.
    pub fn add(&mut self, id: &str) -> () {
        self.set.insert(id.into());
    }

    /// Generates a new unique identifier.
    pub fn next_id(&mut self) -> SmolStr {
        loop {
            let uid = index_to_css_identifier(&mut self.buf, self.index);
            self.index += 1;
            if !self.exclude.iter().any(|r| r.is_match(uid)) {
                if !self.set.contains(uid) {
                    return uid.into();
                }
            }
        }
    }
}

/// Converts a number to a css identifier:
/// `[a-zA-Z][a-zA-Z0-9_-]*`
fn index_to_css_identifier<'a>(result: &'a mut [u8; 4], mut i: usize) -> &'a str {
    let mut base = 52;
    while i >= base {
        i -= base;
        base <<= 6;
    }

    base /= 52;
    result[0] = ID_CHARS[i / base];
    // result.push(ID_CHARS[i / base]);

    let mut p = 1;
    while base > 1 {
        i %= base;
        base >>= 6;
        result[p] = ID_CHARS[i / base];
        p += 1;
    }
    unsafe { std::str::from_utf8_unchecked(&result[..p]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_to_string_0() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 0), "a");
    }

    #[test]
    fn id_to_string_1() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 1), "b");
    }

    #[test]
    fn id_to_string_51() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 51), "Z");
    }

    #[test]
    fn id_to_string_52() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 52), "aa");
    }

    #[test]
    fn id_to_string_53() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 53), "ab");
    }

    #[test]
    fn id_to_string_54() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 54), "ac");
    }

    #[test]
    fn id_to_string_103() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 103), "aZ");
    }

    #[test]
    fn test_id_to_string_104() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 104), "a0");
    }

    #[test]
    fn id_to_string_115() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 115), "a-");
    }

    #[test]
    fn id_to_string_116() {
        let mut buf: [u8; 4] = [0, 0, 0, 0];
        assert_eq!(index_to_css_identifier(&mut buf, 116), "ba");
    }
}
