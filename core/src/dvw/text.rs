//! The text of a file: the recent ones are UTF-8, the older ones in the
//! code page of the computer of the scout (1252 in western Europe, 1250 in
//! the central one...), which the file may declare in `[3MATCH]`.
//! DataVolley 4 adds a UTF-8 copy of the names, whatever the code page.

use encoding_rs::{Encoding, WINDOWS_1252};

/// The bytes as text: UTF-8 when they are, otherwise the declared code page
/// or, without one, Windows-1252.
pub(crate) fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => {
            let encoding = declared_encoding(bytes).unwrap_or(WINDOWS_1252);
            encoding.decode_without_bom_handling(bytes).0.into_owned()
        }
    }
}

// the 9th field of the line after `[3MATCH]`: a Windows code page, e.g.
// `1250`; `1` stands for the default of the computer, which says nothing
fn declared_encoding(bytes: &[u8]) -> Option<&'static Encoding> {
    let ascii = String::from_utf8_lossy(bytes);
    let mut lines = ascii.lines().skip_while(|l| l.trim() != "[3MATCH]");
    let declared = lines.nth(1)?.split(';').nth(8)?.trim().to_string();
    match declared.parse::<u16>() {
        Ok(page) if page > 1 => Encoding::for_label(format!("windows-{page}").as_bytes()),
        _ => Encoding::for_label(declared.as_bytes()),
    }
}

/// A UTF-8 copy of DataVolley 4: `\u{f}`, the number of hex digits per
/// character, then the code points in hex. None when the field isn't one,
/// or doesn't decode.
pub(crate) fn embedded_utf8(field: &str) -> Option<String> {
    let encoded = field.strip_prefix('\u{f}')?;
    let mut chars = encoded.chars();
    let width = chars.next()?.to_digit(10)? as usize;
    let digits: Vec<char> = chars.collect();
    match width {
        2 | 4 => digits
            .chunks(width)
            .map(|chunk| {
                u32::from_str_radix(&chunk.iter().collect::<String>(), 16)
                    .ok()
                    .and_then(char::from_u32)
            })
            .collect::<Option<String>>()
            .map(|text| text.trim().to_string()),
        _ => None,
    }
}

/// The UTF-8 copy when there is one, otherwise the field as written.
pub(crate) fn prefer_utf8(plain: &str, copy: Option<&str>) -> String {
    copy.and_then(embedded_utf8)
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| plain.trim().to_string())
}
