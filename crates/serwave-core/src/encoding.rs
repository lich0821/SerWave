use encoding_rs::{UTF_8, UTF_16LE, GBK};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextEncoding {
    Auto,
    Utf8,
    Utf16,
    Ascii,
    Gbk,
    Gb2312,
}

impl std::str::FromStr for TextEncoding {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "UTF-8" => Self::Utf8,
            "UTF-16" => Self::Utf16,
            "ASCII" => Self::Ascii,
            "GBK" => Self::Gbk,
            "GB2312" => Self::Gb2312,
            _ => Self::Auto,
        })
    }
}

impl TextEncoding {

    pub fn decode(&self, bytes: &[u8]) -> String {
        match self {
            Self::Auto => detect_and_decode(bytes),
            Self::Utf8 => UTF_8.decode(bytes).0.into_owned(),
            Self::Utf16 => UTF_16LE.decode(bytes).0.into_owned(),
            Self::Ascii => bytes.iter().map(|&b| if b < 128 { b as char } else { '?' }).collect(),
            Self::Gbk | Self::Gb2312 => GBK.decode(bytes).0.into_owned(),
        }
    }
}

fn detect_and_decode(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }

    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);

    encoding.decode(bytes).0.into_owned()
}
