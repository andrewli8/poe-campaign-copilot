/// Hard cap on the not-yet-terminated tail buffer. A real Client.txt log
/// line is a single short status message — never anywhere close to this —
/// so if no newline has arrived by the time `partial` grows past this many
/// bytes, something is wrong upstream (a non-log file pointed at the
/// tailer, a corrupt/binary stream, ...) and the buffer would otherwise
/// grow unbounded for as long as that keeps happening.
const MAX_LINE_BYTES: usize = 64 * 1024;

#[derive(Default)]
pub struct LineAssembler {
    partial: Vec<u8>,
}

impl LineAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        self.partial.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = self.partial.iter().position(|&b| b == b'\n') {
            let mut line: Vec<u8> = self.partial.drain(..=pos).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(String::from_utf8_lossy(&line).into_owned());
        }
        // No newline arrived and the still-partial tail has grown past the
        // cap: discard it. There's no newline anywhere in `partial` at this
        // point (the loop above would have drained it otherwise), so this
        // is safe to just reset — a later real newline still starts a
        // fresh, empty buffer and works normally.
        if self.partial.len() > MAX_LINE_BYTES {
            self.partial.clear();
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_complete_lines_and_holds_partial_tail() {
        let mut a = LineAssembler::new();
        assert_eq!(a.feed(b"hello\nwor"), vec!["hello".to_string()]);
        assert_eq!(a.feed(b"ld\n"), vec!["world".to_string()]);
        assert_eq!(a.feed(b""), Vec::<String>::new());
    }

    #[test]
    fn strips_carriage_returns() {
        let mut a = LineAssembler::new();
        assert_eq!(
            a.feed(b"a\r\nb\r\n"),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn multiple_lines_in_one_chunk_and_split_across_chunks() {
        let mut a = LineAssembler::new();
        assert_eq!(
            a.feed(b"one\ntwo\nthr"),
            vec!["one".to_string(), "two".to_string()]
        );
        assert_eq!(
            a.feed(b"ee\nfour\n"),
            vec!["three".to_string(), "four".to_string()]
        );
    }

    #[test]
    fn invalid_utf8_is_lossy_not_fatal() {
        let mut a = LineAssembler::new();
        let lines = a.feed(b"ok\xFF\xFEline\n");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("ok"));
        assert!(lines[0].ends_with("line"));
    }

    #[test]
    fn oversized_partial_line_is_dropped_and_a_later_real_newline_still_works() {
        let mut a = LineAssembler::new();
        // 200 KiB with no newline at all: well past MAX_LINE_BYTES
        // (64 KiB), so the junk must be discarded rather than held onto
        // forever (proving `partial` doesn't grow unbounded).
        let junk = vec![b'x'; 200 * 1024];
        assert_eq!(a.feed(&junk), Vec::<String>::new());

        // Feeding a real, reasonably-sized line afterwards must yield
        // "real" and must NOT contain any of the (dropped) junk bytes —
        // the leading '\n' terminates the now-empty buffer as a
        // zero-length line, which is expected/harmless, not a resurgence
        // of the junk.
        let lines = a.feed(b"\nreal\n");
        assert!(
            lines.iter().all(|l| l.len() < 1024),
            "no oversized junk line leaked through: {lines:?}"
        );
        assert_eq!(lines.last(), Some(&"real".to_string()));
    }
}
