use std::io;

pub trait Redirectable<T: ?Sized>
{
    fn redirect(&self, destination: &T) -> io::Result<()>;
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
        fn redirect(&self, destination: &T2) -> io::Result<()> {
            let src_fd = self.as_raw_fd();
            let dst_fd = destination.as_raw_fd();
            return crate::common::redirect_fd_to_fd(src_fd, dst_fd);
        }
    }
}

#[cfg(any(target_os = "windows"))]
mod platform
{
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use libc::{c_int, open_osfhandle};

    pub type Descriptor = c_int;

    pub trait Descriptable: AsRawHandle {}
    impl<T: AsRawHandle> Descriptable for T {}

    impl<T1: Descriptable, T2: Descriptable> Redirectable<T2> for T1 {
        fn redirect(&self, destination: &T2) -> io::Result<()> {
            let src_handle = self.as_raw_handle();
            let dst_handle = destination.as_raw_handle();

            let src_fd = unsafe { open_osfhandle(src_handle as isize, 0) };
            if src_fd < 0 {
                return Err(io::Error::last_os_error());
            }

            let dst_fd = unsafe { open_osfhandle(dst_handle as isize, 0) };
            if dst_fd < 0 {
                return Err(io::Error::last_os_error());
            }

            return crate::common::redirect_fd_to_fd(src_fd, dst_fd);
        }
    }
}

#[cfg(any(unix, target_os = "windows"))]
mod common
{
    use super::*;
    use libc::dup2;
    use crate::platform::Descriptor;

    pub fn redirect_fd_to_fd(src: Descriptor, dst: Descriptor) -> io::Result<()> {
        let result = unsafe {
            dup2(dst, src)
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }

        return Ok(());
    }
}

#[cfg(any(unix, target_os = "windows"))]
mod convenience
{
    use super::*;
    use std::path::Path;
    use std::fs::OpenOptions;
    use std::io::{stderr, stdout};

    impl<T: Descriptable> Redirectable<Path> for T {
        fn redirect(&self, destination: &Path) -> io::Result<()> {
            let dst = OpenOptions::new().read(false).write(true).create(true).open(destination)?;
            let result = self.redirect(&dst);
            if result.is_ok() {
                std::mem::forget(dst);
            }
            return result;
        }
    }

    pub fn redirect_std_to_path(destination: &Path) -> io::Result<()> {
        let dst = OpenOptions::new().read(false).write(true).create(true).open(destination)?;
        stdout().redirect(&dst)?;
        stderr().redirect(&dst)?;
        std::mem::forget(dst);
        return Ok(());
    }
}

pub use platform::*;
pub use convenience::*;

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::{Read, Write};
    use super::*;

    #[test]
    fn redirects() {
        // Arrange
        let tempdir = tempfile::tempdir().unwrap();
        let mut file1 = File::create(tempdir.path().join("file1.txt")).unwrap();
        let mut file2 = File::create(tempdir.path().join("file2.txt")).unwrap();

        // Act
        file1.redirect(&file2).unwrap();
        file1.write_all(b"Hello,").unwrap();
        file2.write_all(b" World!").unwrap();

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
}
