//! Cross-platform I/O redirection.
//!
//! This crate provides a `Redirectable<T>` trait, platform-specific implementations of this trait,
//! and convenience methods for handling stdout and stderr redirection. This is most useful if you
//! can't print to stdout or stderr directly (e.g., a systemd generator) or need to hijack file
//! access without changing user code.
//!
//! ## Platform Support
//! | Platform  | Required Features | File to File | Stdout/Stderr to File | Any FD to Any FD |
//! | -         | -                 | -            | -                     | -                |
//! | Unix-like | `libc_on_unix`    | Yes          | Yes                   | Yes              |
//! | Windows   | `windows-sys`     | No           | Yes                   | No               |
//! | Windows   | `libc_on_windows` | Yes          | No                    | No               |
//!
//! All features are enabled by default on all platforms.
//!
//! <div class="warning">
//! On Windows, `Redirectable<T>` trait accepts any `T` that can be converted into a handle.
//! Be careful not to feed handles without file semantics such as a thread handle.
//! Also, if the handle is being directly used elsewhere, it won't benefit from redirection.
//! </div>
//!
//! ## Usage
//! For a more detailed example, see the `selftest` executable.
//!
//! ### File to File
//! ```no_run
//! use io_redirect::Redirectable;
//! # use std::fs::File;
//!
//! let mut file_src = File::create("src.txt").unwrap();
//! let file_dst = File::create("dst.txt").unwrap();
//!
//! file_src.redirect(&file_dst).unwrap();
//! ```
//!
//! ### Redirect Standard Streams to a File
//! ```no_run
//! use io_redirect::redirect_std_to_path;
//! # use std::io::stdout;
//! # use std::path::PathBuf;
//!
//! let some_path = PathBuf::from("/dev/kmsg");
//!
//! // redirect both stdout and stderr to the same file
//! redirect_std_to_path(some_path.as_path(), true).unwrap();
//!
//! // or just one stream
//! # use io_redirect::Redirectable;
//! stdout().redirect(some_path.as_path()).unwrap();
//! ```
//!
//! ## Notes and Caveats
//! - **Resource Management**: Avoid using `Redirectable<Path>::redirect(...)` multiple times on the same entity as each call will leak a file descriptor. `Redirectable<File>` does not suffer from the same.
//! - **OS-Specific Behavior**: Not all features may function identically across platforms; ensure
//!   feature flags match the intended target for compilation.
//!

use std::io;
use std::io::{Stdout, Stderr};
use std::fs::{File, OpenOptions};

/// A trait to represent entities that can have their I/O redirected to a specified target.
///
/// # Type Parameters
/// - `T`: The type of the destination. It is a dynamically sized type (`?Sized`) so that
///   it can be used with types that do not have a statically known size.
///
/// # Notes
/// Be cautious of potential side effects or resource management issues when implementing
/// this trait, especially in cases where redirection involves I/O operations or state transitions.
pub trait Redirectable<T: ?Sized>
{
    /// Redirects I/O to a specified destination.
    ///
    /// # Parameters
    /// - `destination`: A reference to the target destination.
    ///
    /// # Returns
    /// - `io::Result<()>`: `Ok` if successful, `Err` otherwise.
    ///
    /// # Examples
    /// ```no_run
    /// use io_redirect::Redirectable;
    ///
    /// let mut source = std::io::stdout();
    /// let destination = std::fs::File::create("dst.txt").unwrap();
    /// match source.redirect(&destination) {
    ///     Ok(_) => println!("Redirection successful!"),
    ///     Err(_) => eprintln!("Failed to redirect!"),
    /// }
    /// ```
    ///
    /// # Notes
    /// The behavior of this function depends on the implementation.
    fn redirect(&mut self, destination: &T) -> io::Result<()>;
}

#[cfg(any(unix))]
mod platform
{
    use super::*;
    use std::os::fd::{AsRawFd, RawFd};

    pub type Descriptor = RawFd;

    pub trait Descriptable: AsRawFd {}
    impl<T: AsRawFd> Descriptable for T {}

    impl<T1: Descriptable, T2: Descriptable> Redirectable<T2> for T1 {
        fn redirect(&mut self, destination: &T2) -> io::Result<()> {
            let src_fd = self.as_raw_fd();
            let dst_fd = destination.as_raw_fd();
            return libc_common::redirect_fd_to_fd(src_fd, dst_fd);
        }
    }
}

