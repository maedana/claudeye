use std::io;
use std::path::PathBuf;

/// Build the socket path: /tmp/crmux-{uid}.sock
pub fn socket_path() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/crmux-{uid}.sock"))
}

/// Encode a msgpack-rpc request (Type 0): [0, msgid, method, params]
pub fn encode_request(msgid: u32, method: &str, params: &serde_json::Value) -> Vec<u8> {
    // msgpack-rpc Type 0: [0, msgid, method, params]
    let request = (0u32, msgid, method, params);
    rmp_serde::to_vec(&request).expect("failed to encode msgpack-rpc request")
}

/// Decode a msgpack-rpc response (Type 1): [1, msgid, error, result]
/// Returns (msgid, result) on success.
pub fn decode_response(data: &[u8]) -> io::Result<(u32, serde_json::Value)> {
    let resp: (u32, u32, serde_json::Value, serde_json::Value) =
        rmp_serde::from_slice(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    if resp.0 != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected type 1 (response), got {}", resp.0),
        ));
    }

    if !resp.2.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("rpc error: {}", resp.2),
        ));
    }

    Ok((resp.1, resp.3))
}

/// Connect to crmux socket and fetch sessions.
pub fn fetch_sessions() -> io::Result<serde_json::Value> {
    use std::io::{Read, Write};
    use std::net::Shutdown;
    use std::os::unix::net::UnixStream;

    let path = socket_path();
    let mut stream = UnixStream::connect(&path)?;

    let request = encode_request(0, "get_sessions", &serde_json::json!({}));
    stream.write_all(&request)?;
    stream.shutdown(Shutdown::Write)?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf)?;

    let (_msgid, result) = decode_response(&buf)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a fake msgpack-rpc response for testing: [1, msgid, null, result]
    fn encode_response_for_test(msgid: u32, result: &serde_json::Value) -> Vec<u8> {
        let resp = (1u32, msgid, serde_json::Value::Null, result);
        rmp_serde::to_vec(&resp).expect("failed to encode test response")
    }

    #[test]
    fn test_socket_path_contains_uid() {
        let path = socket_path();
        let uid = unsafe { libc::getuid() };
        assert!(path.to_str().unwrap().contains(&uid.to_string()));
        assert!(path.to_str().unwrap().starts_with("/tmp/crmux-"));
        assert!(path.to_str().unwrap().ends_with(".sock"));
    }

    #[test]
    fn test_encode_request_produces_valid_msgpack() {
        let params = serde_json::json!({});
        let encoded = encode_request(1, "get_sessions", &params);
        assert!(!encoded.is_empty());

        // Decode it back to verify structure
        let decoded: (u32, u32, String, serde_json::Value) =
            rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(decoded.0, 0); // type 0 = request
        assert_eq!(decoded.1, 1); // msgid
        assert_eq!(decoded.2, "get_sessions"); // method
        assert_eq!(decoded.3, serde_json::json!({})); // params
    }

    #[test]
    fn test_encode_request_decode_round_trip() {
        let params = serde_json::json!({});
        let _encoded = encode_request(1, "get_sessions", &params);

        let result = serde_json::json!({"sessions": [], "visible": true});
        let response_bytes = encode_response_for_test(1, &result);

        let (msgid, decoded_result) = decode_response(&response_bytes).unwrap();
        assert_eq!(msgid, 1);
        assert_eq!(decoded_result, result);
    }

    #[test]
    fn test_decode_response_extracts_sessions() {
        let sessions_data = serde_json::json!({
            "sessions": [
                {
                    "pane_id": "%1",
                    "pid": 100,
                    "project_name": "crmux",
                    "state": "Working",
                    "elapsed_secs": 45,
                    "model": "Opus",
                    "context_percent": 23,
                    "title": "implementing feature X",
                    "session_id": "abc-123",
                    "git_branch": "main"
                }
            ],
            "visible": true
        });

        let response_bytes = encode_response_for_test(42, &sessions_data);
        let (msgid, result) = decode_response(&response_bytes).unwrap();

        assert_eq!(msgid, 42);
        let sessions = result["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0]["pane_id"], "%1");
        assert_eq!(sessions[0]["model"], "Opus");
        assert_eq!(sessions[0]["context_percent"], 23);
        assert_eq!(sessions[0]["title"], "implementing feature X");
    }

    #[test]
    fn test_decode_response_rejects_wrong_type() {
        // Type 0 (request) instead of Type 1 (response)
        let bad = rmp_serde::to_vec(&(0u32, 1u32, serde_json::Value::Null, serde_json::json!({})))
            .unwrap();
        let err = decode_response(&bad).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_decode_response_rejects_rpc_error() {
        let error_resp = rmp_serde::to_vec(&(
            1u32,
            1u32,
            serde_json::json!("something went wrong"),
            serde_json::Value::Null,
        ))
        .unwrap();
        let err = decode_response(&error_resp).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Other);
    }

    #[test]
    fn test_decode_response_invalid_data() {
        let err = decode_response(&[0xFF, 0x00]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
