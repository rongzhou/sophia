//! span ↔ LSP 位置换算。
//!
//! Sophia `Span` 用字节偏移 + 0 基行列；LSP `Position` 用 0 基行 + **UTF-16**
//! code unit 列。这里做二者转换，并提供 LSP `Position` → 字节偏移（供 hover/goto
//! 按光标位置查询）。

use sophia_syntax::Span;
use tower_lsp::lsp_types::{Position, Range};

/// Sophia `Span` → LSP `Range`。
///
/// Sophia 的 column 是字节列；需转换为 UTF-16 列。由于 span 自身不带源码，
/// 转换需要源码：见 [`span_to_range`]。
pub fn span_to_range(source: &str, span: Span) -> Range {
    Range {
        start: byte_to_position(source, span.start.byte),
        end: byte_to_position(source, span.end.byte),
    }
}

/// 字节偏移 → LSP `Position`（0 基行 + UTF-16 列）。
pub fn byte_to_position(source: &str, byte: usize) -> Position {
    let byte = byte.min(source.len());
    let mut line = 0u32;
    let mut line_start = 0usize; // 当前行起始字节。
    for (i, c) in source.char_indices() {
        if i >= byte {
            break;
        }
        if c == '\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    // 行内列：从行首到 byte 的 UTF-16 长度。
    let col_utf16 = source[line_start..byte].encode_utf16().count() as u32;
    Position {
        line,
        character: col_utf16,
    }
}

/// LSP `Position`（0 基行 + UTF-16 列）→ 字节偏移。
pub fn position_to_byte(source: &str, pos: Position) -> usize {
    let mut line_starts = vec![0usize];
    for (i, c) in source.char_indices() {
        if c == '\n' {
            line_starts.push(i + 1);
        }
    }

    let Some(&line_start) = line_starts.get(pos.line as usize) else {
        return source.len();
    };

    // 行内：消费 pos.character 个 UTF-16 code unit。
    let mut utf16_seen = 0u32;
    let mut byte = line_start;
    for c in source[line_start..].chars() {
        if utf16_seen >= pos.character || c == '\n' {
            break;
        }
        utf16_seen += c.len_utf16() as u32;
        byte += c.len_utf8();
    }
    byte
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_position_roundtrip_ascii() {
        let src = "abc\ndef\nghi";
        // 'e' 在第 1 行第 1 列（0 基）。
        let byte = src.find('e').unwrap();
        let pos = byte_to_position(src, byte);
        assert_eq!(
            pos,
            Position {
                line: 1,
                character: 1
            }
        );
        assert_eq!(position_to_byte(src, pos), byte);
    }

    #[test]
    fn utf16_column_for_multibyte() {
        // 中文字符占 3 字节、1 个 UTF-16 code unit。
        let src = "中文 x";
        let byte = src.find('x').unwrap();
        let pos = byte_to_position(src, byte);
        // "中文 " → 3 个 UTF-16 单元（两个汉字 + 空格）。
        assert_eq!(
            pos,
            Position {
                line: 0,
                character: 3
            }
        );
        assert_eq!(position_to_byte(src, pos), byte);
    }

    #[test]
    fn position_past_end_clamps() {
        let src = "ab";
        let pos = Position {
            line: 5,
            character: 0,
        };
        assert_eq!(position_to_byte(src, pos), src.len());
    }

    #[test]
    fn position_on_trailing_empty_line_maps_to_eof() {
        let src = "a\n";
        assert_eq!(
            position_to_byte(
                src,
                Position {
                    line: 1,
                    character: 0
                }
            ),
            src.len()
        );
    }

    #[test]
    fn position_on_consecutive_empty_lines_roundtrips() {
        let src = "a\n\nb";
        let byte = src.find('b').unwrap();
        let pos = byte_to_position(src, byte);
        assert_eq!(
            pos,
            Position {
                line: 2,
                character: 0
            }
        );
        assert_eq!(position_to_byte(src, pos), byte);
    }

    #[test]
    fn position_inside_surrogate_pair_clamps_after_scalar() {
        let src = "😀x";
        assert_eq!(
            position_to_byte(
                src,
                Position {
                    line: 0,
                    character: 1
                }
            ),
            "😀".len()
        );
    }
}