#[cfg(any(target_os = "windows"))]
mod platform
{
    use super::*;
    use std::os::windows::io::AsRawHandle;

    pub trait Descriptable: AsRawHandle {}
    impl<T: AsRawHandle> Descriptable for T {}

    #[cfg(feature = "libc_on_windows")]
    mod libc_backend
    {
        use std::os::windows::io::RawHandle;
        use super::*;
        use crate::{Descriptable, Redirectable};
        use libc::{c_int, get_osfhandle, open_osfhandle};

        pub type Descriptor = c_int;

        impl<T: Descriptable> Redirectable<T> for File {
            fn redirect(&mut self, destination: &T) -> io::Result<()> {
                let src_handle = self.as_raw_handle() as isize;
                let dst_handle = destination.as_raw_handle() as isize;

                let src_fd = unsafe { open_osfhandle(src_handle, 0) };
                if src_fd < 0 {
                    return Err(io::Error::last_os_error());
                }

                let dst_fd = unsafe { open_osfhandle(dst_handle, 0) };
                if dst_fd < 0 {
                    return Err(io::Error::last_os_error());
                }

                libc_common::redirect_fd_to_fd(src_fd, dst_fd)?;

                let new_src_handle = unsafe { get_osfhandle(src_fd) };
                if new_src_handle < 0 {
                    return Err(io::Error::last_os_error());
                }

                unsafe {
                    let handle_ptr = (self as *mut File) as *mut RawHandle;
                    *handle_ptr = new_src_handle as RawHandle;
                }

                return Ok(());
            }
        }
    }

    #[cfg(feature = "libc_on_windows")]
    pub use libc_backend::*;

    #[cfg(feature = "windows-sys")]
    mod windows_sys_backend
    {
        use super::*;
        use windows_sys::Win32::Foundation::HANDLE;
        use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_OUTPUT_HANDLE};

        impl<T: Descriptable> Redirectable<T> for Stdout {
            fn redirect(&mut self, destination: &T) -> io::Result<()> {
                redirect_using_setstdhandle(STD_OUTPUT_HANDLE, destination)
            }
        }

        impl<T: Descriptable> Redirectable<T> for Stderr {
            fn redirect(&mut self, destination: &T) -> io::Result<()> {
                redirect_using_setstdhandle(STD_ERROR_HANDLE, destination)
            }
        }

        fn redirect_using_setstdhandle<T: Descriptable>(std_handle: STD_HANDLE, destination: &T) -> io::Result<()> {
            let dst_handle = destination.as_raw_handle() as HANDLE;
            let result = unsafe { SetStdHandle(std_handle, dst_handle) };
            if result == 0 {
                return Err(io::Error::last_os_error());
            }
            return Ok(());
        }
    }

    #[cfg(feature = "windows-sys")]
    pub use windows_sys_backend::*;
}

#[cfg(any(all(unix, feature = "libc_on_unix"), all(target_os = "windows", feature = "libc_on_windows")))]
mod libc_common
{
    use super::*;
    use crate::platform::Descriptor;
    use libc::dup2;

    pub fn redirect_fd_to_fd(src: Descriptor, dst: Descriptor) -> io::Result<()> {
        let result = unsafe {
            dup2(dst, src)
            // After this call on Windows, get_osfhandle seems to return a different value
            // than the one passed to open_osfhandle. This is why the libc backend is off on Windows.
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }

        return Ok(());
    }
}

#[cfg(any(all(unix, feature = "libc_on_unix"), all(target_os = "windows", feature = "libc_on_windows")))]
mod libc_convenience
{
    use super::*;
    use std::fs::OpenOptions;
    use std::path::Path;


    impl<T: Redirectable<File>> Redirectable<Path> for T {
        fn redirect(&mut self, destination: &Path) -> io::Result<()> {
            let dst = OpenOptions::new().read(false).write(true).create(true).append(true).open(destination)?;
            let result = self.redirect(&dst);
            if result.is_ok() {
                std::mem::forget(dst);
            }
            return result;
        }
    }
}

#[cfg(any(all(unix, feature = "libc_on_unix"), all(target_os = "windows", feature = "libc_on_windows")))]
pub use libc_convenience::*;

