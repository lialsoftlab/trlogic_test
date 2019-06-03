use std::thread;
use trlogic_test::http_handlers::ImageUploadResult;
use trlogic_test::microservice;

#[test]
fn test_http_microservice_for_json_post()
{
    let mut tmp_path = std::env::temp_dir();
    tmp_path.push("trlogic-test-sfdgcvbr5");
    let _ = std::fs::remove_dir_all(&tmp_path);
    std::fs::create_dir_all(&tmp_path).unwrap();

    let (server, srv_tx, srv_rx) = microservice::init("localhost", 8100, tmp_path.to_str().unwrap());
    let srv = thread::spawn(move || {    
        microservice::run(server, srv_rx);
    });

    let mut response = mock::json_request(8100).send().unwrap();
    assert!(response.status.is_success());

    let content_lenght = response.headers.get("Content-Length").unwrap().parse::<usize>().unwrap();
    let mut body = vec![0u8; content_lenght];
    response.body.read_exact(&mut body).unwrap();

    let results: Vec<ImageUploadResult> = serde_json::from_slice(&body[..]).unwrap();
    assert_eq!(results[0].success, false);
    assert_eq!(results[1].success, true);
    assert_eq!(results[2].success, true);
    assert_eq!(results[3].success, false);
    assert_eq!(results[4].success, true);

    thread::sleep(std::time::Duration::from_secs(5)); // Await for thumbnails generation complete.
     
    let mut dir_list = std::fs::read_dir(&tmp_path)
        .unwrap()
        .map(|x| x.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(dir_list.len(), 4);
    dir_list[..].sort();
    assert_eq!(dir_list[0].to_str(), Some("123.jpg"));
    assert_eq!(dir_list[1].to_str(), Some("png.png"));
    assert_eq!(dir_list[2].to_str(), Some("thumbnails"));
    assert_eq!(dir_list[3].to_str(), Some("valid_base64.bin"));

    tmp_path.push("thumbnails");
    let mut dir_list = std::fs::read_dir(&tmp_path)
        .unwrap()
        .map(|x| x.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(dir_list.len(), 2);
    dir_list[..].sort();
    assert_eq!(dir_list[0].to_str(), Some("123.jpg"));
    assert_eq!(dir_list[1].to_str(), Some("png.png"));
    tmp_path.pop();

    srv_tx.send("stop").unwrap();
    srv.join().unwrap();
    let _ = std::fs::remove_dir_all(&tmp_path);
}

#[test]
fn test_http_microservice_for_multipart_post()
{
    let mut tmp_path = std::env::temp_dir();
    tmp_path.push("trlogic-test-dfgdrtythhf4");
    let _ = std::fs::remove_dir_all(&tmp_path);
    std::fs::create_dir_all(&tmp_path).unwrap();

    let (server, srv_tx, srv_rx) = microservice::init("localhost", 8101, tmp_path.to_str().unwrap());
    let srv = thread::spawn(move || {    
        microservice::run(server, srv_rx);
    });

    let mut response = mock::multipart_request(8101).send().unwrap();
    assert!(response.status.is_success());

    let content_lenght = response.headers.get("Content-Length").unwrap().parse::<usize>().unwrap();
    let mut body = vec![0u8; content_lenght];
    response.body.read_exact(&mut body).unwrap();

    let results: Vec<ImageUploadResult> = serde_json::from_slice(&body[..]).unwrap();
    assert_eq!(results[0].success, true);
    assert_eq!(results[1].success, true);
    assert_eq!(results[2].success, false);

    thread::sleep(std::time::Duration::from_secs(5)); // Await for thumbnails generation complete.
     
    let mut dir_list = std::fs::read_dir(&tmp_path)
        .unwrap()
        .map(|x| x.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(dir_list.len(), 2);
    dir_list[..].sort();
    assert_eq!(dir_list[0].to_str(), Some("file-from-name.png"));
    assert_eq!(dir_list[1].to_str(), Some("sample.jpg"));

    srv_tx.send("stop").unwrap();
    srv.join().unwrap();
    let _ = std::fs::remove_dir_all(&tmp_path);
}

#[test]
fn test_http_microservice_for_unacceptable_content_post()
{
    let mut tmp_path = std::env::temp_dir();
    tmp_path.push("trlogic-test-vsgtecvyjj56");
    let _ = std::fs::remove_dir_all(&tmp_path);
    std::fs::create_dir_all(&tmp_path).unwrap();

    let (server, srv_tx, srv_rx) = microservice::init("localhost", 8102, tmp_path.to_str().unwrap());
    let srv = thread::spawn(move || {    
        microservice::run(server, srv_rx);
    });

    let response = mock::plain_request(8102).send().unwrap();
    assert_eq!(i32::from(&response.status), 406);

    srv_tx.send("stop").unwrap();
    srv.join().unwrap();
    let _ = std::fs::remove_dir_all(&tmp_path);
}

#[test]
fn test_http_microservice_for_malformed_post()
{
    let mut tmp_path = std::env::temp_dir();
    tmp_path.push("trlogic-test-bnsghjtf4");
    let _ = std::fs::remove_dir_all(&tmp_path);
    std::fs::create_dir_all(&tmp_path).unwrap();

    let (server, srv_tx, srv_rx) = microservice::init("localhost", 8103, tmp_path.to_str().unwrap());
    let srv = thread::spawn(move || {    
        microservice::run(server, srv_rx);
    });

    let response = mock::malformed_json_request(8103).send().unwrap();
    assert_eq!(i32::from(&response.status), 400);

    srv_tx.send("stop").unwrap();
    srv.join().unwrap();
    let _ = std::fs::remove_dir_all(&tmp_path);
}

mod mock {
    use mrq;
    
    pub fn json_request(port: u16) -> mrq::Request {
        let body = r#"
                [
                    { "url": "http://not-existent-server-a3bc8def" },
                    { "url": "https://placehold.co/321/png" },
                    { "url": "https://via.placeholder.com/123.jpg" },
                    { "data": "errorneus data sdgfsdfgs5tegdsgd" },
                    { "filename": "valid_base64", "data": "VEVTVCBKUEVHIERBVEE=" }
                ]
                "#;

        mrq::post(format!("http://localhost:{}/images", port))
            .with_header("Content-Type", "application/json")
            .with_body(body)
    }

    pub fn multipart_request(port: u16) -> mrq::Request {
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

        mrq::post(format!("http://localhost:{}/images", port))
            .with_header("Content-Type", "multipart/form-data; boundary=boundary-guard-abcdef123456")
            .with_body(body)
    }

    pub fn plain_request(port: u16) -> mrq::Request {
        let body = "Hello World";

        mrq::post(format!("http://localhost:{}/images", port))
            .with_header("Content-Type", "text/plain")
            .with_body(body)
    }

    pub fn malformed_json_request(port: u16) -> mrq::Request {
        let body = r#"
                [
                    { "url": "http://not-existent-server-a3bc8def" }
                    { "url": "https://placehold.co/321/png" },
                    { "url "https://via.placeholder.com/123.jpg" 
                    { "data": "errorneus data sdgfsdfgs5tegdsgd" },
                    { "filename": "valid_base64", "data": "VEVTVCBKUEVHIERBVEE=" }
                
                "#;

        mrq::post(format!("http://localhost:{}/images", port))
            .with_header("Content-Type", "application/json")
            .with_body(body)
    }
}