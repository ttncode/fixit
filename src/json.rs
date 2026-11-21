//! Just enough JSON for Gemini's request and reply: no crate beyond `toml` is allowed.

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn at(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Arr(items) => items.get(i),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
}

pub fn parse(s: &str) -> Result<Json, String> {
    let mut p = Parser { chars: s.chars().peekable() };
    let value = p.value()?;
    p.skip_ws();
    match p.chars.next() {
        None => Ok(value),
        Some(c) => Err(format!("unexpected {c:?} after the JSON value")),
    }
}

/// A quoted JSON string literal for `s`.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Parser<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.chars.next_if(|c| matches!(c, ' ' | '\t' | '\n' | '\r')).is_some() {}
    }

    fn expect(&mut self, want: char) -> Result<(), String> {
        match self.chars.next() {
            Some(c) if c == want => Ok(()),
            Some(c) => Err(format!("expected {want:?}, found {c:?}")),
            None => Err(format!("expected {want:?}, found the end")),
        }
    }

    fn word(&mut self, word: &str, value: Json) -> Result<Json, String> {
        for want in word.chars() {
            self.expect(want)?;
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.chars.peek() {
            None => Err("unexpected end".into()),
            Some('n') => self.word("null", Json::Null),
            Some('t') => self.word("true", Json::Bool(true)),
            Some('f') => self.word("false", Json::Bool(false)),
            Some('"') => self.string().map(Json::Str),
            Some('[') => self.array(),
            Some('{') => self.object(),
            Some(c) if *c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected {c:?}")),
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let mut text = String::new();
        while let Some(c) = self.chars.next_if(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')) {
            text.push(c);
        }
        text.parse::<f64>().map(Json::Num).map_err(|_| format!("bad number {text:?}"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut n = 0;
        for _ in 0..4 {
            let d = self.chars.next().and_then(|c| c.to_digit(16)).ok_or("bad \\u escape")?;
            n = n * 16 + d;
        }
        Ok(n)
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            match self.chars.next().ok_or("unterminated string")? {
                '"' => return Ok(out),
                '\\' => match self.chars.next().ok_or("unterminated string")? {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        let hi = self.hex4()?;
                        let code = if (0xD800..0xDC00).contains(&hi) {
                            self.expect('\\')?;
                            self.expect('u')?;
                            let lo = self.hex4()?;
                            if !(0xDC00..0xE000).contains(&lo) {
                                return Err("unpaired surrogate".into());
                            }
                            0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                        } else {
                            hi
                        };
                        out.push(char::from_u32(code).ok_or("unpaired surrogate")?);
                    }
                    c => return Err(format!("bad escape \\{c}")),
                },
                c if (c as u32) < 0x20 => return Err("control character in string".into()),
                c => out.push(c),
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.chars.next_if_eq(&']').is_some() {
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.chars.next() {
                Some(',') => continue,
                Some(']') => return Ok(Json::Arr(items)),
                _ => return Err("expected ',' or ']'".into()),
            }
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect('{')?;
        let mut fields = Vec::new();
        self.skip_ws();
        if self.chars.next_if_eq(&'}').is_some() {
            return Ok(Json::Obj(fields));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(':')?;
            fields.push((key, self.value()?));
            self.skip_ws();
            match self.chars.next() {
                Some(',') => continue,
                Some('}') => return Ok(Json::Obj(fields)),
                _ => return Err("expected ',' or '}'".into()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalars() {
        assert_eq!(parse("null"), Ok(Json::Null));
        assert_eq!(parse(" true "), Ok(Json::Bool(true)));
        assert_eq!(parse("false"), Ok(Json::Bool(false)));
        assert_eq!(parse("-1.5e2"), Ok(Json::Num(-150.0)));
        assert_eq!(parse("0"), Ok(Json::Num(0.0)));
        assert_eq!(parse("\"x\""), Ok(Json::Str("x".into())));
    }

    #[test]
    fn parses_nested() {
        let v = parse(r#"{"a":[1,{"b":"c"}], "d": {}}"#).unwrap();
        assert_eq!(v.get("a").and_then(|a| a.at(1)).and_then(|o| o.get("b")).and_then(Json::as_str), Some("c"));
        assert_eq!(v.get("a").and_then(|a| a.at(0)), Some(&Json::Num(1.0)));
        assert_eq!(v.get("d"), Some(&Json::Obj(vec![])));
        assert_eq!(v.get("missing"), None);
        assert_eq!(v.at(0), None);
    }

    #[test]
    fn parses_unicode_escapes_and_surrogates() {
        assert_eq!(parse(r#""\u00e9\uD83D\uDE00\n\/\"""#), Ok(Json::Str("é😀\n/\"".into())));
        assert!(parse(r#""\uD83D""#).is_err());
        assert!(parse(r#""\uDE00""#).is_err());
    }

    #[test]
    fn rejects_bad_input() {
        for bad in ["", "{", "[1,]", "\"abc", "{} x", "{\"a\" 1}", "tru", "\"a\nb\"", "01x"] {
            assert!(parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn escape_round_trips() {
        for s in ["q\"b\\s", "a\nb\r\tc", "\u{1}", "Tiếng Việt có dấu", "😀"] {
            assert_eq!(parse(&escape(s)), Ok(Json::Str(s.into())), "for {s:?}");
        }
        assert!(escape("\u{1}").contains("\\u0001"));
        assert_eq!(escape("Việt"), "\"Việt\"");
    }
}
