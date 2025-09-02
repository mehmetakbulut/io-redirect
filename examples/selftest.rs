use std::fs::File;
use std::io::{stderr, stdout, Read, Write};
use io_redirect::redirect_std_to_path;

/// This executable demonstrates the process of redirecting both `stdout`
/// and `stderr` to a specified file path and validating that the contents
/// of the file match the expected output.
fn main() {
    // Arrange
    let tempdir = tempfile::tempdir().unwrap();
    let log_path = tempdir.path().join("log.txt");

    // Act
    redirect_std_to_path(log_path.as_path(), true).unwrap();
    print!("Hello to stdout!");
    stdout().flush().unwrap();
    eprint!("Hello to stderr!");
    stderr().flush().unwrap();

    // Assert
    let mut dst_contents = String::new();
    File::open(&log_path).unwrap().read_to_string(&mut dst_contents).unwrap();
    assert_eq!(dst_contents, "Hello to stdout!Hello to stderr!");
}