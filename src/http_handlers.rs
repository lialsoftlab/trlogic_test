use base64;
use mrq;
use multipart::server::{FieldHeaders, Multipart, MultipartData, MultipartField};
use rouille::input::multipart::get_multipart_input;
use rouille::{router, try_or_400};
use rouille::{Request, Response};
use serde_derive::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;

use super::file_utils;
use super::thumbnail;

/// Top level HTTP request router.
pub fn route(request: &Request, file_path: &str) -> Response {
    log::trace!("route({:?}) ...", request);

    let response = router!(request,
        (GET) (/images) => {
            handle_images_json_get(file_path)
        },

        (POST) (/images) => {
            route_images_post_by_content_type(request, file_path)
        },

        _ => rouille::Response::empty_404()
    );

    log::info!(
        "{} \"{}{}\" {} {}",
        request.method(),
        request.header("Host").unwrap_or(""),
        request.url(),
        request.remote_addr(),
        response.status_code
    );
    log::debug!("route({:?}) => {:?}", request, response);
    response
}

///Get response with sorted image files list in json array.
pub fn handle_images_json_get(file_path: &str) -> Response {
    log::trace!("handle_images_json_get...");

    let mut dir_list = match std::fs::read_dir(&file_path) {
        Ok(x) => x.map(|x| x.unwrap().file_name()).collect::<Vec<_>>(),

        Err(e) => {
            log::warn!(
                "I/O ERROR: \"{}\" while reading directory {}",
                e.to_string(),
                file_path
            );
            panic!(e);
        }
    };
    dir_list[..].sort();

    let mut files_list = Vec::new();

    for item in dir_list {
        match item.into_string() {
            Ok(filename) => files_list.push(filename),
            Err(filename) => log::warn!("UTF-8 incompatible file name {:?} is ignored", filename),
        }
    }

    let response = Response::json(&files_list);
    log::debug!("handle_images_json_get => {:?}", &files_list[..]);
    response
}

