//! Gemini `generateContent`: the request body, and the reply as text or a short message.

use crate::json::{self, Json};

pub const URL_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models/";
pub const DEFAULT_MODEL: &str = "gemini-flash-lite-latest";

pub fn body(style: &str, text: &str) -> String {
    format!(
        r#"{{"systemInstruction":{{"parts":[{{"text":{}}}]}},"contents":[{{"role":"user","parts":[{{"text":{}}}]}}],"generationConfig":{{"temperature":0.2}}}}"#,
        json::escape(style),
        json::escape(text)
    )
}

/// The model name goes into the URL, so only `[A-Za-z0-9._-]` is allowed.
pub fn check_model(model: &str) -> Result<(), String> {
    let ok = !model.is_empty() && model.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok { Ok(()) } else { Err("GEMINI_MODEL has invalid characters".into()) }
}

/// `curl_stdout` is the reply body followed by `-w "\n%{http_code}"`.
pub fn parse_reply(curl_stdout: &str) -> Result<String, String> {
    let not_understood = || "Gemini reply was not understood".to_string();
    let (body, status) = curl_stdout.rsplit_once('\n').ok_or_else(not_understood)?;
    let status: u16 = status.trim().parse().map_err(|_| not_understood())?;
    let reply = json::parse(body);
    if status != 200 {
        let message = reply.ok().and_then(|r| r.get("error")?.get("message")?.as_str().map(str::to_owned));
        return Err(message.unwrap_or_else(|| format!("HTTP {status}")));
    }
    let reply = reply.map_err(|_| not_understood())?;
    let no_text = |reason: Option<&str>| match reason {
        Some(r) => format!("Gemini returned no text ({r})"),
        None => "Gemini returned no text".to_string(),
    };
    let Some(candidate) = reply.get("candidates").and_then(|c| c.at(0)) else {
        let reason = reply.get("promptFeedback").and_then(|f| f.get("blockReason")).and_then(Json::as_str);
        return Err(no_text(reason));
    };
    // MAX_TOKENS and the rest mean a cut or filtered answer, which must never replace the user's text.
    match candidate.get("finishReason").and_then(Json::as_str) {
        None | Some("STOP") => {}
        Some(reason) => return Err(no_text(Some(reason))),
    }
    let mut text = String::new();
    let mut i = 0;
    while let Some(part) = candidate.get("content").and_then(|c| c.get("parts")).and_then(|p| p.at(i)) {
        text.push_str(part.get("text").and_then(Json::as_str).unwrap_or(""));
        i += 1;
    }
    if text.trim().is_empty() { Err(no_text(None)) } else { Ok(text) }
}

/// The result with the original's leading/trailing whitespace and its CRLF line endings.
pub fn fit(original: &str, result: &str) -> String {
    let mut core = result.trim().to_string();
    if original.contains("\r\n") {
        core = core.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    let lead = &original[..original.len() - original.trim_start().len()];
    let trail = &original[original.trim_end().len()..];
    format!("{lead}{core}{trail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_round_trips() {
        let style = "Fix \"it\"";
        let text = "Tiếng Việt\n\t\"x\"";
        let v = json::parse(&body(style, text)).unwrap();
        let first_text = |v: &Json| v.get("parts").and_then(|p| p.at(0)).and_then(|p| p.get("text")).and_then(Json::as_str).map(str::to_owned);
        assert_eq!(v.get("systemInstruction").and_then(first_text), Some(style.into()));
        let content = v.get("contents").and_then(|c| c.at(0)).unwrap();
        assert_eq!(first_text(content), Some(text.into()));
        assert_eq!(content.get("role").and_then(Json::as_str), Some("user"));
        assert_eq!(v.get("generationConfig").and_then(|g| g.get("temperature")), Some(&Json::Num(0.2)));
    }

    #[test]
    fn reply_joins_parts() {
        let out = "{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"A\"},{\"text\":\"B\"}]},\"finishReason\":\"STOP\"}]}\n200";
        assert_eq!(parse_reply(out), Ok("AB".into()));
    }

    #[test]
    fn reply_error_message() {
        let out = "{\"error\":{\"code\":400,\"message\":\"API key not valid.\"}}\n400";
        assert_eq!(parse_reply(out), Err("API key not valid.".into()));
    }

    #[test]
    fn reply_non_json_error() {
        assert_eq!(parse_reply("<html>busy</html>\n503"), Err("HTTP 503".into()));
    }

    #[test]
    fn reply_blocked_prompt() {
        let out = "{\"promptFeedback\":{\"blockReason\":\"SAFETY\"}}\n200";
        assert_eq!(parse_reply(out), Err("Gemini returned no text (SAFETY)".into()));
    }

    #[test]
    fn reply_max_tokens() {
        let out = "{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Half a sen\"}]},\"finishReason\":\"MAX_TOKENS\"}]}\n200";
        assert_eq!(parse_reply(out), Err("Gemini returned no text (MAX_TOKENS)".into()));
    }

    #[test]
    fn reply_empty_text() {
        let out = "{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"\"}]},\"finishReason\":\"STOP\"}]}\n200";
        assert!(parse_reply(out).is_err());
    }

    #[test]
    fn split_status_without_newline_in_body() {
        let pretty = "{\n  \"candidates\": [{\"content\": {\"parts\": [{\"text\": \"Hi\"}]}, \"finishReason\": \"STOP\"}]\n}\n200";
        assert_eq!(parse_reply(pretty), Ok("Hi".into()));
        let tight = "{\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hi\"}]},\"finishReason\":\"STOP\"}]}\n200";
        assert_eq!(parse_reply(tight), Ok("Hi".into()));
    }

    #[test]
    fn reply_garbage() {
        for out in ["", "abc", "{}\nabc", "{}\n200"] {
            assert!(parse_reply(out).is_err(), "{out:?}");
        }
        assert_eq!(parse_reply("abc"), Err("Gemini reply was not understood".into()));
    }

    #[test]
    fn fit_keeps_whitespace() {
        assert_eq!(fit("  hi there\n", " Hi there. \n\n"), "  Hi there.\n");
        assert_eq!(fit("hi", "Hi."), "Hi.");
    }

    #[test]
    fn fit_restores_crlf() {
        assert_eq!(fit("a\r\nb\r\n", "A\nB"), "A\r\nB\r\n");
        assert_eq!(fit("a\r\nb", "A\r\nB"), "A\r\nB");
        assert_eq!(fit("a\nb", "A\nB"), "A\nB");
    }

    #[test]
    fn model_check() {
        assert_eq!(check_model("gemini-flash-lite-latest"), Ok(()));
        assert_eq!(check_model("gemini-2.5-flash_x"), Ok(()));
        for bad in ["a/b", "a?b", "", "a b"] {
            assert_eq!(check_model(bad), Err("GEMINI_MODEL has invalid characters".into()), "{bad:?}");
        }
    }
}
