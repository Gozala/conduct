//! Requests to the page's origin through `wasi:http/client`.

use element_kit::wasi::http::client;
use element_kit::wasi::http::types::{ErrorCode, Fields, Method, Request, Response};
use element_kit::{wit_future, wit_stream};

/// Send `method` to `path` on the page's origin, with `body` as JSON if
/// there is one, and read the whole response's body. The demo's
/// elements read the body whatever the status.
pub async fn request(method: Method, path: &str, body: Option<String>) -> Result<Vec<u8>, ErrorCode> {
    let headers = Fields::new();
    let contents = match body {
        Some(body) => {
            headers.append("content-type", b"application/json").expect("a valid header");
            let (mut writer, reader) = wit_stream::new::<u8>();
            // The host reads the body while it sends, so it is written
            // beside the call rather than before it.
            wit_bindgen::spawn_local(async move {
                writer.write_all(body.into_bytes()).await;
            });
            Some(reader)
        }
        None => None,
    };
    // No trailers.
    let (trailers, trailers_reader) = wit_future::new(|| Ok(None));
    wit_bindgen::spawn_local(async move {
        let _ = trailers.write(Ok(None)).await;
    });
    let (request, _transmitted) = Request::new(headers, contents, trailers_reader, None);
    request.set_method(&method).expect("a valid method");
    request.set_path_with_query(Some(path)).expect("a valid path");

    let response = client::send(request).await?;
    // The future tells the host the body has been read: when its writer
    // drops after the read, it resolves to its default, `Ok(())`.
    let (done, done_reader) = wit_future::new(|| Ok(()));
    let (stream, _trailers) = Response::consume_body(response, done_reader);
    let body = stream.collect().await;
    drop(done);
    Ok(body)
}
