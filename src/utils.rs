// some useful utility functions
use anyhow::{Result, anyhow};
use std::iter::Iterator;
use std::path::Path;
use std::fs::File;
use std::io::{Read, BufRead, BufReader, Bytes};

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
    // TODO take_whitespace: bool,
}

impl<R> WidthIter<R> {
    pub fn new(iter: Bytes<BufReader<R>>,step: usize) -> Self {
        Self {
            byte_iter: iter,
            step
        }
    }
}

impl<R: Read> Iterator for WidthIter<R> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        let mut segment = String::new();
        let mut eof = false;

        // Take at most step-length long string then append with line break character.
        // Then it falls back to the same logic for the line iterator.
        let mut counter = 0;
        while counter < self.step {
            if let Some(Ok(ch_u8)) = self.byte_iter.next() {
                if let Some(chr) = char::from_u32(ch_u8 as u32) {
                    // When encounter line break, it means this line does not exceed max width.
                    if chr == '\n' {
                        break;
                    }
                    segment.push(chr);
                    counter += 1;
                }
            } else {
                eof = true;
                break;
            }
        }

        if !eof {
            Some(segment)
        } else {
            None
        }
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
}

