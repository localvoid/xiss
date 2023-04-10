use std::{error, fmt, io, str::CharIndices};

use crate::id::IdKind;

#[derive(Debug)]
pub enum ErrorKind {
    InvalidChar(usize, char),
    UnexpectedEOL,
    IOError(io::Error),
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    line_num: usize,
}

impl Error {
    fn new(kind: ErrorKind) -> Self {
        Self { kind, line_num: 0 }
    }

    fn with_line_num(self, line_num: usize) -> Self {
        Self { line_num, ..self }
    }
}

impl error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ErrorKind::InvalidChar(pos, c) => {
                write!(f, "Invalid char '{}' at [{};{}]", self.line_num, pos, c)
            }
            ErrorKind::UnexpectedEOL => write!(f, "Unexpected end of line {}", self.line_num),
            ErrorKind::IOError(err) => err.fmt(f),
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error {
            kind: ErrorKind::IOError(value),
            line_num: 0,
        }
    }
}

pub type ParseResult<V> = Result<V, Error>;

pub struct Parser<'a> {
    s: &'a str,
    iter: CharIndices<'a>,
    line_num: usize,
    prev_module_id: &'a str,
}

impl<'a> Parser<'a> {
    pub fn new(s: &'a str) -> Self {
        Self {
            s,
            iter: s.char_indices(),
            line_num: 1,
            prev_module_id: "",
        }
    }

    pub fn next_id(
        &mut self,
    ) -> Result<Option<(IdKind, Option<&'a str>, &'a str, &'a str)>, Error> {
        if let Some((kind, module_id, local_id, global_id)) =
            parse_line(&self.s, &mut self.iter).map_err(|err| err.with_line_num(self.line_num))?
        {
            let module_id = if self.prev_module_id == module_id {
                None
            } else {
                self.prev_module_id = module_id;
                Some(module_id)
            };
            self.line_num += 1;
            Ok(Some((kind, module_id, local_id, global_id)))
        } else {
            Ok(None)
        }
    }
}

fn parse_line<'a>(
    s: &'a str,
    iter: &mut CharIndices<'a>,
) -> ParseResult<Option<(IdKind, &'a str, &'a str, &'a str)>> {
    if let (2, kind) = parse_id_kind(iter)? {
        let (i, module_id) = parse_module_id(s, iter)?;
        let (i, local_id) = parse_local_id(s, i, iter)?;
        let global_id = parse_global_id(s, i, iter)?;
        Ok(Some((kind, module_id, local_id, global_id)))
    } else {
        Ok(None)
    }
}

fn parse_id_kind<'a>(iter: &mut CharIndices<'a>) -> ParseResult<(usize, IdKind)> {
    let kind = if let Some((i, c)) = iter.next() {
        match c {
            'C' => IdKind::Class,
            'V' => IdKind::Var,
            'K' => IdKind::Keyframes,
            _ => return Err(Error::new(ErrorKind::InvalidChar(i, c))),
        }
    } else {
        return Ok((0, IdKind::Class));
    };

    if let Some((i, c)) = iter.next() {
        if c == ',' {
            Ok((2, kind))
        } else {
            Err(Error::new(ErrorKind::InvalidChar(i, c)))
        }
    } else {
        Err(Error::new(ErrorKind::UnexpectedEOL))
    }
}

fn parse_module_id<'a>(s: &'a str, iter: &mut CharIndices<'a>) -> ParseResult<(usize, &'a str)> {
    if let Some((i, c)) = iter.next() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => {
                for (i, c) in iter {
                    match c {
                        'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '/' => {}
                        ',' => {
                            return Ok((i + 1, &s[2..i]));
                        }
                        _ => return Err(Error::new(ErrorKind::InvalidChar(i, c))),
                    }
                }
            }
            _ => return Err(Error::new(ErrorKind::InvalidChar(i, c))),
        }
    }
    Err(Error::new(ErrorKind::UnexpectedEOL))
}