/// Route a HTTP POST request with respect to the Content-Type header.
///
/// Attempts to route a POST request to resource with respect to the Content-Type
/// header, acceptable types are "application/json" and "multipart/form-data".
/// If any other type is specified – returns a HTTP 406 "Not Acceptable" error response.
/// If Content-Type isn't specified – returns a HTTP 400 "Bad Request" error response.
pub fn route_images_post_by_content_type(request: &Request, file_path: &str) -> Response {
    match request.header("Content-Type") {
        Some(content_type) => match &content_type
            .to_lowercase()
            .split(';')
            .collect::<Vec<&str>>()[0][..]
        {
            "application/json" => handle_json_images_post(request, file_path),
            "multipart/form-data" => handle_multipart_images_post(request, file_path),
            _ => Response::empty_406(),
        },
        None => Response::empty_400(),
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ImageUploadResult {
    pub filename: String,
    pub content_type: String,
    pub size: u64,
    pub success: bool,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
struct ImageUploadRequest {
    filename: Option<String>,
    content_type: Option<String>,
    url: Option<String>,
    data: Option<String>,
}

/// Handle a request with a body containing JSON with an array of base64-encoded images
/// or URLs to download.
///
/// Handles a request with a body containg JSON with an array of base64-encoded images
/// or URLS to download, saving valid images to disk storage.
/// Returning JSON array with info about successfully saved images.
/// In case of severe errors returns a HTTP 400 Bad request error.
pub fn handle_json_images_post(request: &Request, file_path: &str) -> Response {
    log::trace!("handle_json_images_post...");

    let upload_requests: Vec<ImageUploadRequest> = try_or_400!(rouille::input::json_input(request));
    log::debug!("upload_requests = {:?}", upload_requests);

    let mut results = Vec::<ImageUploadResult>::new();
    let mut file_path: PathBuf = [file_path, "placeholder.bin"].iter().collect();

    for mut item in upload_requests {
        let image_from = if item.data.is_some() {
            image_from_base64_data
        } else if item.url.is_some() {
            image_from_url
        } else {
            results.push(ImageUploadResult {
                filename: item.filename.unwrap_or_else(String::new),
                content_type: item.content_type.unwrap_or_else(String::new),
                size: 0,
                success: false,
                reason: String::from("nor url or data are specified"),
            });

            continue;
        };

        match image_from(&mut item) {
            Ok((filename, content_type, data)) => {
                file_path.set_file_name(&filename);

                let (success, size, err) = file_utils::write_image_data(&data[..], &file_path)
                    .and_then(|size| Ok((true, size, "ok")))
                    .or_else::<(), _>(|_| Ok((false, 0, "I/O error")))
                    .unwrap();

                results.push(ImageUploadResult {
                    filename,
                    content_type: content_type,
                    size,
                    success,
                    reason: err.to_string(),
                });

                {
                    let file_path = file_path.clone();
                    std::thread::spawn(move || {
                        thumbnail::make(&file_path.to_string_lossy());
                    });
                }
            }
            Err(e) => {
                results.push(ImageUploadResult {
                    filename: item.filename.unwrap_or_else(String::new),
                    content_type: item.content_type.unwrap_or_else(String::new),
                    size: 0,
                    success: false,
                    reason: e.to_string(),
                });
            }
        };
    }

    log::debug!("handle_json_images_post => results = {:?}", results);
    Response::json(&results)
}

/// Handle a multipart request with body containing binary images data array.
///
/// Handles a request body parts containing MIME of "image/*" type, other
/// parts are skipped, saving valid images to disk storage.
/// Returning JSON array with info about successfully saved images.
/// In case of severe errors returns a HTTP 400 Bad request error.
pub fn handle_multipart_images_post(request: &Request, file_path: &str) -> Response {
    log::trace!("handle_multipart_images_post...");
    let mut multipart_items = match get_multipart_input(request) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("Multipart data parsing error: {}", e.to_string());
            return Response::empty_400();
        }
    };

    let mut results = Vec::<ImageUploadResult>::new();
    let mut file_path: PathBuf = [file_path, "placeholder.bin"].iter().collect();

    while let Some(mut item) = multipart_items.next() {
        match image_from_multipart_field(&mut item) {
            Ok((filename, content_type, data)) => {
                file_path.set_file_name(&filename);

                let (success, size, err) = file_utils::write_image_data(data, &file_path)
                    .and_then(|size| Ok((true, size, "ok")))
                    .or_else::<(), _>(|_| Ok((false, 0, "I/O error")))
                    .unwrap();

                results.push(ImageUploadResult {
                    filename,
                    content_type: content_type,
                    size,
                    success,
                    reason: err.to_string(),
                });

                {
                    let file_path = file_path.clone();
                    std::thread::spawn(move || {
                        thumbnail::make(&file_path.to_string_lossy());
                    });
                }
            }
            Err((headers, err)) => {
                results.push(ImageUploadResult {
                    filename: headers.name.to_string(),
                    content_type: headers
                        .content_type
                        .clone()
                        .map(|x| x.to_string())
                        .unwrap_or(String::new()),
                    size: 0,
                    success: false,
                    reason: err.to_string(),
                });
            }
        }
    }

    log::debug!("handle_multipart_images_post => results = {:?}", results);
    Response::json(&results)
}

/// Decode an image from multipart/form-data field and
/// return a (filename, content-type, image-data-reader) tuple.
fn image_from_multipart_field<'a, 'b>(
    item: &'b mut MultipartField<&'b mut Multipart<rouille::RequestBody<'a>>>,
) -> Result<
    (
        String,
        String,
        &'b mut MultipartData<&'b mut Multipart<rouille::RequestBody<'a>>>,
    ),
    (&'b FieldHeaders, &'b str),
> {
    log::trace!("image_from_multipart_field...");
    let headers = &item.headers;

    if let Some(content_type) = &headers.content_type {
        let content_type = content_type.to_string();
        if content_type.starts_with("image/") {
            let filename = if let Some(filename) = &headers.filename {
                &filename[..]
            } else {
                &headers.name[..]
            };
            let filename = file_utils::normalize_image_filename(&filename, &content_type);

            log::debug!(
                "image_from_multipart_field => (\"{}\", \"{}\", _)",
                filename,
                content_type
            );
            return Ok((filename, content_type, &mut item.data));
        }
    }

    log::debug!("image_from_multipart_field => Err(_, \"no image data\")");
    Err((headers, "no image data"))
}

/// Decode a base64-encoded image data and
/// return a (filename, content-type, image-data-reader) tuple.
fn image_from_base64_data(
    item: &mut ImageUploadRequest,
) -> Result<(String, String, Vec<u8>), String> {
    log::trace!("image_from_base64_data...");

    if let Some(data) = &item.data {
        let content_type = item
            .content_type
            .take()
            .unwrap_or(String::from("application/octet-stream"));
        let filename = if let Some(filename) = &item.filename {
            file_utils::normalize_image_filename(&filename, &content_type)
        } else {
            file_utils::normalize_image_filename("", &content_type)
        };

        return match base64::decode(&data) {
            Ok(data) => {
                log::debug!(
                    "image_from_base64_data => (\"{}\", \"{}\", _)",
                    filename,
                    content_type
                );
                Ok((filename, content_type, data))
            }

            Err(e) => {
                let e = e.to_string();
                log::debug!("image_from_base64_data => Err({})", e);
                Err(e)
            }
        };
    }

    log::debug!("image_from_base64_data => Err(\"no image data\")");
    Err("no image data".to_string())
}

/// Download an image specified by URL to buffer and
/// return a (filename, content-type, image-data-reader) tuple.
fn image_from_url(item: &mut ImageUploadRequest) -> Result<(String, String, Vec<u8>), String> {
    log::trace!("image_from_url...");

    if let Some(url) = &item.url {
        match mrq::get(&url[..]).send() {
            Ok(mut response) => {
                let content_type = response
                    .headers
                    .get("Content-Type")
                    .unwrap_or(&item.content_type.take().unwrap_or_else(String::new))
                    .to_string();
                if content_type.starts_with("image/") {
                    if let Some(content_length) = response.headers.get("Content-Length") {
                        if let Ok(content_length) = content_length.parse::<usize>() {
                            let filename = if let Some(filename) = &item.filename {
                                file_utils::normalize_image_filename(&filename, &content_type)
                            } else {
                                file_utils::normalize_image_filename(
                                    &url.split('/').last().unwrap_or("").to_string(),
                                    &content_type,
                                )
                            };

                            let mut buffer = vec![0u8; content_length];

                            let result = response
                                .body
                                .read_exact(&mut buffer[..])
                                .and_then(|_| Ok((filename, content_type, buffer)))
                                .or_else(|e| Err(e.to_string()));

                            match &result {
                                Ok((filename, content_type, _)) => log::debug!(
                                    "image_from_url(\"{}\") => Ok((\"{}\", \"{}\"))",
                                    url,
                                    filename,
                                    content_type
                                ),
                                Err(e) => {
                                    log::debug!("image_from_url(\"{}\") => Err(\"{}\")", url, e)
                                }
                            }

                            return result;
                        }
                    }
                    let e = String::from("invalid content length in response");
                    log::debug!("image_from_url(\"{}\") => Err(\"{}\")", url, e);
                    return Err(e);
                }
                let e = String::from("not an image");
                log::debug!("image_from_url(\"{}\") => Err(\"{}\")", url, e);
                return Err(e);
            }
            Err(e) => {
                let e = e.to_string();
                log::debug!("image_from_url(\"{}\") => Err(\"{}\")", url, e);
                return Err(e);
            }
        }
    }

    let e = String::from("image URL not specified");
    log::debug!("image_from_url => Err(\"{}\")", e);
    Err(e)
}

#[cfg(test)]
mod tests {
    use image::ImageDecoder;
    use rouille::input::multipart::get_multipart_input;
    use std::io::Read;

    #[test]
    fn test_image_from_multipart_field() {
        let http_rq = mock::multipart_formdata_request();
        let mut multipart_items = get_multipart_input(&http_rq).unwrap();

        let mut item = multipart_items.next().unwrap();
        let (filename, content_type, data) = super::image_from_multipart_field(&mut item).unwrap();
        assert_eq!(filename, "sample.jpg");
        assert_eq!(content_type, "image/jpeg");
        let mut buffer = [0u8; 15];
        data.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"JPEG IMAGE DATA");

        let mut item = multipart_items.next().unwrap();
        let (filename, content_type, data) = super::image_from_multipart_field(&mut item).unwrap();
        assert_eq!(filename, "file-from-name.png");
        assert_eq!(content_type, "image/png");
        let mut buffer = [0u8; 14];
        data.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"PNG IMAGE DATA");

        let mut item = multipart_items.next().unwrap();
        match super::image_from_multipart_field(&mut item) {
            Err((_, msg)) => assert_eq!(msg, "no image data"),
            _ => panic!("text/plain is not an image!"),
        }
    }

    #[test]
    fn test_image_from_base64_data() {
        let mut uprq = super::ImageUploadRequest {
            content_type: None,
            data: None,
            filename: None,
            url: None,
        };

        match super::image_from_base64_data(&mut uprq) {
            Err(e) => assert_eq!(e, "no image data"),
            _ => panic!("data == None isn't an image!"),
        }

        uprq.data = Some(String::from("VEVTVCBKUEVHIERBVEE="));

        let (filename, content_type, data) = super::image_from_base64_data(&mut uprq).unwrap();
        assert_eq!(data, b"TEST JPEG DATA".to_vec());
        assert!(filename.starts_with("untitled@") && filename.ends_with(".bin"));
        assert_eq!(content_type, "application/octet-stream");

        uprq.filename = Some(String::from("test.jpg"));

        let (filename, content_type, data) = super::image_from_base64_data(&mut uprq).unwrap();
        assert_eq!(data, b"TEST JPEG DATA".to_vec());
        assert_eq!(filename, "test.jpg");
        assert_eq!(content_type, "application/octet-stream");

        uprq.content_type = Some(String::from("image/jpeg"));

        let (filename, content_type, data) = super::image_from_base64_data(&mut uprq).unwrap();
        assert_eq!(data, b"TEST JPEG DATA".to_vec());
        assert_eq!(filename, "test.jpg");
        assert_eq!(content_type, "image/jpeg");
    }

    #[test]
    fn test_image_from_url() {
        let mut uprq = super::ImageUploadRequest {
            content_type: None,
            data: None,
            filename: None,
            url: None,
        };

        match super::image_from_url(&mut uprq) {
            Err(e) => assert_eq!(e, "image URL not specified"),
            _ => panic!("url == None isn't an image!"),
        }

        uprq.url = Some(String::from("https://ya.ru"));

        match super::image_from_url(&mut uprq) {
            Err(e) => assert_eq!(e, "not an image"),
            _ => panic!("url pointing to html page isn't an image!"),
        }

        uprq.url = Some(String::from("https://placehold.co/321/png"));

        let (filename, content_type, data) = super::image_from_url(&mut uprq).unwrap();
        assert_eq!(filename, "png.png");
        assert_eq!(content_type, "image/png");
        let img = image::png::PNGDecoder::new(&data[..]).unwrap();
        assert_eq!(img.dimensions(), (321, 321));

        uprq.url = Some(String::from("https://via.placeholder.com/123.jpg"));

        let (filename, content_type, data) = super::image_from_url(&mut uprq).unwrap();
        assert_eq!(filename, "123.jpg");
        assert_eq!(content_type, "image/jpeg");
        let img = image::jpeg::JPEGDecoder::new(&data[..]).unwrap();
        assert_eq!(img.dimensions(), (123, 123));
    }

    #[test]
    fn test_image_from_url_self_hosted() {
        let mut uprq = super::ImageUploadRequest {
            content_type: None,
            data: None,
            filename: None,
            url: None,
        };

        match super::image_from_url(&mut uprq) {
            Err(e) => assert_eq!(e, "image URL not specified"),
            _ => panic!("url == None isn't an image!"),
        }

        uprq.url = Some(String::from(
            "http://qerqcqwer3454fdsgdfgsdfg/not-exist-server",
        ));

        if let Ok(_) = super::image_from_url(&mut uprq) {
            panic!("url pointing to not existent server isn't an image!")
        }

        let (join_handle, srv_tx) = mock::test_http_server(8888);
        std::thread::sleep(std::time::Duration::from_secs(2)); // Await for server warm-up or else may false test failures araise.

        uprq.url = Some(String::from("http://localhost:8888/not-exist-url"));

        match super::image_from_url(&mut uprq) {
            Err(e) => assert_eq!(e, "not an image"),
            _ => panic!("url pointing to invalid resource isn't an image!"),
        }

        uprq.url = Some(String::from("http://localhost:8888/unknown-content-type"));

        match super::image_from_url(&mut uprq) {
            Err(e) => assert_eq!(e, "not an image"),
            _ => panic!("url pointing to a resource with unknown Content-Type isn't an image!"),
        }

        uprq.content_type = Some(String::from("image/jpeg"));

        let (filename, content_type, data) = super::image_from_url(&mut uprq).unwrap();
        assert_eq!(filename, "unknown-content-type.jpg");
        assert_eq!(content_type, "image/jpeg");
        assert_eq!(data, b"Hello!");

        uprq.url = Some(String::from("http://localhost:8888/"));

        match super::image_from_url(&mut uprq) {
            Err(e) => assert_eq!(e, "not an image"),
            _ => panic!("url pointing to html or text resource isn't an image!"),
        }

        uprq.content_type = None;
        uprq.url = Some(String::from("http://localhost:8888/image"));

        let (filename, content_type, data) = super::image_from_url(&mut uprq).unwrap();
        assert_eq!(filename, "image.jpg");
        assert_eq!(content_type, "image/jpeg");
        assert_eq!(data, b"TEST JPEG DATA");

        uprq.filename = Some(String::from("testfile"));

        let (filename, _, _) = super::image_from_url(&mut uprq).unwrap();
        assert_eq!(filename, "testfile.jpg");

        uprq.filename = Some(String::from("testfile.jpeg"));

        let (filename, _, _) = super::image_from_url(&mut uprq).unwrap();
        assert_eq!(filename, "testfile.jpeg");

        srv_tx.send("stop").unwrap();
        join_handle.join().unwrap();
    }

    #[test]
    fn test_handle_multipart_images_post() {
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push("test-multipart-ghdfhfgh4");
        let _ = std::fs::remove_dir_all(&tmp_path);
        std::fs::create_dir_all(&tmp_path).unwrap();

        let http_rq = mock::multipart_formdata_request();

        super::handle_multipart_images_post(&http_rq, &tmp_path.to_string_lossy());

        let mut dir_list = std::fs::read_dir(&tmp_path)
            .unwrap()
            .map(|x| x.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(dir_list.len(), 2);
        dir_list[..].sort();
        assert_eq!(dir_list[0].to_str(), Some("file-from-name.png"));
        assert_eq!(dir_list[1].to_str(), Some("sample.jpg"));

        std::fs::remove_dir_all(&tmp_path).unwrap();
    }

    #[test]
    fn handle_json_images_post() {
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push("test-json-qwere234erfvdf");
        let _ = std::fs::remove_dir_all(&tmp_path);
        std::fs::create_dir_all(&tmp_path).unwrap();

        let (join_handle, srv_tx) = mock::test_http_server(8889);
        std::thread::sleep(std::time::Duration::from_secs(2)); // Await for server warm-up or else may false test failures araise.

        let http_rq = mock::json_request(8889);

        let (reader, _) = super::handle_json_images_post(&http_rq, &tmp_path.to_string_lossy())
            .data
            .into_reader_and_size();
        let results: Vec<super::ImageUploadResult> = serde_json::from_reader(reader).unwrap();

        assert_eq!(results[0].success, false);
        assert_eq!(results[1].success, true);
        assert_eq!(results[2].success, false);
        assert_eq!(results[3].success, true);

        let mut dir_list = std::fs::read_dir(&tmp_path)
            .unwrap()
            .map(|x| x.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(dir_list.len(), 2);
        dir_list[..].sort();
        assert_eq!(dir_list[0].to_str(), Some("image.jpg"));
        assert_eq!(dir_list[1].to_str(), Some("valid_base64.bin"));

        for i in 0..dir_list.len() {
            let mut buffer = String::new();
            tmp_path.push(dir_list[i].to_str().unwrap());
            let mut f = std::fs::File::open(&tmp_path).unwrap();
            f.read_to_string(&mut buffer).unwrap();
            assert_eq!(
                buffer,
                "TEST JPEG DATA",
                "filename = {}",
                dir_list[i].to_str().unwrap()
            );
            tmp_path.pop();
        }

        srv_tx.send("stop").unwrap();
        join_handle.join().unwrap();

        std::fs::remove_dir_all(&tmp_path).unwrap();
    }

    #[test]
    fn test_route_images_post_by_content_type() {
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push("test-route-cvbcvbngfh");
        let _ = std::fs::remove_dir_all(&tmp_path);
        std::fs::create_dir_all(&tmp_path).unwrap();

        let (join_handle, srv_tx) = mock::test_http_server(8890);
        std::thread::sleep(std::time::Duration::from_secs(2)); // Await for server warm-up or else may false test failures araise.

        let http_rq = mock::json_request(8890);

        let response =
            super::route_images_post_by_content_type(&http_rq, &tmp_path.to_string_lossy());
        assert_eq!(response.status_code, 200);

        srv_tx.send("stop").unwrap();
        join_handle.join().unwrap();

        std::fs::remove_dir_all(&tmp_path).unwrap();
        std::fs::create_dir_all(&tmp_path).unwrap();
        let http_rq = mock::multipart_formdata_request();

        let response =
            super::route_images_post_by_content_type(&http_rq, &tmp_path.to_string_lossy());
        assert_eq!(response.status_code, 200);

        std::fs::remove_dir_all(&tmp_path).unwrap();
        std::fs::create_dir_all(&tmp_path).unwrap();
        let http_rq = mock::plaintext_request();

        let response =
            super::route_images_post_by_content_type(&http_rq, &tmp_path.to_string_lossy());
        assert_eq!(response.status_code, 406);

        std::fs::remove_dir_all(&tmp_path).unwrap();
        std::fs::create_dir_all(&tmp_path).unwrap();
        let http_rq = mock::unknown_content_request();

        let response =
            super::route_images_post_by_content_type(&http_rq, &tmp_path.to_string_lossy());
        assert_eq!(response.status_code, 400);

        std::fs::remove_dir_all(&tmp_path).unwrap();
    }

    #[test]
    fn test_handle_images_json_get() {
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push("test-get-ckenvthslc");
        let _ = std::fs::remove_dir_all(&tmp_path);
        std::fs::create_dir_all(&tmp_path).unwrap();

        let response = super::handle_images_json_get(&tmp_path.to_string_lossy());
        assert_eq!(response.status_code, 200);
        let (reader, _) = response.data.into_reader_and_size();
        let results: Vec<String> = serde_json::from_reader(reader).unwrap();
        assert_eq!(results.len(), 0);

        {
            tmp_path.push("test.jpg");
            std::fs::File::create(&tmp_path).unwrap();
            tmp_path.pop();

            tmp_path.push("test.png");
            std::fs::File::create(&tmp_path).unwrap();
            tmp_path.pop();
        }

        let response = super::handle_images_json_get(&tmp_path.to_string_lossy());
        assert_eq!(response.status_code, 200);
        let (reader, _) = response.data.into_reader_and_size();
        let results: Vec<String> = serde_json::from_reader(reader).unwrap();
        assert_eq!(results, ["test.jpg", "test.png"]);

        assert_eq!(response.status_code, 200);

        std::fs::remove_dir_all(&tmp_path).unwrap();
    }

    mod mock {
        use rouille::router;
        use std::sync::mpsc;
        use std::thread;

        pub fn test_http_server(port: u16) -> (thread::JoinHandle<()>, mpsc::Sender<&'static str>) {
            let (srv_tx, srv_rx) = mpsc::channel::<&str>();
            let join_handle = thread::spawn(move || {
                let server = rouille::Server::new(format!("localhost:{}", port), |request| {
                    router!(request,
                        (GET) (/) => {
                            rouille::Response::text("Hello!")
                        },

                        (GET) (/unknown-content-type) => {
                            let mut r = rouille::Response::text("Hello!");
                            r.headers = r.headers.iter().filter(|x| x.0 != "Content-Type").map(|x| x.clone()).collect::<Vec<_>>();
                            r
                        },

                        (GET) (/image) => {
                            use std::borrow::Cow;

                            let mut r = rouille::Response::text("TEST JPEG DATA");
                            r.headers = r.headers.iter().filter(|x| x.0 != "Content-Type").map(|x| x.clone()).collect::<Vec<_>>();
                            r.headers.push((Cow::from("Content-Type"), Cow::from("image/jpeg")));
                            r

                        },

                        _ => rouille::Response::empty_404(),
                    )
                }).unwrap();

                loop {
                    match srv_rx.recv_timeout(std::time::Duration::from_millis(10)) {
                        Ok(_) => break,
                        _ => server.poll(),
                    }
                }
            });

            (join_handle, srv_tx)
        }

        pub fn multipart_formdata_request() -> rouille::Request {
            let body = "\
                        --boundary-guard-abcdef123456\r\n\
                        Content-Disposition: form-data; name=\"file\"; filename=\"sample.jpg\"\r\n\
                        Content-Type: image/jpeg\r\n\
                        \r\n\
                        JPEG IMAGE DATA\r\n\
                        --boundary-guard-abcdef123456\r\n\
                        Content-Disposition: form-data; name=\"file-from-name\"\r\n\
                        Content-Type: image/PNG\r\n\
                        \r\n\
                        PNG IMAGE DATA\r\n\
                        --boundary-guard-abcdef123456\r\n\
                        Content-Disposition: form-data; name=\"not-an-image\"\r\n\
                        Content-Type: text/plain\r\n\
                        \r\n\
                        Some text.\r\n\
                        --boundary-guard-abcdef123456--";

            let headers = [
                (
                    String::from("Content-Type"),
                    String::from("multipart/form-data; boundary=boundary-guard-abcdef123456"),
                ),
                (
                    String::from("Content-Length"),
                    body.as_bytes().len().to_string(),
                ),
            ];

            rouille::Request::fake_http(
                "POST",
                "/images",
                headers.to_vec(),
                body.as_bytes().to_vec(),
            )
        }

        pub fn json_request(port: u16) -> rouille::Request {
            let body = r#"
                    [
                        { "url": "http://not-existent-server-a3bc8def" },
                        { "url": "http://localhost:@port@/image" },
                        { "data": "errorneus data sdgfsdfgs5tegdsgd" },
                        { "filename": "valid_base64", "data": "VEVTVCBKUEVHIERBVEE=" }
                    ]
                    "#;
            let body = body.replace("@port@", &port.to_string());

            let headers = [
                (
                    String::from("Content-Type"),
                    String::from("application/json"),
                ),
                (
                    String::from("Content-Length"),
                    body.as_bytes().len().to_string(),
                ),
            ];

            rouille::Request::fake_http(
                "POST",
                "/images",
                headers.to_vec(),
                body.as_bytes().to_vec(),
            )
        }

        pub fn plaintext_request() -> rouille::Request {
            let body = "Hello World!";

            let headers = [
                (String::from("Content-Type"), String::from("text/plain")),
                (
                    String::from("Content-Length"),
                    body.as_bytes().len().to_string(),
                ),
            ];

            rouille::Request::fake_http(
                "POST",
                "/images",
                headers.to_vec(),
                body.as_bytes().to_vec(),
            )
        }

        pub fn unknown_content_request() -> rouille::Request {
            let body = "Unknown?";

            let headers = [(
                String::from("Content-Length"),
                body.as_bytes().len().to_string(),
            )];

            rouille::Request::fake_http(
                "POST",
                "/images",
                headers.to_vec(),
                body.as_bytes().to_vec(),
            )
        }

    }
}
