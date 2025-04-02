// some useful utility functions
use anyhow::{Result, anyhow};
use std::iter::Iterator;
use std::path::Path;
use std::fs::File;
use std::io::{Read, BufRead, BufReader, Bytes};

// Reads file line by line, splitting lines longer than `max_chars_per_line`.
// Tries to wrap at whitespace for ASCII text.
pub fn open_file_by_lines_width<P: AsRef<Path>>(path: P, max_chars_per_line: usize) -> Result<Vec<String>> {
    let path = path.as_ref();
    if path.exists() && path.is_file() {
        match File::open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let width_iter = WidthLineIterator::new(reader, max_chars_per_line);
                Ok(width_iter.collect())
            },
            Err(err) => Err(anyhow!("{}: {}", path.display(), err)),
        }
    } else {
        Err(anyhow!(
            "{}: doesn't exist or is not a regular file", path.display()))
    }
}

// Reads file line by line without width constraints.
pub fn open_file_by_lines<P: AsRef<Path>>(path: P) -> Result<Vec<String>> {
    let path = path.as_ref();
    if path.exists() && path.is_file() {
        match File::open(path) {
            Ok(file) => {
                let reader = BufReader::new(file);
                let lines = reader.lines().collect::<Result<Vec<String>, _>>()
                    .map_err(|e| anyhow!("{}: {}", path.display(), e))?;
                Ok(lines)
            },
            Err(err) => Err(anyhow!("{}: {}", path.display(), err)),
        }
    } else {
        Err(anyhow!(
            "{}: doesn't exist or is not a regular file", path.display()))
    }
}


// --- WidthLineIterator ---
// Iterator that reads lines from a BufReader, but splits lines exceeding
// a specified character width, attempting word wrapping for ASCII.

struct WidthLineIterator<R: BufRead> {
    reader: R,
    max_width: usize,
    buffer: String, // Holds leftover part of a line for the next iteration
}

impl<R: BufRead> WidthLineIterator<R> {
    fn new(reader: R, max_width: usize) -> Self {
        WidthLineIterator {
            reader,
            max_width,
            buffer: String::new(),
        }
    }
}

impl<R: BufRead> Iterator for WidthLineIterator<R> {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If buffer has content exceeding max_width, process it first
            if self.buffer.chars().count() > self.max_width {
                let (line_part, remaining_part) = split_line(&self.buffer, self.max_width);
                self.buffer = remaining_part;
                return Some(line_part);
            }

            // Read a new line from the reader
            let mut line = String::new();
            match self.reader.read_line(&mut line) {
                Ok(0) => { // EOF
                    // If buffer still has content, return it as the last line
                    if !self.buffer.is_empty() {
                        let last_part = std::mem::take(&mut self.buffer);
                        return Some(last_part);
                    } else {
                        return None; // No more lines and buffer is empty
                    }
                }
                Ok(_) => { // Successfully read a line
                    // Prepend buffer content to the newly read line
                    self.buffer.push_str(&line.trim_end_matches(|c| c == '\r' || c == '\n'));

                    // If the combined buffer now exceeds max_width, process it
                    if self.buffer.chars().count() > self.max_width {
                        let (line_part, remaining_part) = split_line(&self.buffer, self.max_width);
                        self.buffer = remaining_part;
                        return Some(line_part);
                    } else {
                        // Line (with buffer content) fits, return it and clear buffer
                        let full_line = std::mem::take(&mut self.buffer);
                        return Some(full_line);
                    }
                }
                Err(e) => {
                    eprintln!("Error reading line: {}", e);
                    // Treat error as EOF, return any remaining buffer content
                     if !self.buffer.is_empty() {
                        let last_part = std::mem::take(&mut self.buffer);
                        return Some(last_part);
                    } else {
                        return None;
                    }
                }
            }
        }
    }
}

// Helper function to split a line at max_width, trying to wrap at whitespace.
fn split_line(line: &str, max_width: usize) -> (String, String) {
    if line.chars().count() <= max_width {
        return (line.to_string(), String::new());
    }

    // Find the character index corresponding to max_width
    let mut split_char_index = 0;
    for (idx, _) in line.char_indices().skip(max_width) {
        split_char_index = idx;
        break;
    }
     // If max_width lands exactly at the end, handle potential edge case (though unlikely with > check)
    if split_char_index == 0 && line.chars().count() > max_width {
         split_char_index = line.char_indices().nth(max_width).map(|(i, _)| i).unwrap_or(line.len());
    }


    // Look backwards from the split point for whitespace (only for ASCII for simplicity)
    let potential_split_point = &line[..split_char_index];
    let wrap_index = potential_split_point
        .char_indices()
        .rev()
        .find(|&(_, c)| c.is_ascii_whitespace())
        .map(|(i, _)| i);

    if let Some(idx) = wrap_index {
        // Found whitespace: split before it, trim whitespace from end of first part
        // and start of second part.
        let first_part = potential_split_point[..idx].trim_end().to_string();
        let second_part = line[idx..].trim_start().to_string();
        (first_part, second_part)
    } else {
        // No whitespace found before split point, or non-ASCII: hard break at max_width chars
        let (first_part, second_part) = line.split_at(split_char_index);
        (first_part.to_string(), second_part.to_string())
    }
}


