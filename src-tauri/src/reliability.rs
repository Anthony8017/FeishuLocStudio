use serde_json::Value;
use std::{future::Future, time::Duration};

// HTTP chunks may end in the middle of a UTF-8 code point.
#[derive(Default)]
pub(crate) struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    pub fn push(&mut self, bytes: &[u8]) -> Result<String, String> {
        self.pending.extend_from_slice(bytes);
        let length = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(_) => return Err("Invalid UTF-8 in AI stream.".into()),
        };
        let text = String::from_utf8(self.pending.drain(..length).collect()).unwrap();
        Ok(text)
    }

    pub fn finish(&self) -> Result<(), String> {
        if self.pending.is_empty() {
            Ok(())
        } else {
            Err("AI stream ended inside a UTF-8 character.".into())
        }
    }
}

#[derive(Default)]
pub(crate) struct OpenAiStream {
    decoder: Utf8Stream,
    buffer: String,
    pub content: String,
    pub usage: Value,
    stopped: bool,
    frames: usize,
    done: bool,
    received_bytes: usize,
    finish_reason: Option<String>,
}

impl OpenAiStream {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<String>, String> {
        self.received_bytes += bytes.len();
        self.buffer.push_str(&self.decoder.push(bytes)?);
        let mut deltas = Vec::new();
        while let Some(index) = self.buffer.find('\n') {
            let line = self.buffer[..index].to_string();
            self.buffer.drain(..=index);
            if let Some(delta) = self.line(&line)? {
                deltas.push(delta);
            }
        }
        Ok(deltas)
    }

    fn line(&mut self, line: &str) -> Result<Option<String>, String> {
        let Some(data) = line.trim().strip_prefix("data:").map(str::trim) else {
            return Ok(None);
        };
        if data == "[DONE]" {
            self.done = true;
            return Ok(None);
        }
        if data.is_empty() {
            return Ok(None);
        }
        self.frames += 1;
        let json: Value =
            serde_json::from_str(data).map_err(|e| format!("Invalid SSE JSON: {e}"))?;
        if let Some(error) = json.get("error").filter(|v| !v.is_null()) {
            return Err(format!("AI stream error: {error}"));
        }
        if let Some(choice) = json
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
        {
            if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
                self.finish_reason = Some(reason.to_string());
                if reason != "stop" {
                    return Err(format!("AI output incomplete: finish_reason={reason}"));
                }
                self.stopped = true;
            }
            if let Some(delta) = choice.pointer("/delta/content").and_then(Value::as_str) {
                self.content.push_str(delta);
                if json.get("usage").is_some_and(|v| !v.is_null()) {
                    self.usage = json.clone();
                }
                return Ok(Some(delta.to_string()));
            }
        }
        if json.get("usage").is_some_and(|v| !v.is_null()) {
            self.usage = json;
        }
        Ok(None)
    }

    pub fn finish(&mut self) -> Result<Value, String> {
        self.decoder.finish()?;
        let tail = std::mem::take(&mut self.buffer);
        self.line(&tail)?;
        if self.content.trim().is_empty() {
            return Err("AI stream returned no content.".into());
        }
        if !self.stopped {
            return Err(format!(
                "AI stream ended without a successful finish_reason. frames={}, done={}, content_bytes={}, valid_json={}",
                self.frames, self.done, self.content.len(), serde_json::from_str::<Value>(&self.content).is_ok()
            ));
        }
        serde_json::from_str(&self.content)
            .map_err(|e| format!("Invalid structured AI output: {e}"))
    }

    pub fn diagnostics(&self) -> Value {
        serde_json::json!({
            "receivedBytes": self.received_bytes,
            "frames": self.frames,
            "done": self.done,
            "successfulFinish": self.stopped,
            "finishReason": self.finish_reason,
            "contentBytes": self.content.len(),
            "validJson": serde_json::from_str::<Value>(&self.content).is_ok()
        })
    }
}

pub(crate) async fn cancellable<T>(
    task: &str,
    deadline: Duration,
    future: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::pin!(future);
    let timeout = tokio::time::sleep(deadline);
    tokio::pin!(timeout);
    let mut poll = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            biased;
            _ = poll.tick() => { super::ensure_task_not_cancelled(task)?; }
            _ = &mut timeout => return Err("AI request timed out.".into()),
            result = &mut future => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream(text: &str, reason: &str) -> Vec<u8> {
        format!(
            "data: {}\r\n\r\ndata: {}\n\ndata: [DONE]\n",
            serde_json::json!({"choices":[{"delta":{"content":text}}]}),
            serde_json::json!({"choices":[{"delta":{},"finish_reason":reason}]})
        )
        .into_bytes()
    }

    #[test]
    fn unicode_survives_every_possible_network_split() {
        let bytes = stream(r#"{"text":"蚁巢・日本語🐜"}"#, "stop");
        for split in 0..=bytes.len() {
            let mut parser = OpenAiStream::default();
            parser.push(&bytes[..split]).unwrap();
            parser.push(&bytes[split..]).unwrap();
            assert_eq!(parser.finish().unwrap()["text"], "蚁巢・日本語🐜");
        }
    }

    #[test]
    fn done_without_finish_reason_remains_rejected_and_diagnosable() {
        let mut parser = OpenAiStream::default();
        parser.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"{}\"}}]}\ndata: [DONE]\n").unwrap();
        assert!(parser.finish().unwrap_err().contains("valid_json=true"));
        let diagnostic = parser.diagnostics();
        assert_eq!(diagnostic["done"], true);
        assert_eq!(diagnostic["successfulFinish"], false);
        assert_eq!(diagnostic["validJson"], true);
        assert_eq!(diagnostic["frames"], 1);
        assert!(diagnostic["finishReason"].is_null());
        assert!(diagnostic.get("content").is_none());
    }

    #[test]
    fn empty_error_truncated_and_invalid_streams_fail() {
        assert!(OpenAiStream::default().finish().is_err());
        assert!(OpenAiStream::default()
            .push(b"data: {\"error\":{\"message\":\"quota\"}}\n")
            .is_err());
        assert!(OpenAiStream::default()
            .push(&stream("{}", "length"))
            .is_err());
        let mut parser = OpenAiStream::default();
        parser
            .push(b"data: {\"choices\":[{\"delta\":{\"content\":\"{}\"}}]}\n")
            .unwrap();
        assert!(parser.finish().is_err());
        let mut decoder = Utf8Stream::default();
        decoder.push(&[0xe8]).unwrap();
        assert!(decoder.finish().is_err());
        assert!(Utf8Stream::default().push(&[0xff]).is_err());
        let mut parser = OpenAiStream::default();
        parser.push(&stream("{", "stop")).unwrap();
        assert!(parser.finish().is_err());
    }

    #[tokio::test]
    async fn cancellation_and_timeout_drop_pending_requests() {
        super::super::cancel_task("test-cancel");
        let result = cancellable(
            "test-cancel",
            Duration::from_secs(1),
            std::future::pending::<Result<(), String>>(),
        )
        .await;
        assert_eq!(result.unwrap_err(), "TRANSLATION_CANCELLED");
        super::super::clear_cancelled_task("test-cancel");
        assert!(cancellable(
            "test-timeout",
            Duration::from_millis(10),
            std::future::pending::<Result<(), String>>()
        )
        .await
        .unwrap_err()
        .contains("timed out"));
    }
}
