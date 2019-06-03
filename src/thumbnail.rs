use fs2::FileExt;
use image;
use std::fs;
use std::path::PathBuf;

pub fn make(file_path: &str) {
    log::trace!("make(\"{}\") ...", &file_path);

    let mut file_path: PathBuf = file_path.into();

    let img = {
        let file = fs::OpenOptions::new().read(true).open(&file_path);
        if let Err(e) = &file {
            log::warn!(
                "I/O ERROR \"{}\" while {} file opening for read!",
                e.to_string(),
                &file_path.to_string_lossy()
            );
            return;
        }
        let file = file.unwrap();

        let lock = file.lock_shared();
        if let Err(e) = &lock {
            log::warn!(
                "I/O ERROR \"{}\" while attempt to place shared lock on {} file!",
                e.to_string(),
                &file_path.to_string_lossy()
            );
            return;
        }

        if let Ok(data) = image::open(&file_path) {
            let lock = file.unlock();
            if let Err(e) = &lock {
                log::warn!(
                    "I/O ERROR \"{}\" while attempt to free shared lock on {} file!",
                    e.to_string(),
                    &file_path.to_string_lossy()
                );
            }
            data
        } else {
            let lock = file.unlock();
            if let Err(e) = &lock {
                log::warn!(
                    "I/O ERROR \"{}\" while attempt to free shared lock on {} file!",
                    e.to_string(),
                    &file_path.to_string_lossy()
                );
            }
            return;
        }
    };

    let thumbnail = img.resize_to_fill(100, 100, image::FilterType::Lanczos3);

    let file = file_path.file_name().unwrap().to_os_string();
    file_path.pop();
    file_path.push("thumbnails");

    match fs::create_dir_all(&file_path) {
        Ok(_) => file_path.push(file),

        Err(e) => {
            log::warn!(
                "I/O ERROR \"{}\" while attempt to create directory {}!",
                e.to_string(),
                &file_path.to_string_lossy()
            );
            return;
        }
    }

    if let Err(e) = thumbnail.save(&file_path) {
        log::warn!(
            "I/O ERROR \"{}\" while saving thumbnail to file {}!",
            e.to_string(),
            &file_path.to_string_lossy()
        );
    }

    log::debug!("make => {}", file_path.to_string_lossy());
}
