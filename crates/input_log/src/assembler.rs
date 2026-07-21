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
}