mod convenience
{
    use super::*;
    use std::fs::OpenOptions;
    use std::io::{stderr, stdout};
    use std::path::Path;
    pub fn redirect_std_to_path(destination: &Path, append: bool) -> io::Result<()> {
        let dst = OpenOptions::new().read(false).write(true).create(true).append(append).open(destination)?;
        stdout().redirect(&dst)?;
        stderr().redirect(&dst)?;
        std::mem::forget(dst);
        return Ok(());
    }
}

pub use convenience::*;
pub use platform::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::mem::ManuallyDrop;
    use libc::close;

    #[cfg(any(all(unix, feature = "libc_on_unix"), all(target_os = "windows", feature = "libc_on_windows")))]
    #[test]
    fn redirects_file_to_file() {
        // Arrange
        let tempdir = tempfile::tempdir().unwrap();
        let mut file1 = File::create(tempdir.path().join("file1.txt")).unwrap();
        let mut file2 = File::create(tempdir.path().join("file2.txt")).unwrap();

        // Act
        file1.redirect(&file2).unwrap();
        file1.write_all(b"Hello,").unwrap();
        file1.flush().unwrap();
        file2.write_all(b" World!").unwrap();
        file2.flush().unwrap();

        // Assert
        let mut dst_file = File::open(tempdir.path().join("file2.txt")).unwrap();
        let mut dst_contents = String::new();
        dst_file.read_to_string(&mut dst_contents).unwrap();
        assert_eq!(dst_contents, "Hello, World!");

        let mut old_file1_contents = String::new();
        let mut old_file1 = File::open(tempdir.path().join("file1.txt")).unwrap();
        old_file1.read_to_string(&mut old_file1_contents).unwrap();
        assert_eq!(old_file1_contents, "");
    }

    #[cfg(any(all(unix, feature = "libc_on_unix"), all(target_os = "windows", feature = "libc_on_windows")))]
    #[test]
    fn redirects_file_to_path() {
        // Arrange
        let tempdir = tempfile::tempdir().unwrap();
        let src_path = tempdir.path().join("src.txt");
        let dst_path = tempdir.path().join("dst.txt");
        let mut src = OpenOptions::new().create(true).read(true).write(true).open(&src_path).unwrap();

        // Act
        src.redirect(dst_path.as_path()).unwrap();
        src.write_all(b"abc").unwrap();
        src.flush().unwrap();

        // Assert
        let mut dst_contents = String::new();
        File::open(&dst_path).unwrap().read_to_string(&mut dst_contents).unwrap();
        assert_eq!(dst_contents, "abc");

        let mut original_contents = String::new();
        File::open(&src_path).unwrap().read_to_string(&mut original_contents).unwrap();
        assert_eq!(original_contents, "");
    }

    #[cfg(any(all(unix, feature = "libc_on_unix"), all(target_os = "windows", feature = "libc_on_windows")))]
    #[test]
    fn errors_on_redirect_to_directory() {
        // Arrange
        let tempdir = tempfile::tempdir().unwrap();
        let dir_path = tempdir.path();
        let mut src = File::create(dir_path.join("somefile.txt")).unwrap();

        // Act
        let err = src.redirect(dir_path).unwrap_err();

        // Assert
        assert!(err.raw_os_error().is_some());
    }

    #[cfg(any(all(unix, feature = "libc_on_unix"), all(target_os = "windows", feature = "libc_on_windows")))]
    #[test]
    fn errors_on_redirect_with_missing_parent_directory() {
        // Arrange
        let tempdir = tempfile::tempdir().unwrap();
        let mut src = File::create(tempdir.path().join("s.txt")).unwrap();
        let bad_path = tempdir.path().join("no_such_dir").join("f.txt");

        // Act
        let err = src.redirect(bad_path.as_path()).unwrap_err();

        // Assert
        assert!(err.raw_os_error().is_some());
    }

    #[cfg(any(all(unix, feature = "libc_on_unix")))]
    #[test]
    fn errors_on_redirect_to_closed_fd() {
        use std::os::fd::AsRawFd;
        // Arrange
        let tempdir = tempfile::tempdir().unwrap();
        let mut src_file = File::create(tempdir.path().join("src.txt")).unwrap();
        let dst_file = File::create(tempdir.path().join("dst.txt")).unwrap();

        let dst_file = ManuallyDrop::new(dst_file);
        let fd = dst_file.as_raw_fd();
        unsafe { close(fd) };

        // Act
        let err = src_file.redirect(&*dst_file).unwrap_err();

        // Assert
        assert!(err.raw_os_error().is_some());
    }
}