#[cfg(test)]
mod test_utils{
  use super::*;
  use std::io::Cursor;

  #[test]
  fn test_open_file_not_found() {
        match open_file_by_lines("/tmp/file-does-not-exist-hopefully") {
            Ok(_) => panic!("Should have failed"),
            Err(e) => assert!(e.to_string().contains("doesn't exist or is not a regular file")),
        }
         match open_file_by_lines_width("/tmp/file-does-not-exist-hopefully", 80) {
            Ok(_) => panic!("Should have failed"),
            Err(e) => assert!(e.to_string().contains("doesn't exist or is not a regular file")),
        }
  }

  #[test]
  fn test_read_lines_basic() {
      let data = "line1\nline2\nline3";
      let cursor = Cursor::new(data);
      let reader = BufReader::new(cursor);
      let lines: Vec<String> = reader.lines().map(|l| l.unwrap()).collect();
      assert_eq!(lines, vec!["line1", "line2", "line3"]);
  }

   #[test]
    fn test_split_line_simple() {
        let (l, r) = split_line("abcdefghijkl", 5);
        assert_eq!(l, "abcde");
        assert_eq!(r, "fghijkl");
    }

    #[test]
    fn test_split_line_with_whitespace_wrap() {
        let (l, r) = split_line("abcde fghijkl", 8);
        assert_eq!(l, "abcde"); // Wraps before 'f' at the space
        assert_eq!(r, "fghijkl");
    }

     #[test]
    fn test_split_line_with_whitespace_at_end() {
        let (l, r) = split_line("abcde ", 5); // Space is exactly at width limit
        assert_eq!(l, "abcde"); // Space is trimmed
        assert_eq!(r, "");
    }

    #[test]
    fn test_split_line_no_whitespace() {
        let (l, r) = split_line("abcdefghijkl", 5);
        assert_eq!(l, "abcde"); // Hard break
        assert_eq!(r, "fghijkl");
    }

     #[test]
    fn test_split_line_non_ascii() {
        let (l, r) = split_line("你好世界你好世界", 3); // Split after 3 chars
        assert_eq!(l, "你好世");
        assert_eq!(r, "界你好世界");
    }


  #[test]
  fn test_width_iter_long_text_no_wrap() {
        let data = "123123123";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 3);
        let lines: Vec<String> = width_iter.collect();
        assert_eq!(lines, vec!["123", "123", "123"]);
  }

  #[test]
  fn test_width_iter_non_ascii_wrap() {
        let data = "当我发现我童年和少年时期的旧日记时，它们已经被尘埃所覆盖。";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 26);
        let lines: Vec<String> = width_iter.collect();
        // Should hard break as no ASCII whitespace involved
        assert_eq!(lines, vec!["当我发现我童年和少年时期的旧日记时，它们已经被尘埃所", "覆盖。"]);
  }

  #[test]
  fn test_width_iter_text_wrapping_ascii() {
        let data = "When I found my old diaries from my childhood and teen years, they were covered in dust.";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 76);
        let lines: Vec<String> = width_iter.collect();
        // Should wrap at "were"
        assert_eq!(lines, vec!["When I found my old diaries from my childhood and teen years, they were", "covered in dust."]);
  }

   #[test]
  fn test_width_iter_multiple_lines_wrapping() {
        let data = "This is the first line which is quite long and needs wrapping.\nThis is the second line, also long.\nShort third.";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 20);
        let lines: Vec<String> = width_iter.collect();
        assert_eq!(lines, vec![
            "This is the first",
            "line which is quite",
            "long and needs",
            "wrapping.",
            "This is the second",
            "line, also long.",
            "Short third."
            ]);
  }

   #[test]
  fn test_width_iter_empty_lines() {
        let data = "Line 1\n\nLine 3";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 80);
        let lines: Vec<String> = width_iter.collect();
        assert_eq!(lines, vec!["Line 1", "", "Line 3"]);
  }

   #[test]
  fn test_width_iter_exact_width() {
        let data = "12345\n67890";
        let cursor = Cursor::new(data);
        let reader = BufReader::new(cursor);
        let width_iter = WidthLineIterator::new(reader, 5);
        let lines: Vec<String> = width_iter.collect();
        assert_eq!(lines, vec!["12345", "67890"]);
  }
}
