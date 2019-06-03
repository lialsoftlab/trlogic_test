use chrono::prelude::*;
use fs2::FileExt;
use std::fs;
use std::io;
use std::path::Path;

/// Try to normalize specified image filename with respect of mime type.
///
/// Attempts to normalize the specified image filename; if it is partial – infering
/// extension from the specified MIME type.
/// If the filename is missing, it generates stub — also with inferred extension.
///
/// # Examples
///
/// ```rust
///     use regex::Regex;
///     use trlogic_test::file_utils;
///     
///     assert_eq!(
///         &file_utils::normalize_image_filename("concrete.jpg", "image/jpeg"),
///         "concrete.jpg"
///     );
///
///     assert_eq!(
///         &file_utils::normalize_image_filename("partial", "image/jpeg"),
///         "partial.jpg"
///     );
///
///     let re = Regex::new(r"^untitled@\d{18}\.jpg$").unwrap();
///     assert!(re.is_match(&file_utils::normalize_image_filename("", "image/jpeg")));
///
///     let re = Regex::new(r"^untitled@\d{18}\.bin$").unwrap();
///     assert!(re.is_match(&file_utils::normalize_image_filename("", "")));
/// ```
pub fn normalize_image_filename(filename: &str, content_type: &str) -> String {
    log::trace!(
        "normalize_image_filename(\"{}\", \"{}\") ...",
        &filename,
        &content_type
    );
    fn ext_for(content_type: &str) -> &str {
        let pair: Vec<&str> = content_type.split("/").collect();
        if content_type.to_lowercase().starts_with("image/") && pair.len() == 2 {
            match pair[1] {
                "jpeg" => "jpg",
                "pjpeg" => "jpg",
                "svg+xml" => "svg",
                "tiff" => "tif",
                "vnd.microsoft.icon" => "ico",
                "vnd.wap.wbmp" => "wbmp",
                "*" => "bin",
                x => x,
            }
        } else {
            "bin"
        }
    }

    let result = if filename.trim().len() == 0 {
        format!(
            "untitled@{}.{}",
            Utc::now().format("%y%m%d%H%M%S%6f"),
            ext_for(content_type)
        )
    } else if !filename.contains('.') {
        format!("{}.{}", filename, ext_for(content_type))
    } else {
        filename.to_string()
    };

    log::debug!(
        "normalize_image_filename(\"{}\", \"{}\") => \"{}\"",
        &filename,
        &content_type,
        &result
    );
    result
}

/// Save image data to disk storage.
///
/// Saves image data to the specified path on the disk storage.
/// An exclusive file lock is acquired before attempting to save data, and released
/// immediately after save to prevent data corruption in case of concurrent access.
///
/// # Examples
///
/// ```rust
///    use trlogic_test::file_utils;
///
///    let mut file_path = std::env::temp_dir();
///    file_path.push("image.bin"); // target file path
///
///    let image_data = vec![0x0F; 1024]; // any source implementing Read trait
///
///    assert_eq!(
///        file_utils::write_image_data(&mut &image_data[..], &file_path).unwrap(),
///        image_data.len() as u64
///    );
/// ```
pub fn write_image_data<R: io::Read>(mut source: R, target: &Path) -> io::Result<u64> {
    log::trace!("write_image_data(R, \"{}\") ...", target.display());

    let file = fs::OpenOptions::new().write(true).create(true).open(target);
    if let Err(e) = &file {
        log::warn!(
            "I/O ERROR \"{}\" while {} file opening for write!",
            e.to_string(),
            &target.to_string_lossy()
        );
    };
    let mut file = file?;

    let lock = file.lock_exclusive();
    if let Err(e) = &lock {
        log::warn!(
            "I/O ERROR \"{}\" while attempt to place exclusive lock on {} file!",
            e.to_string(),
            &target.to_string_lossy()
        );
    }
    let _ = lock?;

    file.set_len(0)?;

    let result = io::copy(&mut source, &mut file);

    if let Err(e) = &result {
        log::warn!(
            "I/O ERROR: \"{}\" while saving image data to {} file!",
            e,
            target.display()
        );
    }

    let lock = file.unlock();
    if let Err(e) = &lock {
        log::warn!(
            "I/O ERROR \"{}\" while attempt to free exclusive lock on {} file!",
            e.to_string(),
            &target.to_string_lossy()
        );
    }

    log::debug!(
        "write_image_data(R, \"{}\") => {:?}",
        target.display(),
        result
    );
    result
}