fn parse_local_id<'a>(
    s: &'a str,
    start: usize,
    iter: &mut CharIndices<'a>,
) -> ParseResult<(usize, &'a str)> {
    if let Some((i, c)) = iter.next() {
        match c {
            'a'..='z' | 'A'..='Z' | '_' => {
                for (i, c) in iter {
                    match c {
                        'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => {}
                        ',' => {
                            return Ok((i + 1, &s[start..i]));
                        }
                        _ => return Err(Error::new(ErrorKind::InvalidChar(i, c))),
                    }
                }
            }
            _ => return Err(Error::new(ErrorKind::InvalidChar(i, c))),
        }
    }

    Err(Error::new(ErrorKind::UnexpectedEOL))
}

fn parse_global_id<'a>(
    s: &'a str,
    start: usize,
    iter: &mut CharIndices<'a>,
) -> ParseResult<&'a str> {
    if let Some((i, c)) = iter.next() {
        match c {
            'a'..='z' | 'A'..='Z' => {
                for (i, c) in iter {
                    match c {
                        'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => {}
                        '\n' => {
                            return Ok(&s[start..i]);
                        }
                        _ => return Err(Error::new(ErrorKind::InvalidChar(i, c))),
                    }
                }
                Ok(&s[start..])
            }
            _ => Err(Error::new(ErrorKind::InvalidChar(i, c))),
        }
    } else {
        Err(Error::new(ErrorKind::UnexpectedEOL))
    }
}

#[cfg(test)]
mod tests {
    mod parse_line {
        use std::fmt::Write;

        use super::super::*;

        #[test]
        fn single_chars() {
            let line = "C,m,l,g\n";
            let mut iter = line.char_indices();
            let (kind, module_id, local_id, global_id) =
                parse_line(line, &mut iter).unwrap().unwrap();
            assert_eq!(kind, IdKind::Class);
            assert_eq!(module_id, "m");
            assert_eq!(local_id, "l");
            assert_eq!(global_id, "g");
        }

        #[test]
        fn multiple_chars() {
            let line = "C,m2,l2,g2\n";
            let mut iter = line.char_indices();
            let (kind, module_id, local_id, global_id) =
                parse_line(line, &mut iter).unwrap().unwrap();
            assert_eq!(kind, IdKind::Class);
            assert_eq!(module_id, "m2");
            assert_eq!(local_id, "l2");
            assert_eq!(global_id, "g2");
        }

        #[test]
        fn var() {
            let line = "V,m,l,g\n";
            let mut iter = line.char_indices();
            let (kind, module_id, local_id, global_id) =
                parse_line(line, &mut iter).unwrap().unwrap();
            assert_eq!(kind, IdKind::Var);
            assert_eq!(module_id, "m");
            assert_eq!(local_id, "l");
            assert_eq!(global_id, "g");
        }

        #[test]
        fn keyframes() {
            let line = "K,m,l,g\n";
            let mut iter = line.char_indices();
            let (kind, module_id, local_id, global_id) =
                parse_line(line, &mut iter).unwrap().unwrap();
            assert_eq!(kind, IdKind::Keyframes);
            assert_eq!(module_id, "m");
            assert_eq!(local_id, "l");
            assert_eq!(global_id, "g");
        }

        #[test]
        fn module_id_valid_chars() {
            let mut line = String::with_capacity(10);
            let mut expected_module_id = String::with_capacity(3);
            let mut test = move |c1: char, c2: Option<char>, c3: Option<char>| {
                expected_module_id.push(c1);
                if let Some(c2) = c2 {
                    expected_module_id.push(c2);
                    if let Some(c3) = c3 {
                        expected_module_id.push(c3);
                    }
                }
                write!(&mut line, "K,{},l,g\n", &expected_module_id).unwrap();
                let mut iter = line.char_indices();
                let (_, module_id, _, _) = parse_line(&line, &mut iter).unwrap().unwrap();
                assert_eq!(module_id, &expected_module_id);
                line.clear();
                expected_module_id.clear();
            };

            for c in 'a'..='z' {
                test(c, None, None);
                test('a', Some(c), None);
                test('a', Some('/'), Some(c));
            }
            for c in 'A'..='Z' {
                test(c, None, None);
                test('a', Some(c), None);
                test('a', Some('/'), Some(c));
            }
            for c in '0'..='9' {
                test(c, None, None);
                test('a', Some(c), None);
                test('a', Some('/'), Some(c));
            }
            test('_', None, None);
            test('a', Some('_'), None);
            test('a', Some('/'), Some('_'));

            test('a', Some('-'), None);
            test('a', Some('/'), Some('-'));
        }
    }
}
