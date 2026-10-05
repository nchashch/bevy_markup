use std::ops::Range;

pub(crate) fn matches_fluent_ws(c: char) -> bool {
    c == ' ' || c == '\r' || c == '\n'
}

pub trait Slice<'s>: AsRef<str> + Clone + PartialEq {
    fn slice(&self, range: Range<usize>) -> Self;
    fn trim(&mut self);
}

impl<'s> Slice<'s> for String {
    fn slice(&self, range: Range<usize>) -> Self {
        // PATCH(bevy_markup): the parser computes some ranges in bytes minus one or
        // with byte guesses, which can land inside a multi-byte character.
        // Clamp to char boundaries instead of panicking.
        let start = clamp_boundary(self, range.start);
        let end = clamp_boundary(self, range.end);
        self[start..end].to_string()
    }

    fn trim(&mut self) {
        *self = self.trim_end_matches(matches_fluent_ws).to_string();
    }
}

impl<'s> Slice<'s> for &'s str {
    fn slice(&self, range: Range<usize>) -> Self {
        // PATCH(bevy_markup): see the `String` impl above.
        let start = clamp_boundary(self, range.start);
        let end = clamp_boundary(self, range.end);
        &self[start..end]
    }

    fn trim(&mut self) {
        *self = self.trim_end_matches(matches_fluent_ws);
    }
}

// PATCH(bevy_markup): nearest char boundary at or before `index` (see `Slice::slice`).
fn clamp_boundary(source: &str, index: usize) -> usize {
    let index = index.min(source.len());
    if source.is_char_boundary(index) {
        index
    } else {
        (index..).find(|&i| source.is_char_boundary(i)).unwrap_or(source.len())
    }
}