#[cfg(test)]
mod tests {
    use regex::Regex;
    use std::fs;
    use std::io::Read;
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::thread;

    #[test]
    fn test_normalize_image_filename() {
        // Test for empty filename.
        let re = Regex::new(r"^untitled@\d{2}\d{2}\d{2}\d{2}\d{2}\d{2}\d{6}\.bin$").unwrap();
        assert!(re.is_match(&super::normalize_image_filename("", "")));

        // Test for filename without extension specified (try to infer from mime-type).
        let filename = "placeholder";
        let mime = "image/jpeg";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "placeholder.jpg"
        );
        let mime = "image/pjpeg";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "placeholder.jpg"
        );
        let mime = "image/svg+xml";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "placeholder.svg"
        );
        let mime = "image/tiff";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "placeholder.tif"
        );
        let mime = "image/vnd.microsoft.icon";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "placeholder.ico"
        );
        let mime = "image/vnd.wap.wbmp";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "placeholder.wbmp"
        );
        let mime = "image/*";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "placeholder.bin"
        );
        let mime = "image/~unknown~";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "placeholder.~unknown~"
        );
        let mime = "text/plain";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "placeholder.bin"
        );

        // Test for valid filename (must pass without modifications).
        let filename = "valid.png";
        let mime = "image/jpeg";
        assert_eq!(
            super::normalize_image_filename(&filename, mime),
            "valid.png"
        );
    }

    #[test]
    fn test_write_image_data() {
        let mut file_path = std::env::temp_dir();
        file_path.push("test.bin");
        let file_path = Arc::new(file_path);

        // Testing for simple saving.
        let sample = Arc::new((0..256).map(|x| x as u8).collect::<Vec<u8>>());
        assert_eq!(
            super::write_image_data(&mut &sample[..], &file_path).unwrap(),
            sample.len() as u64
        );

        let mut buffer = Vec::new();
        {
            let mut file = fs::File::open(file_path.as_path()).unwrap();
            file.read_to_end(&mut buffer)
                .expect("Can't read sample data from the test file!");
            assert_eq!(sample[..], buffer[..]);
        }

        // Testing for concurent saving with collision.
        let mut slave = vec![];

        let mutex1 = Arc::new(Mutex::new(()));
        let mutex2 = Arc::new(Mutex::new(()));

        let guard1 = mutex1.lock().unwrap();
        let guard2 = mutex2.lock().unwrap();

        let sample = Arc::new(vec![0xA5u8; 5 * 1024 * 1024]);
        {
            let sample = Arc::clone(&sample);
            let file_path = Arc::clone(&file_path);
            let mutex1 = mutex1.clone();

            slave.push(thread::spawn(move || {
                let _ = mutex1.lock().unwrap();

                assert_eq!(
                    super::write_image_data(&mut &sample[..], &file_path).unwrap(),
                    sample.len() as u64
                );
            }));
        }

        let sample2 = Arc::new(vec![0x5Au8; 1 * 1024 * 1024]);
        {
            let sample2 = Arc::clone(&sample2);
            let file_path = Arc::clone(&file_path);
            let mutex2 = mutex2.clone();

            slave.push(thread::spawn(move || {
                let _ = mutex2.lock().unwrap();

                assert_eq!(
                    super::write_image_data(&mut &sample2[..], &file_path).unwrap(),
                    sample2.len() as u64
                );
            }));
        }

        drop(guard1);
        drop(guard2);

        for x in slave {
            x.join().expect("Panic in a children thread!");
        }

        buffer.truncate(0);
        {
            let mut file = fs::File::open(file_path.as_path()).unwrap();
            file.read_to_end(&mut buffer)
                .expect("Can't read sample data from the test file!");
            assert!(&sample[..] == &buffer[..] || &sample2[..] == &buffer[..]);
        }
    }
}
