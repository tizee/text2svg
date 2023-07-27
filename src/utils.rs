use anyhow::{Result, anyhow};
use std::path::Path;
use std::fs::File;
use std::io::{Read,BufRead, BufReader};

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

