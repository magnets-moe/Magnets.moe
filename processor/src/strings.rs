use std::{
    borrow::Borrow,
    hash::{Hash, Hasher},
    ops::{Deref, Range},
    sync::Arc,
};

pub struct StringLists {
    buf: String,
    strings: Vec<Range<usize>>,
    lists: Vec<Range<usize>>,
}

impl StringLists {
    pub fn new() -> Self {
        let mut lists = Vec::with_capacity(10000);
        lists.push(0..0);
        Self {
            buf: String::with_capacity(10000),
            strings: Vec::with_capacity(10000),
            lists,
        }
    }

    pub fn shrink_to_fit(&mut self) {
        self.buf.shrink_to_fit();
        self.strings.shrink_to_fit();
        self.lists.shrink_to_fit();
    }

    pub fn push_str(&mut self, s: &str) {
        let start = self.buf.len();
        self.buf.push_str(s);
        self.strings.push(start..self.buf.len());
        self.lists.last_mut().unwrap().end += 1;
    }

    pub fn finish_list(&mut self) -> usize {
        let pos = self.lists.len() - 1;
        self.lists.push(self.strings.len()..self.strings.len());
        pos
    }

    pub fn iter(&self, idx: usize) -> impl Iterator<Item = &str> {
        self.strings[self.lists[idx].clone()]
            .iter()
            .map(move |r| &self.buf[r.clone()])
    }
}

#[derive(Eq)]
pub struct ArcString {
    buf: Arc<str>,
    range: Range<usize>,
}

impl PartialEq<ArcString> for ArcString {
    fn eq(&self, other: &ArcString) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Hash for ArcString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl ArcString {
    pub fn new(buf: String) -> Self {
        Self {
            range: 0..buf.len(),
            buf: Arc::from(buf.into_boxed_str()),
        }
    }

    pub fn substring(&self, index: Range<usize>) -> ArcString {
        assert!(index.start <= index.end);
        assert!(index.end <= self.range.len());
        ArcString {
            buf: self.buf.clone(),
            range: self.range.start + index.start..self.range.start + index.end,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.buf[self.range.clone()]
    }
}

impl Borrow<str> for ArcString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Deref for ArcString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}
