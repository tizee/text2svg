// some useful utility functions
use anyhow::{Result, anyhow};
use std::iter::Iterator;
use std::path::Path;
use std::fs::File;
use std::io::{Read, BufRead, BufReader, Bytes};

// the width is actually the max characters for a line
pub fn open_file_by_lines_width<P: AsRef<Path>>(path: P, step: usize)  -> Result<Vec<String>> {
    let path = path.as_ref();
    if path.exists() && path.is_file() {
        return match File::open(path) {
            Ok(file) => Ok(read_file_by_chars(file,step)),
            Err(err) => Err(anyhow!(format!("{}: {}",path.display(),err))),
        };
    }
    Err(anyhow!(format!(
                "{}: doesn't exist or is not a regular file", path.display())))
}


pub fn open_file_by_lines<P: AsRef<Path>>(path: P)  -> Result<Vec<String>> {
    let path = path.as_ref();
    if path.exists() && path.is_file() {
        return match File::open(path) {
            Ok(file) => Ok(read_file_by_lines(file)),
            Err(err) => Err(anyhow!(format!("{}: {}",path.display(),err))),
        };
    }
    Err(anyhow!(format!(
                "{}: doesn't exist or is not a regular file", path.display())))
}

fn read_file_by_lines<R: Read>(file: R) -> Vec<String> {
    let reader = BufReader::new(file);
    let mut lines = vec![];
    reader.lines().for_each(|line| {
        let line = line.unwrap();
        lines.push(line);
    });
    lines
}

pub struct WidthIter<R> {
    byte_iter: Bytes<BufReader<R>>,
    step: usize,
    buffer: Vec<u8>,
    eof: bool,
    last_word: Option<String>,
}

impl<R> WidthIter<R> {
    pub fn new(iter: Bytes<BufReader<R>>,step: usize) -> Self {
        Self {
            byte_iter: iter,
            step,
            buffer: Vec::new(),
            last_word: None,
            eof: false,
        }
    }
}

impl<R: Read> Iterator for WidthIter<R> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        if self.eof {
            return None;
        }

        let mut char_counter = 0;
        if let Some(last_word) = &self.last_word {
            self.buffer.extend(last_word.bytes());
            char_counter += last_word.chars().count();
            self.last_word = None;
        }

        // Take at most step-length long string then append with line break character.
        // Then it falls back to the same logic for the line iterator.
        while char_counter < self.step {
            if let Some(Ok(ch_u8)) = self.byte_iter.next() {

                if ch_u8 == b'\n' {
                    // When encounter line break, it means this line does not exceed max width.
                    break;
                }

                self.buffer.push(ch_u8);
                if let Ok(line) = std::str::from_utf8(&self.buffer) {
                    char_counter = line.chars().count();
                    println!("buffer:\n{:?}", line);
                }
            } else {
                self.eof = true;
                break;
            }
        }

        if self.eof && self.buffer.is_empty() {
            return None;
        }

        // It's weird and hard for reading to cut off a word to cross lines.
        // So I'd like to move the whole word to next line which behaves the same as the text wrapping behavior in CSS.
        // Note that there are text wrapping algorithms which is an over-kill feature for this application.
        // The logic here is straightforward:
        // 1. For Non-ASCII characters we just break line.
        // 2. For ASCII word, we put it at the beginning of next line.
        if char_counter >= self.step {
            if let Ok(cur_line) = std::str::from_utf8(&self.buffer.clone()) {
                let mut last_word = String::new();

                if let Some((space_idx,_)) = cur_line.char_indices().rev().find(|(_,c)| c.is_ascii_whitespace()) {
                    println!("space index {:?} in {:?}", space_idx, cur_line);
                    // For All ASCII text, if the whitespace is not the last character,
                    // then this means we have borken up a word.
                    if space_idx != cur_line.chars().count() - 1 {
                        // Make sure it's all ASCII text,
                        // which means byte index is the same as the character index
                        if char::from_u32(*cur_line.as_bytes().get(space_idx).unwrap() as u32).unwrap().is_ascii_whitespace() {
                        let (line, part_word) = cur_line.split_at(space_idx);
                            last_word.push_str(part_word.trim_start());
                            println!("line:\n {:?}\npart_word:\n{:?}", line, part_word.trim_start());
                            self.buffer = line.trim_end().as_bytes().to_vec();
                        }
                    }
                }

                self.last_word = Some(last_word);
            }
        }

        let line = String::from_utf8(self.buffer.clone()).unwrap();
        self.buffer.clear();

        Some(line)
    }
}

fn read_file_by_chars<R: Read>(file: R, step: usize) ->  Vec<String> {
    let reader = BufReader::new(file);
    let byte_iter = reader.bytes();
    let width_iter= WidthIter::new(byte_iter, step);
    let mut lines = vec![];
    width_iter.for_each(|line| {
        lines.push(line);
    });
    lines
}


#[cfg(test)]
mod test_utils{
  use super::*;
  #[test]
  #[should_panic(expected="should panic")]
  fn test_open_file_by_lines() {
        match open_file_by_lines("/tmp/file-does-not-exist") {
            Ok(_) => (),
            Err(_) => panic!("should panic"),
        }
  }

  #[test]
  fn test_width_iter_long_text() {
        let reader = BufReader::new(&b"123123123"[..]);

        let byte_iter = reader.bytes();
        let width_iter= WidthIter::new(byte_iter, 3);
        width_iter.enumerate().for_each(|(idx,line)| {
            println!("{:?} {:?}", idx, line);
            assert_eq!("123", line);
        });
  }

  #[test]
  fn test_width_iter_non_ascii() {
        let reader = BufReader::new("当我发现我童年和少年时期的旧日记时，它们已经被尘埃所覆盖。".as_bytes());
        let ans = vec!["当我发现我童年和少年时期的旧日记时，它们已经被尘埃所", "覆盖。"];
        let byte_iter = reader.bytes();
        let width_iter= WidthIter::new(byte_iter, 26);
        width_iter.enumerate().for_each(|(idx,line)| {
            println!("{:?} {:?}", idx, line);
            assert_eq!(ans[idx], line);
        });
  }

  #[test]
  fn test_width_iter_text_wrapping() {
        let reader = BufReader::new("When I found my old diaries from my childhood and teen years, they were covered in dust.".as_bytes());
        let ans = vec!["When I found my old diaries from my childhood and teen years, they were", "covered in dust."];
        let byte_iter = reader.bytes();
        let width_iter= WidthIter::new(byte_iter, 76);
        width_iter.enumerate().for_each(|(idx,line)| {
            println!("{:?} {:?}", idx, line);
            assert_eq!(ans[idx], line);
        });
  }
}

